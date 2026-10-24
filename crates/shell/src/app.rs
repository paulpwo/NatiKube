use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::*;
use natikube_alerts::AlertStore;
use natikube_core::{AppCommand, Appearance, ClusterId, ClusterInfo, SettingsStore};
use natikube_ui_kit::set_appearance;

use crate::menus::{
    AppearanceCommand, AppearanceNative, AppearanceObservatory, AppearanceStandard, ClustersReload,
    FreelensImport, KubeconfigAdd, KubeconfigAddFolder, NotificationsOpen, PaletteOpen, Quit,
    SettingsOpen, WindowMinimize, WindowZoom, install_menus,
};
use crate::shell_view::ShellView;

/// Key context for the global `secondary-` shortcuts. `secondary` is Cmd on
/// macOS and Ctrl elsewhere; off macOS the shortcuts must not fire inside a
/// focused terminal, where Ctrl+Q/K/M/, belong to the shell or TUI running
/// there (GPUI dispatches matching bindings before key listeners).
fn app_key_context_for(is_macos: bool) -> Option<&'static str> {
    if is_macos { None } else { Some("!Terminal") }
}

fn app_key_context() -> Option<&'static str> {
    app_key_context_for(cfg!(target_os = "macos"))
}

/// The alert history and whether the shell may start watchers for it.
#[derive(Clone)]
pub struct AlertsOptions {
    /// Where fired alerts are kept (SQLite in the config dir, or memory).
    pub store: Arc<dyn AlertStore>,
    /// Start the monitor for clusters with enabled rules. Off in the demo
    /// and with fake data, where nothing may reach the network.
    pub monitor: bool,
}

/// Everything the shell needs from the binary.
#[derive(Clone)]
pub struct ShellOptions {
    /// Where settings persist (last cluster, namespaces, last view).
    pub store: Arc<dyn SettingsStore>,
    /// Clusters shown in the rail.
    pub catalog: Vec<ClusterInfo>,
    /// Cluster to open at startup (`--context`); falls back to the last used
    /// one, then to the first catalog entry.
    pub initial_cluster: Option<ClusterId>,
    /// Show N synthetic pods instead of connecting (`--fake-rows`).
    pub fake_rows: Option<usize>,
    /// Process start, for the first-frame measurement.
    pub started_at: Instant,
    /// Log `first_frame_ms` / `first_rows_frame_ms` on stderr and exit.
    pub exit_after_first_frame: bool,
    /// Give up waiting for rows after this long when measuring.
    pub rows_timeout: Duration,
    /// `--appearance`: overrides the saved appearance for this run only; it is
    /// never written to the settings file.
    pub appearance_override: Option<Appearance>,
    /// `--native-mode`: force Native light (`false`) or dark (`true`) for this
    /// run only.
    pub native_dark_override: Option<bool>,
    /// `--open-settings`: open the Settings window at startup.
    pub open_settings: bool,
    /// `--settings-section`: the page `--open-settings` starts on.
    pub settings_section: Option<natikube_settings::SettingsSection>,
    /// `--open-alerts`: open the notifications panel at startup.
    pub open_alerts: bool,
    /// `--demo`: a fictional catalog served by the built-in demo data. No
    /// kubeconfig is read, nothing reaches the network, and kubeconfig
    /// management (reload, import, add, remove) does nothing.
    pub demo: bool,
    /// A non-blocking message about startup (for example a settings file that
    /// could not be read); shown in the status bar.
    pub startup_warning: Option<String>,
    /// The alert history and monitor switch.
    pub alerts: AlertsOptions,
    /// Runs when the app quits (flushes queued settings writes).
    pub on_quit: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// Opens the main window and runs the GPUI event loop until quit.
pub fn run(options: ShellOptions) {
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        natikube_ui_kit::register_fonts(cx);
        crate::dock_icon::set_dock_icon();
        if let Some(on_quit) = options.on_quit.clone() {
            cx.on_app_quit(move |_| {
                on_quit();
                async {}
            })
            .detach();
        }
        let saved = options.store.load().unwrap_or_default();
        let initial = options.appearance_override.unwrap_or(saved.appearance);
        if let Some(dark) = options.native_dark_override {
            natikube_ui_kit::force_native_dark(dark, cx);
        }
        crate::settings_ops::apply_ui_effects(&saved, cx);
        set_appearance(initial, cx);

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("secondary-q", Quit, app_key_context()),
            KeyBinding::new("secondary-k", PaletteOpen, app_key_context()),
            KeyBinding::new("secondary-,", SettingsOpen, app_key_context()),
            KeyBinding::new("secondary-m", WindowMinimize, app_key_context()),
            KeyBinding::new("secondary-shift-n", NotificationsOpen, app_key_context()),
        ]);
        install_menus(initial, cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1360.), px(860.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("NatiKube".into()),
                ..TitlebarOptions::default()
            }),
            ..WindowOptions::default()
        };

        let options_open_settings = options.open_settings;
        let options_settings_section = options.settings_section;
        let options_open_alerts = options.open_alerts;
        let (window, shell) = gpui_kit::open_window(window_options, cx, move |window, cx| {
            cx.new(|cx| ShellView::new(options, window, cx))
        })
        .expect("failed to open the main window");
        // Only the main window ends the app; Settings may close on its own.
        let main_id = window.window_id();
        cx.on_window_closed(move |cx, closed| {
            if closed == main_id {
                cx.quit();
            }
        })
        .detach();
        if options_open_settings {
            let shell = shell.clone();
            let _ = window.update(cx, move |_, window, cx| {
                shell.update(cx, |view, cx| {
                    view.dispatch(AppCommand::OpenSettings, window, cx)
                })
            });
        }
        if let Some(section) = options_settings_section.filter(|_| options_open_settings) {
            let shell = shell.clone();
            let _ = window.update(cx, move |_, _, cx| {
                shell.update(cx, |view, cx| view.select_settings_section(section, cx))
            });
        }
        if options_open_alerts {
            let shell = shell.clone();
            let _ = window.update(cx, move |_, window, cx| {
                shell.update(cx, |view, cx| {
                    view.dispatch(AppCommand::OpenAlerts, window, cx)
                })
            });
        }
        wire_menu_actions(window, shell, cx);
        cx.activate(true);
    });
}

