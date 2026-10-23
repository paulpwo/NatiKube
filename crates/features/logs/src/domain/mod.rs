//! Pure log model: bounded line buffer, search matches, timestamp toggle.

mod buffer;
mod line;
mod search;

pub use buffer::LogBuffer;
pub use line::{LogLine, split_timestamp, strip_ansi};
pub use search::{LogSearch, find_ranges};

/// Default number of lines requested from the API (`tailLines`).
pub const DEFAULT_TAIL_LINES: i64 = 1000;
/// Default capacity of the in-memory ring buffer.
pub const DEFAULT_CAPACITY: usize = 50_000;
