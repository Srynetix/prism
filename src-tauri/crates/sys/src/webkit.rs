//! The WebKitGTK library loaded.

/// Its major and minor version, readable before GTK starts.
pub fn version() -> (u32, u32) {
    // SAFETY: constants of the loaded library, with no argument.
    unsafe { (webkit2gtk::ffi::webkit_get_major_version(), webkit2gtk::ffi::webkit_get_minor_version()) }
}
