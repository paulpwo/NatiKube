//! Notification permission as the user sees it. Pure: the macOS adapter maps
//! `UNAuthorizationStatus` and the authorization result into these values, and
//! the shell and Settings decide from them when to ask and what to say.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the operating system lets NatiKube show notifications.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PermissionState {
    /// The platform has no permission to ask for (Windows, Linux) or the
    /// notifier is silent. Settings shows nothing.
    #[default]
    NotApplicable,
    /// macOS only: the binary does not run from a `.app` bundle, so the
    /// notification API cannot be used.
    Unavailable,
    /// The user was never asked.
    NotDetermined,
    Allowed,
    /// The user (or an earlier answer) turned notifications off.
    Denied,
    /// macOS refused the authorization request itself (for example an
    /// ad-hoc signed build); carries the system error, verbatim.
    Refused(String),
}

impl PermissionState {
    /// Maps `UNAuthorizationStatus` (`NotDetermined` 0, `Denied` 1,
    /// `Authorized` 2, `Provisional` 3, `Ephemeral` 4).
    pub fn from_authorization_status(raw: isize) -> Self {
        match raw {
            0 => Self::NotDetermined,
            1 => Self::Denied,
            2..=4 => Self::Allowed,
            _ => Self::Unavailable,
        }
    }

    /// Maps the result of `requestAuthorizationWithOptions`: `error` is the
    /// system error rendered as text, when there was one.
    pub fn from_request_result(granted: bool, error: Option<String>) -> Self {
        match (granted, error) {
            (true, _) => Self::Allowed,
            (false, Some(error)) => Self::Refused(error),
            (false, None) => Self::Denied,
        }
    }

    /// The line shown under the switch; `None` when there is nothing to say.
    pub fn status_line(&self) -> Option<String> {
        match self {
            Self::NotApplicable => None,
            Self::Unavailable => Some("Available when NatiKube runs as an installed app.".into()),
            Self::NotDetermined => {
                Some("Not asked yet. macOS asks when you turn the switch on.".into())
            }
            Self::Allowed => Some("Allowed by macOS.".into()),
            Self::Denied => Some("Turned off for NatiKube in System Settings.".into()),
            Self::Refused(_) => Some("macOS did not allow notifications for this build.".into()),
        }
    }

    /// What Settings shows under the switch: the status line and, for a
    /// refusal, the system error verbatim on its own line.
    pub fn status_text(&self) -> Option<String> {
        let line = self.status_line()?;
        Some(match self.detail() {
            Some(detail) => format!("{line}\n{detail}"),
            None => line,
        })
    }

    /// The system error behind a refusal, for a tooltip or the log.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Refused(error) => Some(error),
            _ => None,
        }
    }

    /// Whether Settings offers "Open System Settings".
    pub fn offers_system_settings(&self) -> bool {
        matches!(self, Self::Denied | Self::Refused(_))
    }

    /// Why a notification cannot be shown right now, if it cannot.
    pub fn blocked_reason(&self) -> Option<String> {
        match self {
            Self::NotApplicable | Self::Allowed => None,
            Self::NotDetermined => Some("Notifications were not allowed yet.".into()),
            other => other.status_line(),
        }
    }
}

/// Remembers that notifications are allowed, so delivering an alert does not
/// repeat the system status round-trip on the thread that fires alerts. Only
/// `Allowed` is kept: any other answer, or a failed delivery, forgets it and
/// the next delivery asks the system again.
#[derive(Debug, Default)]
pub struct PermissionCache(AtomicBool);

impl PermissionCache {
    pub fn record(&self, state: &PermissionState) {
        self.0
            .store(*state == PermissionState::Allowed, Ordering::Relaxed);
    }

