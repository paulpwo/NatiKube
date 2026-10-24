//! Shell side of the update check: decide, fetch off the UI thread, show the
//! modal and act on the user's choice.

use gpui_kit::*;
use natikube_updates::{
    CheckContext, ENV_DISABLE, Release, UpdateEvent, UpdateOffer, available_update, fetch_releases,
    parse_tag, should_check,
};

use crate::shell_view::ShellView;

const RELEASES_PAGE: &str = "https://github.com/paulpwo/NatiKube/releases";

/// The release `--fake-update` pretends exists.
fn simulated_release(version: &str) -> Option<Release> {
    let parsed = parse_tag(version)?;
    Some(Release {
        tag: format!("v{parsed}"),
        name: Some("Simulated release (--fake-update)".to_string()),
        notes: "## What's new\n- This is a simulated release for checking the modal.\n- Nothing was downloaded and no request was made."
            .to_string(),
        url: RELEASES_PAGE.to_string(),
        version: parsed,
    })
}

impl ShellView {
    /// Starts the once-per-launch check if the flags, the environment and the
    /// setting allow it. Failures are silent: only a debug line is logged.
    pub(crate) fn begin_update_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let version = self.options.app_version;
        let simulated = self
            .options
            .fake_update
            .as_deref()
            .and_then(simulated_release);
        if simulated.is_none() {
            let env = std::env::var(ENV_DISABLE).ok();
            let allowed = should_check(CheckContext {
                setting_enabled: self.settings.updates.check_on_startup,
                demo: self.options.demo,
                exit_after_first_frame: self.options.exit_after_first_frame,
                env_disable: env.as_deref(),
            });
            if !allowed {
                tracing::debug!("update check skipped");
                return;
            }
        }
        let is_simulated = simulated.is_some();
        self._update_check = Some(cx.spawn_in(window, async move |this, cx| {
            let releases = match simulated {
                Some(release) => vec![release],
                None => {
                    // The request runs on the shared Tokio runtime; this task
                    // only awaits its result.
                    match natikube_kube_client::spawn(fetch_releases(version)).await {
                        Ok(Ok(releases)) => releases,
                        Ok(Err(error)) => {
                            tracing::debug!(%error, "update check failed");
                            return;
                        }
                        Err(error) => {
                            tracing::debug!(%error, "update check task failed");
                            return;
                        }
                    }
                }
            };
            let _ = this.update_in(cx, move |this, window, cx| {
                let skipped = this.settings.updates.skipped_version.clone();
                let Some(release) = available_update(version, releases, skipped.as_deref()) else {
                    tracing::debug!("no newer release to offer");
                    return;
                };
                let offer = UpdateOffer {
                    current: version.to_string(),
                    release,
                    simulated: is_simulated,
                };
                this.update_modal
                    .update(cx, |modal, cx| modal.show(offer, window, cx));
            });
        }));
    }

    /// The user's choice in the update modal (it has already closed itself).
    pub(crate) fn on_update_event(&mut self, event: UpdateEvent, cx: &mut Context<Self>) {
        match event {
            UpdateEvent::Download(url) => {
                if let Err(error) = natikube_portforward::domain::open_url(&url) {
                    self.show_error(&format!("Could not open the release page: {error}"), cx);
                }
            }
            UpdateEvent::Later => {}
            UpdateEvent::Skip { version, persist } => {
                if persist {
                    self.settings.updates.skipped_version = Some(version);
                    self.persist();
                    self.sync_settings_window(cx);
                }
            }
        }
    }
}
