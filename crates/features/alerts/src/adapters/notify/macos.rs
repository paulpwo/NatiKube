//! macOS notifications through `UNUserNotificationCenter`.
//!
//! The old `NSUserNotificationCenter` path never asked for permission and
//! NatiKube never showed up in System Settings > Notifications. This one asks
//! (`requestAuthorizationWithOptions`) and reports the answer as a
//! [`PermissionState`]. The API needs a `.app` bundle: without a bundle
//! identifier `currentNotificationCenter` raises an Objective-C exception, so
//! every entry point checks that first and reports [`PermissionState::Unavailable`].
//!
//! All calls here block on a channel until the system calls back, so callers
//! run them off the UI thread.

use std::process::Command;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::{Once, atomic::AtomicU64, atomic::Ordering};
use std::time::Duration;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{AnyThread as _, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSettings,
    UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::{NotificationContent, NotificationSink};
use crate::domain::{NotifyError, PermissionCache, PermissionState};

/// The Notifications pane of System Settings (macOS 13+), then the older
/// preference-pane URL.
const SETTINGS_URLS: [&str; 2] = [
    "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
    "x-apple.systempreferences:com.apple.preference.notifications",
];

/// How long a status query or a delivery may take before giving up.
const QUICK_WAIT: Duration = Duration::from_secs(10);
/// How long the authorization prompt may stay unanswered.
const PROMPT_WAIT: Duration = Duration::from_secs(600);

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class has no
    // `Drop` and no instance variables.
    #[unsafe(super(NSObject))]
    #[name = "NatiKubeNotificationDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        /// A foreground app shows nothing unless the delegate says so. NatiKube
        /// is usually in front when the user presses "Send test notification".
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            handler: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            handler.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);

impl Delegate {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: plain `-init` on the freshly allocated NSObject subclass.
        unsafe { msg_send![super(this), init] }
    }
}

/// `true` when the process runs from a bundle that has an identifier.
fn is_bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

/// Renders a system error as `domain (code N): description`, verbatim.
fn describe(error: &NSError) -> String {
    format!(
        "{} (code {}): {}",
        error.domain(),
        error.code(),
        error.localizedDescription()
    )
}

/// [`NotificationSink`] backed by `UNUserNotificationCenter`.
pub struct UserNotificationSink {
    /// Whether the last status answer was "allowed": delivery then skips the
    /// status round-trip, which would otherwise block the thread that fires
    /// alerts (a shared Tokio worker) for up to [`QUICK_WAIT`] each time.
    allowed: PermissionCache,
}

impl UserNotificationSink {
    pub fn new() -> Self {
        if is_bundled() {
            install_delegate();
        }
        Self {
            allowed: PermissionCache::default(),
        }
    }

    /// Asks the system for the current status and remembers an "allowed".
    fn status(&self) -> PermissionState {
        let state = self.query_status();
        self.allowed.record(&state);
        state
    }

    fn query_status(&self) -> PermissionState {
        if !is_bundled() {
            return PermissionState::Unavailable;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
            // SAFETY: the system passes a valid settings object for the call.
            let raw = unsafe { settings.as_ref() }.authorizationStatus().0;
            let _ = tx.send(raw);
        });
        center.getNotificationSettingsWithCompletionHandler(&block);
        match rx.recv_timeout(QUICK_WAIT) {
            Ok(raw) => {
                tracing::info!(status = raw, "notifications: authorization status");
                PermissionState::from_authorization_status(raw)
            }
            Err(_) => {
                tracing::warn!("notifications: no answer to the settings query");
                PermissionState::Unavailable
            }
        }
    }

    fn ask(&self) -> PermissionState {
        if !is_bundled() {
            return PermissionState::Unavailable;
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |granted: Bool, error: *mut NSError| {
            // SAFETY: `error` is null or a valid NSError for the call.
            let error = unsafe { error.as_ref() }.map(describe);
            let _ = tx.send((granted.as_bool(), error));
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &block,
        );
        match rx.recv_timeout(PROMPT_WAIT) {
            Ok((granted, error)) => {
                tracing::info!(granted, error = ?error, "notifications: authorization result");
                let state = PermissionState::from_request_result(granted, error);
                self.allowed.record(&state);
                state
            }
            Err(_) => {
                tracing::warn!("notifications: the permission prompt was not answered");
                PermissionState::NotDetermined
            }
        }
    }
}

impl Default for UserNotificationSink {
    fn default() -> Self {
        Self::new()
    }
}

/// The delegate is held weakly by the center, so it must live for the whole
/// process: leak the single instance on purpose.
fn install_delegate() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let delegate = Delegate::new();
        let center = UNUserNotificationCenter::currentNotificationCenter();
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        std::mem::forget(delegate);
    });
}

impl NotificationSink for UserNotificationSink {
    fn show(&self, content: &NotificationContent) -> Result<(), NotifyError> {
        // Delivery never prompts: it runs on the shared Tokio runtime, and the
        // prompt can stay unanswered for minutes. Only `request_permission`
        // (switch turned on, or startup with the switch on) asks.
        // A remembered "allowed" skips the status round-trip; anything else
        // asks the system (and is rare: alerts only get here with the switch
        // on).
        let state = if self.allowed.allowed() {
            PermissionState::Allowed
        } else {
            self.status()
        };
        if state != PermissionState::Allowed {
            let mut reason = state
                .blocked_reason()
                .unwrap_or_else(|| "Notifications are not allowed.".into());
            if let Some(detail) = state.detail() {
                reason = format!("{reason} {detail}");
            }
            return Err(NotifyError(reason));
        }

        let center = UNUserNotificationCenter::currentNotificationCenter();
        let message = UNMutableNotificationContent::new();
        message.setTitle(&NSString::from_str(&content.title));
        message.setBody(&NSString::from_str(&content.body));
        message.setSound(Some(&UNNotificationSound::defaultSound()));
        let id = format!(
            "natikube-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&id),
            &message,
            None,
        );
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |error: *mut NSError| {
            // SAFETY: `error` is null or a valid NSError for the call.
            let _ = tx.send(unsafe { error.as_ref() }.map(describe));
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
        let delivered = match rx.recv_timeout(QUICK_WAIT) {
            Ok(None) => Ok(()),
            Ok(Some(error)) => Err(NotifyError(error)),
            Err(_) => Err(NotifyError(
                "macOS did not confirm the notification in time".into(),
            )),
        };
        if delivered.is_err() {
            // The user may have turned notifications off since: ask again
            // next time instead of trusting the remembered answer.
            self.allowed.invalidate();
        }
        delivered
    }

    fn permission(&self) -> PermissionState {
        self.status()
    }

    fn request_permission(&self) -> PermissionState {
        // Never prompt twice: only an undetermined status triggers the prompt.
        match self.status() {
            PermissionState::NotDetermined => self.ask(),
            other => other,
        }
    }
}

/// Opens System Settings > Notifications (falls back to the older pane URL).
pub fn open_notification_settings() -> Result<(), NotifyError> {
    for url in SETTINGS_URLS {
        match Command::new("/usr/bin/open").arg(url).status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => tracing::warn!(url, %status, "open failed"),
            Err(err) => tracing::warn!(url, %err, "open failed"),
        }
    }
    Err(NotifyError("could not open System Settings".into()))
}
