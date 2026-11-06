//! The Dock's menu of the app (a right click on its icon): items of the app's own below the windows
//! macOS lists there. AppKit asks the application's delegate for it (`applicationDockMenu:`), a
//! method Tauri's delegate has not: it is added to its class, once, with the one an item calls.

use std::ffi::CStr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{class, msg_send, sel, MainThreadMarker};
use objc2_foundation::NSString;

/// What an item says when picked: its key.
static PICKED: OnceLock<fn(String)> = OnceLock::new();
/// The menu AppKit is given, retained here; null, none.
static MENU: AtomicPtr<AnyObject> = AtomicPtr::new(std::ptr::null_mut());

/// An item of the Dock's menu.
pub struct Item {
    /// What `picked` is told.
    pub key: String,
    pub title: String,
    /// A check mark before it.
    pub checked: bool,
}

/// `applicationDockMenu:`: the menu kept here (AppKit retains what it shows).
extern "C-unwind" fn dock_menu(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject) -> *mut AnyObject {
    MENU.load(Ordering::Acquire)
}

/// `prismDockPick:`: an item picked, its key told.
extern "C-unwind" fn pick(_this: *mut AnyObject, _cmd: Sel, item: *mut AnyObject) {
    if item.is_null() {
        return;
    }
    // SAFETY: `item` is the NSMenuItem AppKit calls the action with; its represented object is
    // the NSString set below (or nil, checked).
    let key = unsafe {
        let key: *mut AnyObject = msg_send![item, representedObject];
        if key.is_null() {
            return;
        }
        (*(key as *const NSString)).to_string()
    };
    if let Some(f) = PICKED.get() {
        f(key);
    }
}

/// The two methods added to the delegate's class; false if it has no delegate yet.
fn install(delegate: *mut AnyObject) -> bool {
    static DONE: OnceLock<bool> = OnceLock::new();
    *DONE.get_or_init(|| {
        // SAFETY: the delegate is a live object of the app; its class gains two methods whose
        // implementations have the types given (`@@:@`: an object from (self, _cmd, an object);
        // `v@:@`: nothing). Added only where it has none of that name (class_addMethod refuses).
        unsafe {
            let cls: *const AnyClass = msg_send![delegate, class];
            if cls.is_null() {
                return false;
            }
            let menu: Imp = std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> *mut AnyObject, Imp>(dock_menu);
            let act: Imp = std::mem::transmute::<extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject), Imp>(pick);
            let types_menu: &CStr = c"@@:@";
            let types_act: &CStr = c"v@:@";
            objc2::ffi::class_addMethod(cls as *mut AnyClass, sel!(applicationDockMenu:), menu, types_menu.as_ptr());
            objc2::ffi::class_addMethod(cls as *mut AnyClass, sel!(prismDockPick:), act, types_act.as_ptr());
        }
        true
    })
}

/// The Dock's menu, as `items` say (none: no menu of the app's), an item picked told to `picked`
/// (on the main thread). False off the main thread or before the app has its delegate.
pub fn set_menu(items: &[Item], picked: fn(String)) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    let _ = PICKED.set(picked);
    // SAFETY: AppKit calls on the main thread (checked above), on the shared application and
    // objects made here; the menu built is retained (+1 from alloc/init) and kept in `MENU`, the
    // former one released once replaced. The items are retained by the menu, released here.
    unsafe {
        let ns_app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let delegate: *mut AnyObject = msg_send![ns_app, delegate];
        if delegate.is_null() || !install(delegate) {
            return false;
        }
        let menu: *mut AnyObject = if items.is_empty() {
            std::ptr::null_mut()
        } else {
            let title = NSString::from_str("");
            let menu: *mut AnyObject = msg_send![class!(NSMenu), alloc];
            let menu: *mut AnyObject = msg_send![menu, initWithTitle: &*title];
            let none = NSString::from_str("");
            for i in items {
                let label = NSString::from_str(&i.title);
                let item: *mut AnyObject = msg_send![class!(NSMenuItem), alloc];
                let item: *mut AnyObject = msg_send![item, initWithTitle: &*label, action: sel!(prismDockPick:), keyEquivalent: &*none];
                let key = NSString::from_str(&i.key);
                let _: () = msg_send![item, setRepresentedObject: &*key];
                let _: () = msg_send![item, setTarget: delegate];
                let _: () = msg_send![item, setState: if i.checked { 1isize } else { 0isize }];
                let _: () = msg_send![menu, addItem: item];
                let _: () = msg_send![item, release];
            }
            menu
        };
        let old = MENU.swap(menu, Ordering::AcqRel);
        if !old.is_null() {
            let _: () = msg_send![old, release];
        }
    }
    true
}
