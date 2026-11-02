//! macOS's kernel values read by name.

use std::ffi::CStr;

/// The integer sysctl `name` (`kern.memorystatus_vm_pressure_level`), `None` if unknown.
pub fn int(name: &CStr) -> Option<i32> {
    let mut value: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    // SAFETY: a read-only sysctl into a local int of the size given; `name` is NUL-terminated.
    let ok = unsafe { libc::sysctlbyname(name.as_ptr(), (&raw mut value).cast(), &mut size, std::ptr::null_mut(), 0) } == 0;
    (ok && size == std::mem::size_of::<libc::c_int>()).then_some(value)
}
