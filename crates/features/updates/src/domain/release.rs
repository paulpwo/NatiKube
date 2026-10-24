//! Release model and the pure rules that decide whether an update is offered.

use semver::Version;

/// One published GitHub release, reduced to what the modal needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// The tag as published (`v0.2.0-alpha.1`).
    pub tag: String,
    pub version: Version,
    /// Release title; `None` when GitHub has none.
    pub name: Option<String>,
    /// Release notes (Markdown source).
    pub notes: String,
    /// The release page, opened by "Download".
    pub url: String,
}

/// A newer release together with the running version it is compared against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOffer {
    pub current: String,
    pub release: Release,
    /// Injected by `--fake-update`: "Skip this version" must not be persisted.
    pub simulated: bool,
}

/// Parses a release tag: `v0.2.0` or `0.2.0`, optionally with a pre-release
/// suffix. Anything else (`nightly`, `v1`) is not a version and is ignored.
pub fn parse_tag(tag: &str) -> Option<Version> {
    let trimmed = tag.trim();
    let digits = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    Version::parse(digits).ok()
}

/// The release with the highest semver precedence. Pre-releases count (every
/// NatiKube release is one during the alpha); the caller has already dropped
/// drafts and unparseable tags.
pub fn newest_release(releases: impl IntoIterator<Item = Release>) -> Option<Release> {
    releases
        .into_iter()
        .max_by(|a, b| a.version.cmp_precedence(&b.version))
}

/// The release to offer, if any: the newest one, strictly newer than
/// `current`, and not at or below the version the user chose to skip.
pub fn available_update(
    current: &str,
    releases: impl IntoIterator<Item = Release>,
    skipped: Option<&str>,
) -> Option<Release> {
    let current = Version::parse(current).ok()?;
    let newest = newest_release(releases)?;
    if newest.version.cmp_precedence(&current).is_le() {
        return None;
    }
    let skipped = skipped.and_then(parse_tag);
    if skipped.is_some_and(|skipped| newest.version.cmp_precedence(&skipped).is_le()) {
        return None;
    }
    Some(newest)
}

/// A short plain-text excerpt of Markdown release notes: formatting markers
/// are dropped, blank lines collapse, and the text is cut at `max_chars`
/// (on a character boundary) with an ellipsis.
pub fn notes_excerpt(notes: &str, max_chars: usize) -> String {
    let lines: Vec<String> = notes
        .lines()
        .map(plain_line)
        .filter(|line| !line.is_empty())
        .collect();
    let text = lines.join("\n");
    if text.chars().count() <= max_chars {
        return text;
    }
    let cut: String = text.chars().take(max_chars).collect();
    format!("{}…", cut.trim_end())
}

/// One Markdown line as plain text: heading hashes, bullets, emphasis, code
/// ticks and `[text](url)` links reduced to their words.
fn plain_line(line: &str) -> String {
    let line = line.trim();
    let line = line.trim_start_matches('#').trim_start();
    let line = match line.strip_prefix(['-', '*', '+']) {
        Some(rest) if rest.starts_with(' ') => rest.trim_start(),
        _ => line,
    };
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        let after = &rest[c.len_utf8()..];
        match c {
            '*' | '_' | '`' | '~' => rest = after,
            '[' => match link_parts(after) {
                // `[text](url)` keeps `text`; a bare `[` stays.
                Some((text, tail)) => {
                    out.push_str(text);
                    rest = tail;
                }
                None => {
                    out.push(c);
                    rest = after;
                }
            },
            other => {
                out.push(other);
                rest = after;
            }
        }
    }
    out.trim().to_string()
}

