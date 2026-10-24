use std::path::PathBuf;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::{AppCommand, Appearance, Density, LogPrefs, Settings, UiPrefs};
use natikube_ui_kit::{ActiveTokens, Palette, segmented_control};

use crate::domain::{
    PendingEdits, PrefEdit, SettingsSection, TEXT_DEBOUNCE, apply_edit, display_path,
    kubeconfig_help, parse_number, shell_placeholder, step,
};

/// Root view of the Settings window. Every edit updates the local copy at once
/// and is emitted as `AppCommand::SettingsChanged`; the shell persists it. The
/// view never touches `settings.json`.
pub struct SettingsView {
    settings: Settings,
    section: SettingsSection,
    mono_font: Entity<InputState>,
    shell: Entity<InputState>,
    namespace: Entity<InputState>,
    tail_lines: Entity<InputState>,
    /// Typed text edits waiting for the debounce timer.
    pending: PendingEdits,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<AppCommand> for SettingsView {}

impl SettingsView {
    pub fn new(settings: Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text_input =
            |placeholder: &str, value: Option<&str>, window: &mut Window, cx: &mut App| {
                let placeholder = placeholder.to_string();
                let value = value.unwrap_or_default().to_string();
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(placeholder)
                        .default_value(value)
                })
            };
        let mono_font = text_input(
            "System monospace",
            settings.ui.mono_font.as_deref(),
            window,
            cx,
        );
        let shell = text_input(
            shell_placeholder(cfg!(windows)),
            settings.terminal.shell.as_deref(),
            window,
            cx,
        );
        let namespace = text_input(
            "All namespaces",
            settings.data.default_namespace.as_deref(),
            window,
            cx,
        );
        let tail_lines = text_input(
            "1000",
            Some(&settings.logs.tail_lines.to_string()),
            window,
            cx,
        );

