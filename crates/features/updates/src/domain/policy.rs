//! Whether the startup check runs at all.

/// Environment variable that turns the check off (`1`, `true`, `yes`).
pub const ENV_DISABLE: &str = "NATIKUBE_NO_UPDATE_CHECK";

/// Everything the "should we check" decision reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CheckContext<'a> {
    /// `updates.check_on_startup` from the settings file.
    pub setting_enabled: bool,
    /// `--demo`: nothing may reach the network.
    pub demo: bool,
    /// `--exit-after-first-frame`: a measurement run.
    pub exit_after_first_frame: bool,
    /// Value of [`ENV_DISABLE`], if set.
    pub env_disable: Option<&'a str>,
}

/// True when this launch may contact GitHub. `--dump*` flags never reach the
/// shell (the binary exits first), so they need no entry here.
pub fn should_check(context: CheckContext<'_>) -> bool {
    context.setting_enabled
        && !context.demo
        && !context.exit_after_first_frame
        && !env_disables(context.env_disable)
}

fn env_disables(value: Option<&str>) -> bool {
    matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> CheckContext<'static> {
        CheckContext {
            setting_enabled: true,
            ..CheckContext::default()
        }
    }

    #[test]
    fn checks_by_default_when_the_setting_is_on() {
        assert!(should_check(enabled()));
    }

    #[test]
    fn the_setting_can_turn_it_off() {
        assert!(!should_check(CheckContext::default()));
    }

    #[test]
    fn demo_and_measurement_runs_never_check() {
        assert!(!should_check(CheckContext {
            demo: true,
            ..enabled()
        }));
        assert!(!should_check(CheckContext {
            exit_after_first_frame: true,
            ..enabled()
        }));
    }

    #[test]
    fn the_environment_variable_turns_it_off() {
        for value in ["1", "true", "TRUE", " yes "] {
            assert!(
                !should_check(CheckContext {
                    env_disable: Some(value),
                    ..enabled()
                }),
                "{value}"
            );
        }
        for value in ["", "0", "false"] {
            assert!(
                should_check(CheckContext {
                    env_disable: Some(value),
                    ..enabled()
                }),
                "{value}"
            );
        }
    }
}
