use std::ops::Range;

use super::buffer::LogBuffer;

/// Case-insensitive (by default) substring search over the buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogSearch {
    query: String,
    case_sensitive: bool,
}

impl LogSearch {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            case_sensitive: false,
        }
    }

    pub fn with_case_sensitive(mut self, on: bool) -> Self {
        self.case_sensitive = on;
        self
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn is_empty(&self) -> bool {
        self.query.is_empty()
    }

    /// Byte ranges of every match in `text`.
    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        find_ranges(text, &self.query, self.case_sensitive)
    }

    /// Indexes (into the buffer) of lines that match, using the displayed text.
    pub fn matching_lines(&self, buffer: &LogBuffer, show_timestamps: bool) -> Vec<usize> {
        if self.is_empty() {
            return Vec::new();
        }
        buffer
            .iter()
            .enumerate()
            .filter(|(_, line)| !self.ranges(&line.display(show_timestamps)).is_empty())
            .map(|(i, _)| i)
            .collect()
    }
}

/// Non-overlapping byte ranges of `needle` in `haystack`. Case folding is
/// ASCII-only so byte offsets stay valid.
pub fn find_ranges(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let (hay, ndl);
    let (hay, ndl) = if case_sensitive {
        (haystack, needle)
    } else {
        hay = haystack.to_ascii_lowercase();
        ndl = needle.to_ascii_lowercase();
        (hay.as_str(), ndl.as_str())
    };
    hay.match_indices(ndl)
        .map(|(start, m)| start..start + m.len())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_occurrences_case_insensitively() {
        let r = find_ranges("Error: error ERROR", "error", false);
        assert_eq!(r, vec![0..5, 7..12, 13..18]);
    }

    #[test]
    fn case_sensitive_only_exact() {
        assert_eq!(find_ranges("Error error", "error", true), vec![6..11]);
    }

    #[test]
    fn empty_needle_matches_nothing() {
        assert!(find_ranges("abc", "", false).is_empty());
    }

    #[test]
    fn ranges_are_valid_char_boundaries() {
        let text = "ñandú Ñandú";
        for r in find_ranges(text, "andú", false) {
            assert!(text.get(r).is_some());
        }
    }

    #[test]
    fn matching_lines_uses_displayed_text() {
        let mut buf = LogBuffer::new(10);
        buf.push_line("2024-05-01T10:00:00Z alpha");
        buf.push_line("2024-05-01T10:00:01Z beta");
        let search = LogSearch::new("2024-05-01T10:00:01");
        assert_eq!(search.matching_lines(&buf, true), vec![1]);
        assert!(search.matching_lines(&buf, false).is_empty());
        assert_eq!(LogSearch::new("ALPHA").matching_lines(&buf, false), vec![0]);
    }
}