        let mut subscriptions = Vec::new();
        let mut on_change = |input: &Entity<InputState>,
                             make: fn(String) -> Option<PrefEdit>,
                             window: &mut Window,
                             cx: &mut Context<Self>| {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |this: &mut Self, state, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => {
                        if let Some(edit) = make(state.read(cx).value().to_string()) {
                            this.edit_debounced(edit, cx);
                        }
                    }
                    InputEvent::Blur | InputEvent::PressEnter { .. } => this.flush_pending(cx),
                    InputEvent::Focus => {}
                },
            ));
        };
        on_change(&mono_font, |t| Some(PrefEdit::MonoFont(t)), window, cx);
        on_change(&shell, |t| Some(PrefEdit::Shell(t)), window, cx);
        on_change(
            &namespace,
            |t| Some(PrefEdit::DefaultNamespace(t)),
            window,
            cx,
        );
        on_change(
            &tail_lines,
            |t| parse_number(&t, &LogPrefs::TAIL_LINES).map(PrefEdit::TailLines),
            window,
            cx,
        );

        Self {
            settings,
            section: SettingsSection::default(),
            mono_font,
            shell,
            namespace,
            tail_lines,
            pending: PendingEdits::default(),
            _subscriptions: subscriptions,
        }
    }

    /// The shell's settings changed outside this window; refresh the controls.
    pub fn set_settings(&mut self, settings: Settings, cx: &mut Context<Self>) {
        if self.settings != settings {
            self.settings = settings;
            cx.notify();
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn section(&self) -> SettingsSection {
        self.section
    }

    pub fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        if self.section != section {
            self.section = section;
            cx.notify();
        }
    }

    /// Applies one edit locally and tells the shell. No-op edits emit nothing.
    fn edit(&mut self, edit: PrefEdit, cx: &mut Context<Self>) {
        let current = self.settings.user_prefs();
        let next = apply_edit(&current, edit);
        if next == current {
            return;
        }
        self.settings.apply_user_prefs(next.clone());
        cx.emit(AppCommand::SettingsChanged(Box::new(next)));
        cx.notify();
    }

    /// Queues a typed text edit: it is applied (and so persisted) once typing
    /// pauses for [`TEXT_DEBOUNCE`], not on every keystroke.
    fn edit_debounced(&mut self, edit: PrefEdit, cx: &mut Context<Self>) {
        let generation = self.pending.push(edit);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TEXT_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                if let Some(edits) = this.pending.take_if_current(generation) {
                    for edit in edits {
                        this.edit(edit, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Applies every queued text edit now. Called when a field loses focus or
    /// Enter is pressed; the shell calls it before closing the window so the
    /// last keystrokes are never lost.
    pub fn flush_pending(&mut self, cx: &mut Context<Self>) {
        for edit in self.pending.take_all() {
            self.edit(edit, cx);
        }
    }

    fn command(&mut self, command: AppCommand, cx: &mut Context<Self>) {
        cx.emit(command);
    }
}

// ---- small building blocks -------------------------------------------------

fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    p: Palette,
    radius: Pixels,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(26.))
        .px_3()
        .flex()
        .items_center()
        .rounded(radius)
        .border_1()
        .border_color(p.control_border)
        .bg(p.control)
        .text_color(p.text)
        .text_sm()
        .cursor_pointer()
        .hover(move |s| s.bg(p.control_hover))
        .active(move |s| s.bg(p.control_active))
        .on_click(on_click)
        .child(label.into())
}

fn switch(
    id: impl Into<ElementId>,
    on: bool,
    p: Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .relative()
        .w(px(36.))
        .h(px(20.))
        .rounded_full()
        .cursor_pointer()
        .bg(if on { p.accent } else { p.control_border })
        .on_click(on_click)
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(if on { px(18.) } else { px(2.) })
                .size(px(16.))
                .rounded_full()
                .bg(if on { p.on_accent } else { p.surface })
                .shadow_xs(),
        )
}

/// A labelled row: title and optional help on the left, the control on the right.
fn field(
    title: &'static str,
    help: Option<&'static str>,
    control: impl IntoElement,
    p: Palette,
) -> Div {
    h_flex()
        .w_full()
        .min_h(px(44.))
        .py_2()
        .gap_4()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(p.row_divider)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().text_color(p.text_strong).child(title))
                .when_some(help, |d, help| {
                    d.child(div().text_xs().text_color(p.text_muted).child(help))
                }),
        )
        .child(div().flex_none().child(control))
}

/// Like [`field`] with the control on its own line below the label, for wide
/// controls such as the four-way template switch.
fn field_stacked(
    title: &'static str,
    help: Option<&'static str>,
    control: impl IntoElement,
    p: Palette,
) -> Div {
    v_flex()
        .w_full()
        .py_2()
        .gap_2()
        .border_b_1()
        .border_color(p.row_divider)
        .child(
            v_flex()
                .gap_0p5()
                .child(div().text_sm().text_color(p.text_strong).child(title))
                .when_some(help, |d, help| {
                    d.child(div().text_xs().text_color(p.text_muted).child(help))
                }),
        )
        .child(div().child(control))
}

fn page(title: &'static str, p: Palette) -> Div {
    v_flex().w_full().gap_1().child(
        div()
            .pb_2()
            .text_xl()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(p.text_strong)
            .child(title),
    )
}

