//! Windows: `PlaySoundW` with a system sound alias (`SystemNotification`,
//! `SystemAsterisk`, ...), asynchronous so it never blocks the caller.

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NODEFAULT};

use crate::domain::{SoundError, SoundPlayer};

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemSoundPlayer;

impl SoundPlayer for SystemSoundPlayer {
    fn play(&self, name: &str) -> Result<(), SoundError> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the
        // call; a null module handle is valid with `SND_ALIAS`. `SND_NODEFAULT`
        // keeps Windows from beeping when the alias is unknown.
        let played = unsafe {
            PlaySoundW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
            )
        };
        if played == 0 {
            return Err(SoundError(format!("Windows could not play {name}")));
        }
        Ok(())
    }
}