    pub fn allowed(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn invalidate(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// Ask when the user turns the switch on and was never asked.
pub fn should_request_on_enable(was_on: bool, now_on: bool, state: &PermissionState) -> bool {
    !was_on && now_on && *state == PermissionState::NotDetermined
}

/// At startup, ask only when the switch is already on and the user was never
/// asked. Never otherwise.
pub fn should_request_at_startup(os_on: bool, state: &PermissionState) -> bool {
    os_on && *state == PermissionState::NotDetermined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_shows_the_exact_system_error_under_the_status_line() {
        let error = "UNErrorDomain (code 1): Notifications are not allowed for this application";
        let text = PermissionState::Refused(error.into())
            .status_text()
            .unwrap();
        assert!(text.starts_with("macOS did not allow notifications for this build."));
        assert!(text.ends_with(error), "the error is verbatim: {text}");
        assert!(text.contains('\n'), "the error sits on its own line");
    }

    #[test]
    fn other_states_show_only_their_status_line() {
        for state in [
            PermissionState::Unavailable,
            PermissionState::NotDetermined,
            PermissionState::Allowed,
            PermissionState::Denied,
        ] {
            assert_eq!(state.status_text(), state.status_line());
        }
        assert_eq!(PermissionState::NotApplicable.status_text(), None);
    }

    #[test]
    fn only_an_allowed_status_is_remembered() {
        let cache = PermissionCache::default();
        assert!(!cache.allowed(), "nothing is known at first");
        cache.record(&PermissionState::Allowed);
        assert!(cache.allowed());
        cache.record(&PermissionState::Denied);
        assert!(!cache.allowed(), "any other answer forgets it");
        cache.record(&PermissionState::Allowed);
        cache.record(&PermissionState::Refused("e".into()));
        assert!(!cache.allowed());
    }

    #[test]
    fn a_failed_delivery_forgets_the_remembered_status() {
        let cache = PermissionCache::default();
        cache.record(&PermissionState::Allowed);
        cache.invalidate();
        assert!(!cache.allowed());
    }

    #[test]
    fn maps_every_authorization_status() {
        use PermissionState::*;
        assert_eq!(PermissionState::from_authorization_status(0), NotDetermined);
        assert_eq!(PermissionState::from_authorization_status(1), Denied);
        assert_eq!(PermissionState::from_authorization_status(2), Allowed);
        assert_eq!(PermissionState::from_authorization_status(3), Allowed);
        assert_eq!(PermissionState::from_authorization_status(4), Allowed);
        assert_eq!(PermissionState::from_authorization_status(9), Unavailable);
    }

    #[test]
    fn maps_the_request_result() {
        use PermissionState::*;
        assert_eq!(PermissionState::from_request_result(true, None), Allowed);
        assert_eq!(PermissionState::from_request_result(false, None), Denied);
        assert_eq!(
            PermissionState::from_request_result(false, Some("boom".into())),
            Refused("boom".into())
        );
    }

    #[test]
    fn asks_only_when_turned_on_and_never_asked() {
        use PermissionState::*;
        assert!(should_request_on_enable(false, true, &NotDetermined));
        assert!(!should_request_on_enable(true, true, &NotDetermined));
        assert!(!should_request_on_enable(false, false, &NotDetermined));
        assert!(!should_request_on_enable(false, true, &Denied));
        assert!(!should_request_on_enable(false, true, &Allowed));
        assert!(!should_request_on_enable(false, true, &Unavailable));
    }

    #[test]
    fn asks_at_startup_only_with_the_switch_on() {
        use PermissionState::*;
        assert!(should_request_at_startup(true, &NotDetermined));
        assert!(!should_request_at_startup(false, &NotDetermined));
        assert!(!should_request_at_startup(true, &Allowed));
        assert!(!should_request_at_startup(true, &Denied));
        assert!(!should_request_at_startup(true, &NotApplicable));
    }

    #[test]
    fn denied_and_refused_offer_system_settings() {
        use PermissionState::*;
        assert!(Denied.offers_system_settings());
        assert!(Refused("x".into()).offers_system_settings());
        for state in [NotApplicable, Unavailable, NotDetermined, Allowed] {
            assert!(!state.offers_system_settings());
        }
    }

    #[test]
    fn lines_are_honest() {
        use PermissionState::*;
        assert_eq!(NotApplicable.status_line(), None);
        assert_eq!(
            Unavailable.status_line().as_deref(),
            Some("Available when NatiKube runs as an installed app.")
        );
        assert_eq!(
            Refused("e".into()).status_line().as_deref(),
            Some("macOS did not allow notifications for this build.")
        );
        assert_eq!(Refused("e".into()).detail(), Some("e"));
        assert_eq!(Denied.detail(), None);
    }

    #[test]
    fn only_allowed_and_not_applicable_are_not_blocked() {
        use PermissionState::*;
        assert!(Allowed.blocked_reason().is_none());
        assert!(NotApplicable.blocked_reason().is_none());
        assert!(Denied.blocked_reason().is_some());
        assert!(Unavailable.blocked_reason().is_some());
        assert!(NotDetermined.blocked_reason().is_some());
    }
}
