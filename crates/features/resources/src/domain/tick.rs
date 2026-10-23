use std::time::Duration;

/// How long to wait before the age text of the freshest row can change.
///
/// `format_age` prints seconds below 10 minutes, minutes below 8 hours and
/// hours or days after, so redrawing once per second only makes sense while
/// something is that young. `newest_age` is the age in seconds of the most
/// recent timestamp shown (`None` when no row has one). Older rows change
/// less often than the newest, so the newest decides.
pub fn age_tick_interval(newest_age: Option<i64>) -> Duration {
    const MINUTE: Duration = Duration::from_secs(60);
    match newest_age {
        None => MINUTE,
        Some(age) if age < 10 * 60 => Duration::from_secs(1),
        Some(age) if age < 8 * 3600 => MINUTE,
        Some(_) => Duration::from_secs(3600),
    }
}
