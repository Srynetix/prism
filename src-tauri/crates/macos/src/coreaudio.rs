//! CoreAudio's property API (`AudioObjectGetPropertyData` and the like): a property is read by
//! its address (the four-letter codes of `AudioHardware.h`, which the caller knows); one an
//! object lacks answers `None`. Values are plain numbers, object id lists and strings only.

use std::ffi::c_void;
use std::sync::Once;

pub type ObjectId = u32;
type Status = i32;

/// The system object, which holds the devices and the processes.
pub const SYSTEM: ObjectId = 1;

/// A property of an object: its selector, scope and element.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Address {
    pub selector: u32,
    pub scope: u32,
    pub element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectHasProperty(object: ObjectId, address: *const Address) -> u8;
    fn AudioObjectIsPropertySettable(object: ObjectId, address: *const Address, settable: *mut u8) -> Status;
    fn AudioObjectGetPropertyDataSize(object: ObjectId, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32) -> Status;
    fn AudioObjectGetPropertyData(object: ObjectId, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32, data: *mut c_void) -> Status;
    fn AudioObjectSetPropertyData(object: ObjectId, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: u32, data: *const c_void) -> Status;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringGetLength(s: *const c_void) -> isize;
    fn CFStringGetCString(s: *const c_void, buffer: *mut u8, size: isize, encoding: u32) -> u8;
    fn CFRelease(object: *const c_void);
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for u32 {}
    impl Sealed for i32 {}
    impl Sealed for f32 {}
}

/// The values a property is read or written as: numbers any bit pattern of which is valid.
pub trait Value: sealed::Sealed + Copy + Default {}
impl Value for u32 {}
impl Value for i32 {}
impl Value for f32 {}

pub fn has(object: ObjectId, a: &Address) -> bool {
    // SAFETY: a plain query on a valid address struct.
    unsafe { AudioObjectHasProperty(object, a) != 0 }
}

pub fn settable(object: ObjectId, a: &Address) -> bool {
    let mut out = 0u8;
    // SAFETY: `out` outlives the call.
    unsafe { AudioObjectIsPropertySettable(object, a, &mut out) == 0 && out != 0 }
}

/// The size of a property's value in bytes, `None` if the object has none.
pub fn size(object: ObjectId, a: &Address) -> Option<u32> {
    let mut size = 0u32;
    // SAFETY: a size query into a local.
    (unsafe { AudioObjectGetPropertyDataSize(object, a, 0, std::ptr::null(), &mut size) } == 0).then_some(size)
}

/// A property of a fixed size.
pub fn get<T: Value>(object: ObjectId, a: &Address) -> Option<T> {
    if !has(object, a) {
        return None;
    }
    let mut value = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    // SAFETY: `value` is `size` bytes, written by CoreAudio at most that much; any bit pattern is
    // a valid `T` (`Value`).
    let status = unsafe { AudioObjectGetPropertyData(object, a, 0, std::ptr::null(), &mut size, (&raw mut value).cast()) };
    (status == 0).then_some(value)
}

/// Writes a property; the status CoreAudio answers when it refuses.
pub fn set<T: Value>(object: ObjectId, a: &Address, value: T) -> Result<(), i32> {
    // SAFETY: `value` is `size_of::<T>()` bytes, read by CoreAudio.
    let status = unsafe { AudioObjectSetPropertyData(object, a, 0, std::ptr::null(), std::mem::size_of::<T>() as u32, (&raw const value).cast()) };
    if status == 0 { Ok(()) } else { Err(status) }
}

/// A property that is a list of object ids (the devices, the processes).
pub fn ids(object: ObjectId, a: &Address) -> Vec<ObjectId> {
    let Some(mut size) = size(object, a).filter(|s| *s > 0) else { return Vec::new() };
    let mut list = vec![0 as ObjectId; size as usize / std::mem::size_of::<ObjectId>()];
    size = (list.len() * std::mem::size_of::<ObjectId>()) as u32;
    // SAFETY: `list` holds `size` bytes, written by CoreAudio at most that much.
    if unsafe { AudioObjectGetPropertyData(object, a, 0, std::ptr::null(), &mut size, list.as_mut_ptr().cast()) } != 0 {
        return Vec::new();
    }
    list.truncate(size as usize / std::mem::size_of::<ObjectId>());
    list
}

/// A CFString property, released once copied.
pub fn string(object: ObjectId, a: &Address) -> Option<String> {
    if !has(object, a) {
        return None;
    }
    let mut cf: *const c_void = std::ptr::null();
    let mut size = std::mem::size_of::<*const c_void>() as u32;
    // SAFETY: `cf` is a pointer's size, written by CoreAudio at most that much.
    let status = unsafe { AudioObjectGetPropertyData(object, a, 0, std::ptr::null(), &mut size, (&raw mut cf).cast()) };
    if status != 0 || size as usize != std::mem::size_of::<*const c_void>() || cf.is_null() {
        return None;
    }
    // SAFETY: CoreAudio hands a retained CFString we own and release; the buffer is sized for
    // UTF-8's worst case and NUL-terminated by CFStringGetCString.
    unsafe {
        let len = CFStringGetLength(cf) * 4 + 1;
        let mut buf = vec![0u8; len as usize];
        let ok = CFStringGetCString(cf, buf.as_mut_ptr(), len, 0x0800_0100) != 0;
        CFRelease(cf);
        ok.then(|| String::from_utf8_lossy(&buf[..buf.iter().position(|b| *b == 0).unwrap_or(0)]).into_owned())
    }
}

/// The HAL sends its notifications on a thread of its own (`kAudioHardwarePropertyRunLoop` set to
/// none), so the device list follows a headset plugged in without Prism's run loop (Apple's
/// advice for a process that polls). Once.
pub fn notify_on_own_thread() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let a = Address { selector: u32::from_be_bytes(*b"rnlp"), scope: u32::from_be_bytes(*b"glob"), element: 0 };
        let none: *const c_void = std::ptr::null();
        // SAFETY: the property is a CFRunLoopRef, set to NULL: the size of a pointer, read by CoreAudio.
        let _ = unsafe { AudioObjectSetPropertyData(SYSTEM, &a, 0, std::ptr::null(), std::mem::size_of::<*const c_void>() as u32, (&raw const none).cast()) };
    });
}