/// Routes one menu action to the shell as an [`AppCommand`].
fn route<A: Action>(
    command: AppCommand,
    window: AnyWindowHandle,
    shell: &Entity<ShellView>,
    cx: &mut App,
) {
    let shell = shell.clone();
    cx.on_action(move |_: &A, cx| {
        let command = command.clone();
        let shell = shell.clone();
        update_window_later(window, cx, move |window, cx| {
            shell.update(cx, |view, cx| view.dispatch(command, window, cx))
        });
    });
}

/// Runs `f` on `window` once the current dispatch finishes. Global action
/// listeners run while GPUI holds the active window, so a direct
/// `window.update` there fails silently.
fn update_window_later(
    window: AnyWindowHandle,
    cx: &mut App,
    f: impl FnOnce(&mut Window, &mut App) + 'static,
) {
    cx.defer(move |cx| {
        let _ = window.update(cx, move |_, window, cx| f(window, cx));
    });
}

/// Connects the macOS menu bar and its shortcuts to the shell.
fn wire_menu_actions(window: AnyWindowHandle, shell: Entity<ShellView>, cx: &mut App) {
    route::<PaletteOpen>(AppCommand::OpenPalette, window, &shell, cx);
    route::<SettingsOpen>(AppCommand::OpenSettings, window, &shell, cx);
    route::<NotificationsOpen>(AppCommand::OpenAlerts, window, &shell, cx);
    route::<FreelensImport>(AppCommand::ImportFreelens, window, &shell, cx);
    route::<KubeconfigAdd>(AppCommand::AddKubeconfig, window, &shell, cx);
    route::<ClustersReload>(AppCommand::ReloadClusters, window, &shell, cx);
    route::<AppearanceStandard>(
        AppCommand::SetAppearance(Appearance::Standard),
        window,
        &shell,
        cx,
    );
    route::<AppearanceCommand>(
        AppCommand::SetAppearance(Appearance::Command),
        window,
        &shell,
        cx,
    );
    route::<AppearanceNative>(
        AppCommand::SetAppearance(Appearance::Native),
        window,
        &shell,
        cx,
    );
    route::<AppearanceObservatory>(
        AppCommand::SetAppearance(Appearance::Observatory),
        window,
        &shell,
        cx,
    );
    let folder_shell = shell.clone();
    cx.on_action(move |_: &KubeconfigAddFolder, cx| {
        let shell = folder_shell.clone();
        update_window_later(window, cx, move |window, cx| {
            shell.update(cx, |view, cx| view.pick_kubeconfigs(true, window, cx))
        });
    });
    cx.on_action(move |_: &WindowMinimize, cx| {
        update_window_later(window, cx, |window, _| window.minimize_window());
    });
    cx.on_action(move |_: &WindowZoom, cx| {
        update_window_later(window, cx, |window, _| window.zoom_window());
    });
}

#[cfg(test)]
mod key_binding_tests {
    use super::app_key_context_for;
    use gpui_kit::{KeyBindingContextPredicate, KeyContext};

    #[test]
    fn global_shortcuts_exclude_the_terminal_off_macos() {
        let ctx = app_key_context_for(false).expect("off macOS a context is required");
        let predicate = KeyBindingContextPredicate::parse(ctx).expect("valid predicate");
        let terminal = vec![
            KeyContext::parse("Workspace").unwrap(),
            KeyContext::parse("Terminal").unwrap(),
        ];
        let other = vec![KeyContext::parse("Workspace").unwrap()];
        assert!(!predicate.eval(&terminal));
        assert!(predicate.eval(&other));
    }

    #[test]
    fn macos_keeps_cmd_shortcuts_global() {
        assert_eq!(app_key_context_for(true), None);
    }
}