/// Splits `text](url) tail` into `(text, tail)`.
fn link_parts(after_bracket: &str) -> Option<(&str, &str)> {
    let close = after_bracket.find("](")?;
    let url_start = close + 2;
    let end = after_bracket[url_start..].find(')')?;
    Some((
        &after_bracket[..close],
        &after_bracket[url_start + end + 1..],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag: tag.to_string(),
            version: parse_tag(tag).expect("test tag parses"),
            name: None,
            notes: String::new(),
            url: format!("https://github.com/paulpwo/NatiKube/releases/tag/{tag}"),
        }
    }

    #[test]
    fn parses_tags_with_or_without_v() {
        assert_eq!(parse_tag("v0.2.0"), Some(Version::new(0, 2, 0)));
        assert_eq!(parse_tag("0.2.0"), Some(Version::new(0, 2, 0)));
        assert!(parse_tag("v0.2.0-alpha.1").is_some());
        assert_eq!(parse_tag("nightly"), None);
        assert_eq!(parse_tag("v1"), None);
        assert_eq!(parse_tag(""), None);
    }

    #[test]
    fn newest_considers_pre_releases_and_semver_order() {
        let newest = newest_release([
            release("v0.1.0"),
            release("v0.2.0-alpha.2"),
            release("v0.2.0-alpha.10"),
            release("v0.2.0-alpha.9"),
        ])
        .unwrap();
        assert_eq!(newest.tag, "v0.2.0-alpha.10");
        // A final release outranks its own pre-releases.
        let newest = newest_release([release("v0.2.0-rc.1"), release("v0.2.0")]).unwrap();
        assert_eq!(newest.tag, "v0.2.0");
        assert!(newest_release(Vec::new()).is_none());
    }

    #[test]
    fn offers_only_strictly_newer_releases() {
        let releases = || [release("v0.1.0"), release("v0.2.0-alpha.1")];
        assert_eq!(
            available_update("0.1.0", releases(), None).unwrap().tag,
            "v0.2.0-alpha.1"
        );
        // Same version, or running something newer: nothing to offer.
        assert!(available_update("0.2.0-alpha.1", releases(), None).is_none());
        assert!(available_update("0.3.0", releases(), None).is_none());
        // A pre-release of the running version is older than it.
        assert!(available_update("0.2.0", releases(), None).is_none());
        // The running version is a pre-release of the published final.
        assert!(available_update("0.2.0-alpha.1", [release("v0.2.0")], None).is_some());
        // An unparseable running version offers nothing instead of guessing.
        assert!(available_update("dev", releases(), None).is_none());
    }

    #[test]
    fn skip_hides_that_version_but_not_a_newer_one() {
        let releases = || [release("v0.2.0-alpha.1")];
        assert!(available_update("0.1.0", releases(), Some("0.2.0-alpha.1")).is_none());
        assert!(available_update("0.1.0", releases(), Some("v0.2.0-alpha.1")).is_none());
        // Skipping a newer version than the newest hides the newest too.
        assert!(available_update("0.1.0", releases(), Some("0.3.0")).is_none());
        // Skipping an older version does not hide a newer release.
        assert!(available_update("0.1.0", releases(), Some("0.1.5")).is_some());
        // Garbage in the setting is ignored, not trusted.
        assert!(available_update("0.1.0", releases(), Some("whatever")).is_some());
    }

    #[test]
    fn excerpt_is_plain_text_and_bounded() {
        let notes = "## What's new\n\n- **Fast** start with `--demo`\n* See [the docs](https://x.y/z) now\n\n";
        assert_eq!(
            notes_excerpt(notes, 200),
            "What's new\nFast start with --demo\nSee the docs now"
        );
        let long = "word ".repeat(100);
        let cut = notes_excerpt(&long, 20);
        assert!(cut.ends_with('…'));
        assert!(cut.chars().count() <= 21);
        assert_eq!(notes_excerpt("", 20), "");
        // Multi-byte characters are never split.
        assert_eq!(notes_excerpt("ñandú ñandú", 4), "ñand…");
        // A lone bracket survives.
        assert_eq!(notes_excerpt("a [b c", 20), "a [b c");
    }
}
