use std::fmt;
use std::pin::Pin;

use futures::Stream;

use super::pod::PodRow;

/// A change in the set of pods a source observes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodEvent {
    /// A pod was added or modified.
    Applied(PodRow),
    /// A pod disappeared.
    Deleted { namespace: String, name: String },
    /// The source (re)listed: this is the complete current set of pods.
    Restarted(Vec<PodRow>),
}

/// A recoverable failure reported by a source. Sources keep retrying after
/// yielding one; the stream only ends when the source shuts down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    message: String,
}

impl SourceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

/// Stream of pod events produced by a [`PodSource`].
pub type PodEventStream = Pin<Box<dyn Stream<Item = Result<PodEvent, SourceError>> + Send>>;

/// Port: anything that can watch pods across all namespaces.
pub trait PodSource: Send + Sync {
    /// Starts watching. The first successful item is a [`PodEvent::Restarted`]
    /// with the initial listing.
    fn watch(&self) -> PodEventStream;
}
