//! In-app toasts: a short stack in a corner of the main window when an alert
//! fires. State is the pure `ToastQueue` (domain); `view` is the GPUI host.

mod view;

pub use view::ToastHost;
