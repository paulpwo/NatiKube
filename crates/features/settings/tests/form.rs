use std::path::Path;

use natikube_core::{Appearance, Density, UserPrefs};
use natikube_settings::domain::{PrefEdit, apply_edit, display_path, parse_number, step};

#[test]
fn step_clamps_to_range_in_both_directions() {
    assert_eq!(step(13u8, 1, &(10..=20)), 14);
    assert_eq!(step(20u8, 1, &(10..=20)), 20);
    assert_eq!(step(10u8, -1, &(10..=20)), 10);
    assert_eq!(step(100u16, 25, &(75..=150)), 125);
    assert_eq!(step(140u16, 25, &(75..=150)), 150);
    assert_eq!(step(1000u32, -5000, &(10..=100_000)), 10);
}

#[test]
fn edits_change_only_their_field_and_clamp() {
    let base = UserPrefs::default();
    let next = apply_edit(&base, PrefEdit::Appearance(Appearance::Native));
    assert_eq!(next.appearance, Appearance::Native);
    assert_eq!(next.ui, base.ui);

    let next = apply_edit(&base, PrefEdit::UiFontSize(99));
    assert_eq!(next.ui.font_size, 20);
    let next = apply_edit(&base, PrefEdit::Zoom(10));
    assert_eq!(next.ui.zoom_percent, 75);
    let next = apply_edit(&base, PrefEdit::Density(Density::Compact));
    assert_eq!(next.ui.density, Density::Compact);
    let next = apply_edit(&base, PrefEdit::TerminalFontSize(1));
    assert_eq!(next.terminal.font_size, 8);
    let next = apply_edit(&base, PrefEdit::TailLines(5_000_000));
    assert_eq!(next.logs.tail_lines, 100_000);
    let next = apply_edit(&base, PrefEdit::LogTimestamps(true));
    assert!(next.logs.timestamps && !next.logs.wrap);
    let next = apply_edit(&base, PrefEdit::LogWrap(true));
    assert!(next.logs.wrap && !next.logs.timestamps);
    let next = apply_edit(&base, PrefEdit::MetricsPollSecs(1));
    assert_eq!(next.data.metrics_poll_secs, 5);
}

#[test]
fn blank_text_edits_become_none_and_values_are_trimmed() {
    let base = UserPrefs::default();
    let next = apply_edit(&base, PrefEdit::MonoFont("  Menlo ".into()));
    assert_eq!(next.ui.mono_font.as_deref(), Some("Menlo"));
    let cleared = apply_edit(&next, PrefEdit::MonoFont("   ".into()));
    assert_eq!(cleared.ui.mono_font, None);
    let next = apply_edit(&base, PrefEdit::Shell("/bin/bash".into()));
    assert_eq!(next.terminal.shell.as_deref(), Some("/bin/bash"));
    let next = apply_edit(&base, PrefEdit::DefaultNamespace("kube-system".into()));
    assert_eq!(next.data.default_namespace.as_deref(), Some("kube-system"));
    let cleared = apply_edit(&next, PrefEdit::DefaultNamespace(String::new()));
    assert_eq!(cleared.data.default_namespace, None);
}

#[test]
fn parse_number_accepts_digits_and_clamps() {
    assert_eq!(parse_number(" 250 ", &(10..=100_000)), Some(250));
    assert_eq!(parse_number("5", &(10..=100_000)), Some(10));
    assert_eq!(parse_number("999999999", &(10..=100_000)), Some(100_000));
    assert_eq!(parse_number("", &(10..=100_000)), None);
    assert_eq!(parse_number("12a", &(10..=100_000)), None);
    assert_eq!(parse_number("-4", &(10..=100_000)), None);
}

#[test]
fn display_path_abbreviates_home() {
    let home = Path::new("/Users/me");
    // The separator after "~" is the platform's own.
    assert_eq!(
        display_path(Path::new("/Users/me/.kube/config"), Some(home)),
        format!("~{}.kube/config", std::path::MAIN_SEPARATOR)
    );
    assert_eq!(display_path(Path::new("/Users/me"), Some(home)), "~");
    assert_eq!(
        display_path(Path::new("/Users/mead/x"), Some(home)),
        "/Users/mead/x"
    );
    assert_eq!(display_path(Path::new("/etc/k"), None), "/etc/k");
}
