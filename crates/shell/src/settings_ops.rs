//! Shell side of the Settings window: open/focus it, apply preference
//! changes, remove kubeconfig sources.

use std::path::PathBuf;

use gpui_kit::*;
use std::time::Duration;

use natikube_core::{AppCommand, Settings, UserPrefs};
use natikube_settings::open_settings_window;

use crate::shell_view::ShellView;

/// Pushes the preferences that have a process-wide effect: UI font size, zoom,
/// density and monospace font (all appearances restyle live) and the terminal
/// font.
pub(crate) fn apply_ui_effects(settings: &Settings, cx: &mut App) {
    natikube_ui_kit::set_ui_prefs(settings.ui.clone(), cx);
    let zoom = f32::from(settings.ui.zoom_percent) / 100.0;
    natikube_terminal::set_font(
        f32::from(settings.terminal.font_size) * zoom,
        settings.ui.mono_font.clone(),
    );
}

impl ShellView {
    /// Opens the Settings window, or brings the existing one to the front.
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((handle, _)) = &self.settings_window
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }
        match open_settings_window(self.settings.clone(), cx) {
            Ok((handle, view)) => {
                self._settings_subscription = Some(cx.subscribe_in(
                    &view,
                    window,
                    |this, _, command: &AppCommand, window, cx| {
                        this.dispatch(command.clone(), window, cx)
                    },
                ));
                self.settings_window = Some((handle, view));
            }
            Err(e) => self.show_error(&format!("Could not open Settings: {e}"), cx),
        }
    }

    /// Applies preferences from the Settings window: clamps, persists,
    /// restyles every window live (appearance, font size, zoom, density,
    /// monospace font, terminal font), restarts the metrics poller when its
    /// interval changed and refreshes the window's copy. Logs defaults and the
    /// default namespace are read when a log tab or cluster opens.
    pub(crate) fn apply_user_prefs(
        &mut self,
        prefs: UserPrefs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut next = self.settings.clone();
        next.apply_user_prefs(prefs);
        if next == self.settings {
            return;
        }
        let poll_changed = next.data.metrics_poll_secs != self.settings.data.metrics_poll_secs;
        let new_appearance = next.appearance;
        let appearance_changed = new_appearance != self.settings.appearance;
        // `set_appearance` owns persisting the appearance (it honors
        // `--appearance`), so keep the old value until it runs.
        next.appearance = self.settings.appearance;
        self.settings = next;
        apply_ui_effects(&self.settings, cx);
        if poll_changed && let Some(session) = &self.session {
            let poll = Duration::from_secs(u64::from(self.settings.data.metrics_poll_secs));
            session
                .resources
                .update(cx, |view, cx| view.set_poll_interval(poll, window, cx));
        }
        if appearance_changed {
            self.set_appearance(new_appearance, cx);
        } else {
            self.persist();
            cx.notify();
        }
        self.sync_settings_window(cx);
    }

    /// "Remove" in Settings → Clusters.
    pub(crate) fn remove_kubeconfig(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.settings.extra_kubeconfigs.len();
        self.settings.extra_kubeconfigs.retain(|p| p != &path);
        if self.settings.extra_kubeconfigs.len() == before {
            return;
        }
        self.persist();
        self.sync_settings_window(cx);
        self.reload_clusters(window, cx);
    }

    /// Pushes the current settings into the open Settings window, if any.
    pub(crate) fn sync_settings_window(&mut self, cx: &mut Context<Self>) {
        if let Some((_, view)) = &self.settings_window {
            let settings = self.settings.clone();
            view.update(cx, |view, cx| view.set_settings(settings, cx));
        }
    }
}
