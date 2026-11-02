//! AppKit: the running applications, the Dock icon, the accessibility settings.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{class, msg_send, MainThreadMarker};
use objc2_foundation::{NSData, NSString};

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

/// The app's Dock icon from a PNG; false off the main thread or for an unreadable image.
pub fn set_app_icon(png: &[u8]) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    let data = NSData::with_bytes(png);
    // SAFETY: AppKit calls on the main thread (checked above); the image stays alive (the app
    // keeps it, never released here).
    unsafe {
        let image: *mut AnyObject = msg_send![class!(NSImage), alloc];
        let image: *mut AnyObject = msg_send![image, initWithData: &*data];
        if image.is_null() {
            return false;
        }
        let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![ns_app, setApplicationIconImage: image];
    }
    true
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
