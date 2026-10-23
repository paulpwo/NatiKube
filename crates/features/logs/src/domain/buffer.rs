use std::collections::VecDeque;

use super::line::{LogLine, split_timestamp, strip_ansi};

/// Bounded line buffer (ring): once `capacity` lines are stored, the oldest
/// is evicted for every new one. Raw chunks may split lines anywhere; the
/// incomplete tail is carried over until its newline arrives.
#[derive(Debug)]
pub struct LogBuffer {
    capacity: usize,
    lines: VecDeque<LogLine>,
    next_seq: u64,
    evicted: u64,
    carry: Vec<u8>,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            lines: VecDeque::new(),
            next_seq: 0,
            evicted: 0,
            carry: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Number of lines dropped because the buffer was full.
    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    pub fn get(&self, index: usize) -> Option<&LogLine> {
        self.lines.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &LogLine> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.carry.clear();
    }

    /// Appends one complete line (as produced by the API, timestamp prefix
    /// included when requested).
    pub fn push_line(&mut self, raw: &str) {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let (timestamp, text) = split_timestamp(raw);
        let line = LogLine {
            seq: self.next_seq,
            timestamp: timestamp.map(str::to_string),
            text: strip_ansi(text),
        };
        self.next_seq += 1;
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
            self.evicted += 1;
        }
        self.lines.push_back(line);
    }

    /// Appends raw bytes; returns how many complete lines were added.
    pub fn push_bytes(&mut self, chunk: &[u8]) -> usize {
        let mut added = 0;
        let mut rest = chunk;
        while let Some(pos) = rest.iter().position(|b| *b == b'\n') {
            self.carry.extend_from_slice(&rest[..pos]);
            let line = String::from_utf8_lossy(&self.carry).into_owned();
            self.carry.clear();
            self.push_line(&line);
            added += 1;
            rest = &rest[pos + 1..];
        }
        self.carry.extend_from_slice(rest);
        added
    }

    /// Renders every stored line, one per row, for saving to a file.
    pub fn to_text(&self, show_timestamps: bool) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.display(show_timestamps));
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_when_full() {
        let mut buf = LogBuffer::new(3);
        for i in 0..5 {
            buf.push_line(&format!("line {i}"));
        }
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.evicted(), 2);
        assert_eq!(buf.get(0).unwrap().text, "line 2");
        assert_eq!(buf.get(2).unwrap().seq, 4);
    }

    #[test]
    fn chunks_may_split_lines_and_utf8() {
        let mut buf = LogBuffer::new(10);
        let bytes = "héllo\nwörld\n".as_bytes();
        // Split in the middle of the two-byte 'é'.
        assert_eq!(buf.push_bytes(&bytes[..2]), 0);
        assert_eq!(buf.push_bytes(&bytes[2..9]), 1);
        assert_eq!(buf.push_bytes(&bytes[9..]), 1);
        assert_eq!(buf.get(0).unwrap().text, "héllo");
        assert_eq!(buf.get(1).unwrap().text, "wörld");
    }

    #[test]
    fn partial_tail_is_not_a_line() {
        let mut buf = LogBuffer::new(10);
        buf.push_bytes(b"done\npartial");
        assert_eq!(buf.len(), 1);
        buf.push_bytes(b" rest\n");
        assert_eq!(buf.get(1).unwrap().text, "partial rest");
    }

    #[test]
    fn strips_carriage_return_and_ansi() {
        let mut buf = LogBuffer::new(10);
        buf.push_bytes(b"\x1b[32mok\x1b[0m\r\n");
        assert_eq!(buf.get(0).unwrap().text, "ok");
    }

    #[test]
    fn parses_timestamps_and_renders_text() {
        let mut buf = LogBuffer::new(10);
        buf.push_line("2024-05-01T10:00:00Z first");
        buf.push_line("no timestamp");
        assert_eq!(
            buf.to_text(true),
            "2024-05-01T10:00:00Z first\nno timestamp\n"
        );
        assert_eq!(buf.to_text(false), "first\nno timestamp\n");
    }

    #[test]
    fn memory_stays_bounded() {
        let mut buf = LogBuffer::new(100);
        for i in 0..10_000 {
            buf.push_line(&format!("line {i}"));
        }
        assert_eq!(buf.len(), 100);
        assert_eq!(buf.evicted(), 9_900);
    }
}