impl SettingsView {
    fn stepper(
        &self,
        id: &'static str,
        value: String,
        dec: PrefEdit,
        inc: PrefEdit,
        cx: &mut Context<Self>,
    ) -> Div {
        let tokens = cx.tokens().clone();
        let (p, radius) = (tokens.palette, tokens.radii.sm);
        let square = |id: (&'static str, usize),
                      label: &'static str,
                      edit: PrefEdit,
                      cx: &mut Context<Self>| {
            div()
                .id(id)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius)
                .border_1()
                .border_color(p.control_border)
                .bg(p.control)
                .text_color(p.text)
                .cursor_pointer()
                .hover(move |s| s.bg(p.control_hover))
                .active(move |s| s.bg(p.control_active))
                .on_click(cx.listener(move |this, _, _, cx| this.edit(edit.clone(), cx)))
                .child(label)
        };
        h_flex()
            .gap_1p5()
            .items_center()
            .child(square((id, 0), "\u{2212}", dec, cx))
            .child(
                div()
                    .min_w(px(52.))
                    .text_center()
                    .text_sm()
                    .text_color(p.text)
                    .child(value),
            )
            .child(square((id, 1), "+", inc, cx))
    }

    fn toggle(
        &self,
        id: &'static str,
        on: bool,
        make: fn(bool) -> PrefEdit,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = cx.tokens().palette;
        switch(
            id,
            on,
            p,
            cx.listener(move |this, _, _, cx| this.edit(make(!on), cx)),
        )
    }

    fn text_box(&self, input: &Entity<InputState>) -> Div {
        div().w(px(240.)).child(Input::new(input).small())
    }

    fn render_appearance(&self, cx: &mut Context<Self>) -> Div {
        let p = cx.tokens().palette;
        let ui = &self.settings.ui;
        let view = cx.entity();
        let labels: Vec<SharedString> = Appearance::all()
            .iter()
            .map(|a| SharedString::from(a.label()))
            .collect();
        let selected = Appearance::all()
            .iter()
            .position(|a| *a == self.settings.appearance)
            .unwrap_or(0);
        let appearance = segmented_control(
            "settings-appearance",
            &labels,
            selected,
            {
                let view = view.clone();
                move |index, _, cx| {
                    let value = Appearance::all()[index];
                    view.update(cx, |this, cx| this.edit(PrefEdit::Appearance(value), cx));
                }
            },
            cx,
        );
        let density_labels: Vec<SharedString> = Density::all()
            .iter()
            .map(|d| SharedString::from(d.label()))
            .collect();
        let density_selected = Density::all()
            .iter()
            .position(|d| *d == ui.density)
            .unwrap_or(0);
        let density = segmented_control(
            "settings-density",
            &density_labels,
            density_selected,
            move |index, _, cx| {
                let value = Density::all()[index];
                view.update(cx, |this, cx| this.edit(PrefEdit::Density(value), cx));
            },
            cx,
        );
        let font = self.stepper(
            "settings-ui-font",
            format!("{} px", ui.font_size),
            PrefEdit::UiFontSize(step(ui.font_size, -1, &UiPrefs::FONT_SIZE)),
            PrefEdit::UiFontSize(step(ui.font_size, 1, &UiPrefs::FONT_SIZE)),
            cx,
        );
        let zoom = self.stepper(
            "settings-zoom",
            format!("{}%", ui.zoom_percent),
            PrefEdit::Zoom(step(ui.zoom_percent, -25, &UiPrefs::ZOOM_PERCENT)),
            PrefEdit::Zoom(step(ui.zoom_percent, 25, &UiPrefs::ZOOM_PERCENT)),
            cx,
        );
        let mono_size = self.stepper(
            "settings-mono-size",
            format!("{} px", ui.mono_font_size),
            PrefEdit::MonoFontSize(step(ui.mono_font_size, -1, &UiPrefs::MONO_FONT_SIZE)),
            PrefEdit::MonoFontSize(step(ui.mono_font_size, 1, &UiPrefs::MONO_FONT_SIZE)),
            cx,
        );
        page("Appearance", p)
            .child(field_stacked(
                "Template",
                Some("Whole-window look. Standard is the original layout."),
                appearance,
                p,
            ))
            .child(field(
                "Density",
                Some("Row and control spacing."),
                density,
                p,
            ))
            .child(field("Interface font size", None, font, p))
            .child(field("Zoom", Some("Scales the whole window."), zoom, p))
            .child(field(
                "Monospace font",
                Some("Used for logs, YAML and terminals."),
                self.text_box(&self.mono_font),
                p,
            ))
            .child(field("Monospace font size", None, mono_size, p))
    }

