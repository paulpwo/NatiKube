//! The few Lucide icons the app draws, embedded on their own.
//!
//! Registering the whole catalog would add every SVG to the binary; this
//! source carries only the icons a widget asks for (the copy button and its
//! check). Pass it to `Application::with_assets` before the first window.

gpui_kit::assets::icon_assets!(pub Icons, [Copy, Check]);
