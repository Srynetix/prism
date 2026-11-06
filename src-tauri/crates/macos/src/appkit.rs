//! AppKit: the running applications, the accessibility settings.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{class, msg_send, MainThreadMarker};
use objc2_foundation::NSString;

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

/// The name of the application with bundle id `bundle`, else of process `pid`.
pub fn running_app_name(bundle: &str, pid: i32) -> Option<String> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"NSRunningApplication")?;
        // SAFETY: NSRunningApplication's documented class methods and getter; nil results checked.
        unsafe {
            let id = NSString::from_str(bundle);
            let apps: *mut AnyObject = msg_send![class, runningApplicationsWithBundleIdentifier: &*id];
            let mut app: *mut AnyObject = if apps.is_null() { std::ptr::null_mut() } else { msg_send![apps, firstObject] };
            if app.is_null() {
                app = msg_send![class, runningApplicationWithProcessIdentifier: pid];
            }
            if app.is_null() {
                return None;
            }
            let name: Option<Retained<NSString>> = msg_send![app, localizedName];
            name.map(|n| n.to_string()).filter(|n| !n.is_empty())
        }
    })
}

/// Whether Accessibility › Display asks to reduce transparency.
pub fn reduce_transparency() -> bool {
    // SAFETY: a read-only property of the shared workspace, which AppKit keeps alive.
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return false;
        }
        msg_send![workspace, accessibilityDisplayShouldReduceTransparency]
    }
}

/// `NSEventMaskFlagsChanged`: a modifier key pressed or released.
const FLAGS_CHANGED: u64 = 1 << 12;
/// `NSEventModifierFlagFunction`.
const FUNCTION: u64 = 1 << 23;
/// `kVK_Function`, and the Globe key of recent keyboards.
const FN_KEYS: [u16; 2] = [63, 179];

/// Calls `on` with `true` when the Fn (🌐) key goes down and `false` when it comes up, while Prism
/// is the active application (a local monitor: other apps' keys are never seen). The event goes
/// on as it came. On the main thread only (`false` elsewhere); the monitor lasts as long as the app.
pub fn watch_fn_key(on: impl Fn(bool) + 'static) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    let handler = block2::RcBlock::new(move |event: *mut AnyObject| -> *mut AnyObject {
        if !event.is_null() {
            // SAFETY: AppKit hands a live NSEvent of the mask asked (flags changed), whose
            // `keyCode` and `modifierFlags` are readable.
            let (code, flags): (u16, u64) = unsafe { (msg_send![event, keyCode], msg_send![event, modifierFlags]) };
            if FN_KEYS.contains(&code) {
                on(flags & FUNCTION != 0);
            }
        }
        event
    });
    // SAFETY: NSEvent's documented class method, on the main thread (checked above), with a block of
    // the signature it calls (copied by AppKit). The monitor it returns is kept for the app's life.
    unsafe {
        let monitor: *mut AnyObject = msg_send![class!(NSEvent), addLocalMonitorForEventsMatchingMask: FLAGS_CHANGED, handler: &*handler];
        if monitor.is_null() {
            return false;
        }
        let _: *mut AnyObject = msg_send![monitor, retain];
    }
    true
}

/// What the Fn (🌐) key does by itself, in System Settings › Keyboard (`AppleFnUsageType`): 0
/// nothing, 1 change the input source, 2 show emoji, 3 start dictation. macOS acts on it too while
/// Prism listens: anything but 0 is worth telling.
pub fn fn_key_usage() -> Option<u32> {
    let out = std::process::Command::new("/usr/bin/defaults").args(["read", "com.apple.HIToolbox", "AppleFnUsageType"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}
