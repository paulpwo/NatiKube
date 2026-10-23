use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PortError {
    #[error("`{0}` is not a valid port (use 1-65535)")]
    Invalid(String),
}

/// Parses a TCP port in `1..=65535`.
pub fn parse_port(input: &str) -> Result<u16, PortError> {
    let trimmed = input.trim();
    match trimmed.parse::<u16>() {
        Ok(port) if port > 0 => Ok(port),
        _ => Err(PortError::Invalid(trimmed.to_string())),
    }
}

/// Parses `"8080:80"` (local:remote) or `"80"` (remote only, local chosen by
/// the OS) into `(local, remote)`.
pub fn parse_port_pair(input: &str) -> Result<(Option<u16>, u16), PortError> {
    match input.split_once(':') {
        Some((local, remote)) => Ok((Some(parse_port(local)?), parse_port(remote)?)),
        None => Ok((None, parse_port(input)?)),
    }
}

/// URL opened by the "open in browser" action.
pub fn format_local_url(port: u16) -> String {
    format!("http://localhost:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_ports() {
        assert_eq!(parse_port("80"), Ok(80));
        assert_eq!(parse_port(" 65535 "), Ok(65535));
    }

    #[test]
    fn rejects_invalid_ports() {
        for bad in ["", "0", "65536", "-1", "abc", "80a"] {
            assert!(parse_port(bad).is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn parses_pairs() {
        assert_eq!(parse_port_pair("8080:80"), Ok((Some(8080), 80)));
        assert_eq!(parse_port_pair("80"), Ok((None, 80)));
        assert!(parse_port_pair("8080:").is_err());
        assert!(parse_port_pair(":80").is_err());
    }

    #[test]
    fn builds_local_url() {
        assert_eq!(format_local_url(3000), "http://localhost:3000");
    }
}
