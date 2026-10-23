//! Display formatting and quantity parsing shared by every feature.

/// Formats an elapsed number of seconds the way `kubectl get` prints the AGE
/// column (Kubernetes `duration.HumanDuration`).
pub fn format_age(seconds: i64) -> String {
    if seconds < -1 {
        return "<invalid>".to_string();
    }
    if seconds < 0 {
        return "0s".to_string();
    }
    if seconds < 60 * 2 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 10 {
        let s = seconds % 60;
        return if s == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m{s}s")
        };
    }
    if minutes < 60 * 3 {
        return format!("{minutes}m");
    }
    let hours = seconds / 3600;
    if hours < 8 {
        let m = minutes % 60;
        return if m == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h{m}m")
        };
    }
    if hours < 48 {
        return format!("{hours}h");
    }
    if hours < 24 * 8 {
        let h = hours % 24;
        return if h == 0 {
            format!("{}d", hours / 24)
        } else {
            format!("{}d{h}h", hours / 24)
        };
    }
    if hours < 24 * 365 * 2 {
        return format!("{}d", hours / 24);
    }
    if hours < 24 * 365 * 8 {
        let days = (hours / 24) % 365;
        let years = hours / 24 / 365;
        return if days == 0 {
            format!("{years}y")
        } else {
            format!("{years}y{days}d")
        };
    }
    format!("{}y", hours / 24 / 365)
}

/// Age of an object created at `created_at` (unix seconds) as seen at `now`.
pub fn age_since(created_at: Option<i64>, now: i64) -> String {
    match created_at {
        Some(created) => format_age(now - created),
        None => "<unknown>".to_string(),
    }
}

/// Formats CPU cores with three decimals, e.g. `0.007`.
pub fn format_cpu(cores: f64) -> String {
    format!("{cores:.3}")
}

/// Formats a byte count with binary units and one decimal, e.g. `34.6Mi`.
/// Trailing `.0` is dropped (`1Ki`), and zero prints as `0`.
pub fn format_memory(bytes: u64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }
    if bytes < 1024 {
        return format!("{bytes}B");
    }
    const UNITS: [&str; 6] = ["Ki", "Mi", "Gi", "Ti", "Pi", "Ei"];
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    let text = format!("{value:.1}");
    let text = text.strip_suffix(".0").unwrap_or(&text);
    format!("{text}{}", UNITS[unit])
}

/// Parses a Kubernetes quantity (`100m`, `128Mi`, `1.5Gi`, `129e6`) into its
/// plain numeric value (cores for CPU, bytes for memory).
pub fn parse_quantity(input: &str) -> Option<f64> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    // Decimal exponent form, e.g. `129e6` or `1.5E3`.
    if let Some(pos) = input.find(['e', 'E'])
        && pos > 0
        && !input[pos..].starts_with("Ei")
        && let Ok(exp) = input[pos + 1..].parse::<i32>()
    {
        let mantissa: f64 = parse_number(&input[..pos])?;
        return Some(mantissa * 10f64.powi(exp));
    }
    let split = input
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(input.len());
    let (number, suffix) = input.split_at(split);
    let value = parse_number(number)?;
    let factor = match suffix {
        "" => 1.0,
        "n" => 1e-9,
        "u" => 1e-6,
        "m" => 1e-3,
        "k" => 1e3,
        "M" => 1e6,
        "G" => 1e9,
        "T" => 1e12,
        "P" => 1e15,
        "E" => 1e18,
        "Ki" => 1024f64,
        "Mi" => 1024f64.powi(2),
        "Gi" => 1024f64.powi(3),
        "Ti" => 1024f64.powi(4),
        "Pi" => 1024f64.powi(5),
        "Ei" => 1024f64.powi(6),
        _ => return None,
    };
    Some(value * factor)
}

fn parse_number(text: &str) -> Option<f64> {
    if text.is_empty() || text.matches('.').count() > 1 {
        return None;
    }
    text.parse().ok()
}

/// CPU quantity in cores, e.g. `7m` is `0.007`.
pub fn parse_cpu_cores(input: &str) -> Option<f64> {
    parse_quantity(input)
}

/// Memory quantity in bytes, rounded to the nearest byte.
pub fn parse_memory_bytes(input: &str) -> Option<u64> {
    let value = parse_quantity(input)?;
    (value >= 0.0).then(|| value.round() as u64)
}
