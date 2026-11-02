//! The Wi-Fi link, through CoreWLAN: signal and rate, never the network's name (it needs Location).

use objc2::msg_send;
use objc2::runtime::{AnyClass, AnyObject};

#[link(name = "CoreWLAN", kind = "framework")]
unsafe extern "C" {}

/// The signal (RSSI, dBm) and transmit rate (Mbit/s) of the Wi-Fi interface; `None` without one
/// or when it is not associated.
pub fn link() -> Option<(i64, f64)> {
    objc2::rc::autoreleasepool(|_| {
        let class = AnyClass::get(c"CWWiFiClient")?;
        // SAFETY: CoreWLAN's documented getters, on objects it owns (autoreleased in the pool);
        // a nil interface (no Wi-Fi) is checked before use.
        unsafe {
            let client: *mut AnyObject = msg_send![class, sharedWiFiClient];
            if client.is_null() {
                return None;
            }
            let interface: *mut AnyObject = msg_send![client, interface];
            if interface.is_null() {
                return None;
            }
            let rssi: isize = msg_send![interface, rssiValue];
            let rate: f64 = msg_send![interface, transmitRate];
            // Not associated: no signal to show.
            (rssi != 0).then_some((rssi as i64, rate))
        }
    })
}
