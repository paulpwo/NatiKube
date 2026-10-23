//! Runtime Dock icon for the bare binary (`cargo run`, a copied executable).
//! A bundled `NatiKube.app` already has its icon from `Info.plist`; setting it
//! again here is harmless and makes the unbundled binary look right too.

/// The 256 px app icon, embedded in the binary.
#[cfg(target_os = "macos")]
const ICON_PNG: &[u8] = include_bytes!("../../../assets/icon/natikube-256.png");

/// Sets the application (Dock) icon. Does nothing off macOS: Windows takes the
/// icon from the executable resource.
#[cfg(target_os = "macos")]
pub(crate) fn set_dock_icon() {
    use objc2::AllocAnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // GPUI runs the application closure on the main thread; if that ever
    // changes the icon is simply skipped.
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(ICON_PNG);
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: called on the main thread with a valid image.
        unsafe { app.setApplicationIconImage(Some(&image)) };
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn set_dock_icon() {}
