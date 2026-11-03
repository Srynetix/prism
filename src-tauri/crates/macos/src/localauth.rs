//! The person at the Mac, confirmed by LocalAuthentication: Touch ID, or the Mac's password where
//! there is none (or it is locked out), as macOS asks it in its own sheet.

use std::{sync::mpsc, time::Duration};

use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2_foundation::{NSError, NSString};

#[link(name = "LocalAuthentication", kind = "framework")]
unsafe extern "C" {}

/// `LAPolicyDeviceOwnerAuthentication`: biometrics, or the account's password.
const DEVICE_OWNER: isize = 2;

/// Asks the person at the Mac to confirm, macOS showing `reason` ("Prism wants to …"), and waits
/// for the answer up to `wait`: whether they did. Blocks: call it off the main thread.
pub fn confirm(reason: &str, wait: Duration) -> bool {
    let Some(class) = AnyClass::get(c"LAContext") else { return false };
    let (tx, rx) = mpsc::channel();
    let reply = RcBlock::new(move |ok: Bool, _error: *mut NSError| {
        let _ = tx.send(ok.as_bool());
    });
    let reason = NSString::from_str(reason);
    // SAFETY: LAContext's designated initializer, a class method with no argument.
    let context: Retained<AnyObject> = unsafe { msg_send![class, new] };
    // SAFETY: its documented `evaluatePolicy:localizedReason:reply:` with a policy it defines, a
    // string, and a block of the signature it calls (copied by LocalAuthentication, so it may
    // outlive this frame); the context is kept alive until the answer came or the wait ended.
    unsafe {
        let _: () = msg_send![&*context, evaluatePolicy: DEVICE_OWNER, localizedReason: &*reason, reply: &*reply];
    }
    let ok = rx.recv_timeout(wait).unwrap_or(false);
    if !ok {
        // SAFETY: a method with no argument: the sheet closes if it is still up.
        unsafe {
            let _: () = msg_send![&*context, invalidate];
        }
    }
    ok
}
