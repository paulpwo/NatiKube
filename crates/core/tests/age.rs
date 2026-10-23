use natikube_core::{age_since, format_age};

#[test]
fn seconds_below_two_minutes() {
    assert_eq!(format_age(0), "0s");
    assert_eq!(format_age(59), "59s");
    assert_eq!(format_age(119), "119s");
}

#[test]
fn small_clock_skew_is_zero_and_large_skew_is_invalid() {
    assert_eq!(format_age(-1), "0s");
    assert_eq!(format_age(-2), "<invalid>");
}

#[test]
fn minutes_with_seconds_below_ten_minutes() {
    assert_eq!(format_age(120), "2m");
    assert_eq!(format_age(125), "2m5s");
    assert_eq!(format_age(9 * 60 + 59), "9m59s");
}

#[test]
fn whole_minutes_below_three_hours() {
    assert_eq!(format_age(21 * 60 + 30), "21m");
    assert_eq!(format_age(179 * 60), "179m");
}

#[test]
fn hours_with_minutes_below_eight_hours() {
    assert_eq!(format_age(3 * 3600), "3h");
    assert_eq!(format_age(3 * 3600 + 5 * 60), "3h5m");
}

#[test]
fn whole_hours_below_two_days() {
    assert_eq!(format_age(19 * 3600 + 59 * 60), "19h");
    assert_eq!(format_age(47 * 3600), "47h");
}

#[test]
fn days_with_hours_below_eight_days() {
    assert_eq!(format_age(3 * 86_400), "3d");
    assert_eq!(format_age(3 * 86_400 + 4 * 3600), "3d4h");
}

#[test]
fn whole_days_below_two_years() {
    assert_eq!(format_age(100 * 86_400 + 7 * 3600), "100d");
}

#[test]
fn years_with_days_below_eight_years() {
    assert_eq!(format_age(2 * 365 * 86_400), "2y");
    assert_eq!(format_age((3 * 365 + 10) * 86_400), "3y10d");
    assert_eq!(format_age(9 * 365 * 86_400), "9y");
}

#[test]
fn age_since_uses_now_and_handles_missing_timestamp() {
    assert_eq!(age_since(Some(1_000), 1_000 + 21 * 60), "21m");
    assert_eq!(age_since(None, 1_000), "<unknown>");
}