    fn render_clusters(&self, cx: &mut Context<Self>) -> Div {
        let tokens = cx.tokens().clone();
        let (p, radius) = (tokens.palette, tokens.radii.sm);
        let home = dirs::home_dir();
        let sources: Vec<PathBuf> = self.settings.extra_kubeconfigs.clone();
        let list = if sources.is_empty() {
            div()
                .py_3()
                .text_sm()
                .text_color(p.text_muted)
                .child("No extra kubeconfig files or folders.")
        } else {
            div()
                .rounded(tokens.radii.md)
                .border_1()
                .border_color(p.control_border)
                .bg(p.surface)
                .children(sources.into_iter().enumerate().map(|(index, path)| {
                    let shown = display_path(&path, home.as_deref());
                    h_flex()
                        .h(px(34.))
                        .px_3()
                        .gap_3()
                        .items_center()
                        .justify_between()
                        .when(index > 0, |d| d.border_t_1().border_color(p.row_divider))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .text_color(p.text)
                                .child(shown),
                        )
                        .child(button(
                            ("settings-remove-kubeconfig", index),
                            "Remove",
                            p,
                            radius,
                            cx.listener(move |this, _, _, cx| {
                                this.command(AppCommand::RemoveKubeconfig(path.clone()), cx)
                            }),
                        ))
                }))
        };
        page("Clusters", p)
            .child(
                div()
                    .pb_2()
                    .text_sm()
                    .text_color(p.text_muted)
                    .child(kubeconfig_help(cfg!(windows))),
            )
            .child(list)
            .child(
                h_flex()
                    .pt_3()
                    .gap_2()
                    .flex_wrap()
                    .child(button(
                        "settings-add-kubeconfig",
                        "Add kubeconfig\u{2026}",
                        p,
                        radius,
                        cx.listener(|this, _, _, cx| this.command(AppCommand::AddKubeconfig, cx)),
                    ))
                    .child(button(
                        "settings-import-freelens",
                        "Import from Freelens",
                        p,
                        radius,
                        cx.listener(|this, _, _, cx| this.command(AppCommand::ImportFreelens, cx)),
                    ))
                    .child(button(
                        "settings-rescan",
                        "Rescan",
                        p,
                        radius,
                        cx.listener(|this, _, _, cx| this.command(AppCommand::ReloadClusters, cx)),
                    )),
            )
    }

    fn render_terminal(&self, cx: &mut Context<Self>) -> Div {
        let p = cx.tokens().palette;
        let size = self.settings.terminal.font_size;
        let font = self.stepper(
            "settings-term-font",
            format!("{size} px"),
            PrefEdit::TerminalFontSize(step(size, -1, &natikube_core::TerminalPrefs::FONT_SIZE)),
            PrefEdit::TerminalFontSize(step(size, 1, &natikube_core::TerminalPrefs::FONT_SIZE)),
            cx,
        );
        page("Terminal", p)
            .child(field("Font size", None, font, p))
            .child(field(
                "Shell",
                Some("Program for local terminals. Applies to new tabs."),
                self.text_box(&self.shell),
                p,
            ))
    }

    fn render_logs(&self, cx: &mut Context<Self>) -> Div {
        let p = cx.tokens().palette;
        let logs = &self.settings.logs;
        let timestamps = self.toggle(
            "settings-log-ts",
            logs.timestamps,
            PrefEdit::LogTimestamps,
            cx,
        );
        let wrap = self.toggle("settings-log-wrap", logs.wrap, PrefEdit::LogWrap, cx);
        page("Logs", p)
            .child(field(
                "Tail lines",
                Some("Lines requested when a log tab opens (10 to 100000)."),
                div()
                    .w(px(120.))
                    .child(Input::new(&self.tail_lines).small()),
                p,
            ))
            .child(field(
                "Show timestamps",
                Some("Default for new log tabs."),
                timestamps,
                p,
            ))
            .child(field(
                "Wrap long lines",
                Some("Default for new log tabs."),
                wrap,
                p,
            ))
    }

    fn render_data(&self, cx: &mut Context<Self>) -> Div {
        let p = cx.tokens().palette;
        let secs = self.settings.data.metrics_poll_secs;
        let poll = self.stepper(
            "settings-poll",
            format!("{secs} s"),
            PrefEdit::MetricsPollSecs(step(secs, -5, &natikube_core::DataPrefs::METRICS_POLL_SECS)),
            PrefEdit::MetricsPollSecs(step(secs, 5, &natikube_core::DataPrefs::METRICS_POLL_SECS)),
            cx,
        );
        page("Data", p)
            .child(field(
                "Metrics refresh",
                Some("How often CPU and memory usage are polled."),
                poll,
                p,
            ))
            .child(field(
                "Default namespace",
                Some("Used when a cluster has no saved namespace filter."),
                self.text_box(&self.namespace),
                p,
            ))
    }

    fn render_updates(&self, cx: &mut Context<Self>) -> Div {
        let p = cx.tokens().palette;
        let check = self.toggle(
            "settings-update-check",
            self.settings.updates.check_on_startup,
            PrefEdit::CheckUpdatesOnStartup,
            cx,
        );
        page("Updates", p).child(field(
            "Check for updates on startup",
            Some("One request to api.github.com when NatiKube opens. Nothing else is sent."),
            check,
            p,
        ))
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let items = SettingsSection::all().map(|section| {
            let selected = section == self.section;
            div()
                .id(("settings-section", section as usize))
                .px_3()
                .h(px(28.))
                .flex()
                .items_center()
                .rounded(tokens.radii.sm)
                .cursor_pointer()
                .text_sm()
                .when(selected, |d| {
                    d.bg(p.sidebar_active).text_color(p.sidebar_active_text)
                })
                .when(!selected, |d| {
                    d.text_color(p.text).hover(move |s| s.bg(p.control_hover))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.select_section(section, cx)))
                .child(section.label())
        });
        let body = match self.section {
            SettingsSection::Appearance => self.render_appearance(cx),
            SettingsSection::Clusters => self.render_clusters(cx),
            SettingsSection::Terminal => self.render_terminal(cx),
            SettingsSection::Logs => self.render_logs(cx),
            SettingsSection::Data => self.render_data(cx),
            SettingsSection::Updates => self.render_updates(cx),
        };
        h_flex()
            .size_full()
            .bg(p.background)
            .text_color(p.text)
            .font_family(tokens.fonts.ui.clone())
            .child(
                v_flex()
                    .flex_none()
                    .w(px(190.))
                    .h_full()
                    .p_2()
                    .gap_0p5()
                    .bg(p.sidebar)
                    .border_r_1()
                    .border_color(p.border)
                    .children(items),
            )
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px_6()
                    .py_5()
                    .child(body),
            )
    }
}

/// Opens the Settings window (760x560, centered) showing `settings`.
pub fn open_settings_window(
    settings: Settings,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<SettingsView>)> {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(760.), px(560.)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some("Settings".into()),
            ..TitlebarOptions::default()
        }),
        ..WindowOptions::default()
    };
    gpui_kit::open_window(options, cx, move |window, cx| {
        let view = cx.new(|cx| SettingsView::new(settings, window, cx));
        // Apply any text edit still waiting on its debounce before the window
        // goes away, so the last keystrokes are never lost.
        let closing = view.clone();
        window.on_window_should_close(cx, move |_, cx| {
            closing.update(cx, |view, cx| view.flush_pending(cx));
            true
        });
        view
    })
}
