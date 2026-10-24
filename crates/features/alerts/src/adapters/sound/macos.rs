//! macOS: `NSSound` named sounds (`/System/Library/Sounds`).
//!
//! The first `NSSound` call loads AppKit's audio stack and took about 200 ms
//! in a release build, so it never runs on the caller's (UI) thread: `play`
//! only queues the name for a worker thread that owns the sounds and returns
//! at once.

use std::cell::RefCell;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use objc2::rc::Retained;
use objc2_app_kit::NSSound;
use objc2_foundation::NSString;

use crate::domain::{SoundError, SoundPlayer};

thread_local! {
    /// The sound being played: AppKit stops a sound whose last reference
    /// goes away, so the worker keeps it until the next one.
    static PLAYING: RefCell<Option<Retained<NSSound>>> = const { RefCell::new(None) };
}

/// Plays `name` on the current thread.
fn play_now(name: &str) -> Result<(), SoundError> {
    let Some(sound) = NSSound::soundNamed(&NSString::from_str(name)) else {
        return Err(SoundError(format!("There is no system sound named {name}")));
    };
    // A named sound is shared: restart it if it is still playing.
    sound.stop();
    if !sound.play() {
        return Err(SoundError(format!("macOS could not play {name}")));
    }
    PLAYING.with(|slot| *slot.borrow_mut() = Some(sound));
    Ok(())
}

/// Result of the most recent play, for the dev check.
type LastResult = Arc<Mutex<Option<Result<(), SoundError>>>>;

#[derive(Debug, Default)]
pub struct SystemSoundPlayer {
    worker: OnceLock<Sender<String>>,
    last: LastResult,
}

impl SystemSoundPlayer {
    pub fn new() -> Self {
        Self::default()
    }

    fn worker(&self) -> Option<&Sender<String>> {
        if let Some(tx) = self.worker.get() {
            return Some(tx);
        }
        let (tx, rx) = channel::<String>();
        let last = Arc::clone(&self.last);
        std::thread::Builder::new()
            .name("natikube-alert-sound".into())
            .spawn(move || {
                while let Ok(name) = rx.recv() {
                    let result = play_now(&name);
                    if let Err(error) = &result {
                        tracing::warn!(%error, "could not play the alert sound");
                    }
                    if let Ok(mut slot) = last.lock() {
                        *slot = Some(result);
                    }
                }
            })
            .ok()?;
        Some(self.worker.get_or_init(|| tx))
    }

    /// What the worker answered to the last sound (dev check).
    pub fn last_result(&self) -> Option<Result<(), SoundError>> {
        self.last.lock().ok().and_then(|slot| slot.clone())
    }
}

impl SoundPlayer for SystemSoundPlayer {
    fn play(&self, name: &str) -> Result<(), SoundError> {
        let queued = self
            .worker()
            .is_some_and(|tx| tx.send(name.to_string()).is_ok());
        if queued {
            Ok(())
        } else {
            Err(SoundError("The sound thread is not running".into()))
        }
    }
}
