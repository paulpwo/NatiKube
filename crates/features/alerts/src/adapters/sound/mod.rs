//! System alert sounds: an implementation of the
//! [`SoundPlayer`](crate::domain::SoundPlayer) port with the platform's own
//! API (`NSSound` on macOS, `PlaySoundW` aliases on Windows). No audio crate.
//! `play` never blocks the caller. Other platforms stay silent.

use crate::domain::{SoundError, SoundPlayer};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::SystemSoundPlayer;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::SystemSoundPlayer;

/// Linux has no backend yet.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemSoundPlayer;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl SoundPlayer for SystemSoundPlayer {
    fn play(&self, _name: &str) -> Result<(), SoundError> {
        Err(SoundError(
            "System sounds are not supported on this platform yet".into(),
        ))
    }
}

/// Never plays anything.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopSoundPlayer;

impl SoundPlayer for NoopSoundPlayer {
    fn play(&self, _name: &str) -> Result<(), SoundError> {
        Ok(())
    }
}
