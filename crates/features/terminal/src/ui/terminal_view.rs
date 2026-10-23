use std::time::Duration;

use alacritty_terminal::index::Side;
use futures::StreamExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use natikube_core::{AppCommand, ResourceRef};
use natikube_kube_client::ClusterConnection;

use super::paint::{self, Frame};
use crate::adapters::{Backend, BackendEvents, LocalPty, PodExec, login_shell_spec_for};
use crate::domain::mouse::{sgr_report, wheel_as_arrows, wheel_button};
use crate::domain::shell::{ClipboardAction, ShellFlavor, clipboard_action};
use crate::domain::{
    BackendEvent, GridMetrics, KeyMods, Palette, PtySize, ScrollAccumulator, TermSession,
    bracketed_paste, encode_key,
};

const SCROLLBACK_LINES: usize = 10_000;
/// Output arriving inside this window is parsed and drawn in one go.
const COALESCE: Duration = Duration::from_millis(6);
const MAX_BATCH: usize = 256;

struct ExecTarget {
    connection: ClusterConnection,
    pod: ResourceRef,
    container: Option<String>,
}

enum Kind {
    LocalShell,
    PodExec(Box<ExecTarget>),
}

/// A terminal tab: a local shell or an exec session into a pod container.
///
/// Contract: the shell creates one per `AppCommand::OpenLocalShell` /
/// `AppCommand::OpenExec` and shows [`title`](Self::title) on the dock tab.
/// Dropping the view ends the session (kills the child / closes the
/// websocket). It may emit `AppCommand::ShowError`.
pub struct TerminalView {
    kind: Kind,
    session: TermSession,
    backend: Option<Box<dyn Backend>>,
    focus: FocusHandle,
    palette: Palette,
    metrics: Option<GridMetrics>,
    origin: Point<Pixels>,
    scroll: ScrollAccumulator,
    selecting: bool,
    mouse_button_down: bool,
    exited: bool,
    _pump: Task<()>,
}

impl EventEmitter<AppCommand> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TerminalView {
    /// A local login shell (`$SHELL -l`; PowerShell on Windows) in the home
    /// directory.
    pub fn local_shell(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::local_shell_with(None, window, cx)
    }

    /// A local login shell running `shell` (`None` = the platform default).
    pub fn local_shell_with(
        shell: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        match LocalPty::spawn(&login_shell_spec_for(shell), PtySize::DEFAULT) {
            Ok((pty, events)) => {
                Self::build(Kind::LocalShell, Some(Box::new(pty)), events, window, cx)
            }
            Err(error) => {
                let message = format!("Could not start the local shell: {error}");
                Self::failed(Kind::LocalShell, message, window, cx)
            }
        }
    }

    /// An interactive shell in a pod container (tty + stdin over websocket).
    pub fn pod_exec(
        connection: ClusterConnection,
        pod: ResourceRef,
        container: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (exec, events) = PodExec::start(&connection, &pod, container.clone(), PtySize::DEFAULT);
        let kind = Kind::PodExec(Box::new(ExecTarget {
            connection,
            pod,
            container,
        }));
        Self::build(kind, Some(Box::new(exec)), events, window, cx)
    }

    fn build(
        kind: Kind,
        backend: Option<Box<dyn Backend>>,
        mut events: BackendEvents,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);

        // The task owns the receiver; dropping the view drops the task, which
        // drops the receiver, which makes the reader thread stop the child.
        let pump = cx.spawn(async move |this, cx| {
            while let Some(first) = events.next().await {
                let mut batch = vec![first];
                while batch.len() < MAX_BATCH
                    && let Ok(next) = events.try_recv()
                {
                    batch.push(next);
                }
                if this
                    .update(cx, |view, cx| view.apply_events(batch, cx))
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(COALESCE).await;
            }
        });

        Self {
            kind,
            session: TermSession::new(
                PtySize::DEFAULT.cols as usize,
                PtySize::DEFAULT.rows as usize,
                SCROLLBACK_LINES,
            ),
            backend,
            focus,
            palette: Palette::dark(),
            metrics: None,
            origin: point(px(0.), px(0.)),
            scroll: ScrollAccumulator::default(),
            selecting: false,
            mouse_button_down: false,
            exited: false,
            _pump: pump,
        }
    }

    /// A view with no session, showing why it could not start.
    fn failed(kind: Kind, message: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (_tx, rx) = futures::channel::mpsc::unbounded::<BackendEvent>();
        let mut view = Self::build(kind, None, rx, window, cx);
        view.report_failure(&message);
        // Subscribers attach right after construction, so emit on the next turn.
        let error = message;
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |_, cx| cx.emit(AppCommand::ShowError(error)));
        })
        .detach();
        view
    }

    /// Dock tab label (`Terminal` or `exec api-1/app`).
    pub fn title(&self) -> SharedString {
        match &self.kind {
            Kind::LocalShell => "Terminal".into(),
            Kind::PodExec(target) => match &target.container {
                Some(container) => format!("exec {}/{container}", target.pod.name).into(),
                None => format!("exec {}", target.pod.name).into(),
            },
        }
    }

    /// The title the running program set through the terminal (OSC 0/2).
    pub fn program_title(&self) -> Option<SharedString> {
        self.session
            .title()
            .map(|t| SharedString::from(t.to_string()))
    }

    /// The connection of an exec session (`None` for a local shell).
    pub fn connection(&self) -> Option<&ClusterConnection> {
        match &self.kind {
            Kind::LocalShell => None,
            Kind::PodExec(target) => Some(&target.connection),
        }
    }

    // ---- session plumbing -------------------------------------------------

    fn apply_events(&mut self, events: Vec<BackendEvent>, cx: &mut Context<Self>) {
        for event in events {
            match event {
                BackendEvent::Output(bytes) => {
                    self.session.feed(&bytes);
                    let replies = self.session.take_replies();
                    if !replies.is_empty() {
                        self.send(replies);
                    }
                    for text in self.session.take_clipboard_requests() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    self.session.take_bell();
                }
                BackendEvent::Exited(reason) => self.report_exit(&reason),
                BackendEvent::Failed(reason) => {
                    self.report_exit(&reason);
                    cx.emit(AppCommand::ShowError(reason));
                }
            }
        }
        cx.notify();
    }

    fn report_exit(&mut self, reason: &str) {
        self.exited = true;
        self.session
            .feed(format!("\r\n\x1b[2m[{reason}]\x1b[0m\r\n").as_bytes());
    }

    fn report_failure(&mut self, message: &str) {
        self.report_exit(message);
        self.backend = None;
    }

    fn send(&self, bytes: Vec<u8>) {
        if self.exited {
            return;
        }
        if let Some(backend) = &self.backend {
            backend.write(bytes);
        }
    }

    /// Called from prepaint with the final bounds: resizes the grid, the PTY
    /// and the remote terminal when the viewport changed.
    fn prepare(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Frame {
        let theme = cx.theme();
        let (fg, bg) = (
            paint::from_hsla(theme.foreground),
            paint::from_hsla(theme.background),
        );
        self.palette = Palette::dark().with_defaults(fg, bg);

        let (cell_w, cell_h) = paint::cell_size(window);
        let width: f32 = bounds.size.width.into();
        let height: f32 = bounds.size.height.into();
        let metrics = GridMetrics::fit(width, height, cell_w, cell_h);
        self.origin = bounds.origin;

        let changed = self
            .metrics
            .is_none_or(|old| old.cols != metrics.cols || old.rows != metrics.rows);
        self.metrics = Some(metrics);
        if changed {
            self.session.resize(metrics.cols, metrics.rows);
            self.session
                .set_cell_size(cell_w.round() as u16, cell_h.round() as u16);
            if let Some(backend) = &self.backend {
                backend.resize(PtySize {
                    cols: metrics.cols as u16,
                    rows: metrics.rows as u16,
                    cell_width: cell_w.round() as u16,
                    cell_height: cell_h.round() as u16,
                });
            }
        }

        Frame {
            snapshot: self.session.snapshot(&self.palette),
            metrics,
            focused: self.focus.is_focused(window),
            cursor_color: self.palette.cursor,
            background: self.palette.background,
        }
    }

    // ---- input ------------------------------------------------------------

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let mods = KeyMods {
            control: modifiers.control,
            alt: modifiers.alt,
            shift: modifiers.shift,
            platform: modifiers.platform,
        };

        // Cmd+C/V/A on macOS, Ctrl+Shift+C/V/A elsewhere (plain Ctrl+C stays
        // an interrupt for the shell).
        if let Some(action) = clipboard_action(
            ShellFlavor::current(),
            keystroke.key.as_str(),
            mods.control,
            mods.shift,
            mods.platform,
        ) {
            match action {
                ClipboardAction::Copy => self.copy(cx),
                ClipboardAction::Paste => self.paste(cx),
                ClipboardAction::SelectAll => {
                    self.session.select_all();
                    cx.notify();
                }
            }
            cx.stop_propagation();
            return;
        }
        // Any other Cmd chord belongs to the app, never to the shell.
        if mods.platform {
            return;
        }

        let snapshot_modes = self.session.mode();
        // Shift+PageUp/PageDown browse history on the primary screen.
        if mods.shift
            && !snapshot_modes.contains(alacritty_terminal::term::TermMode::ALT_SCREEN)
            && matches!(keystroke.key.as_str(), "pageup" | "pagedown")
        {
            let page = self.session.rows().saturating_sub(1) as i32;
            self.session.scroll(if keystroke.key == "pageup" {
                page
            } else {
                -page
            });
            cx.notify();
            cx.stop_propagation();
            return;
        }

        let app_cursor = snapshot_modes.contains(alacritty_terminal::term::TermMode::APP_CURSOR);
        if let Some(bytes) = encode_key(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            mods,
            app_cursor,
        ) {
            if !bytes.is_empty() {
                self.session.clear_selection();
                self.session.scroll_to_bottom();
                self.send(bytes);
                cx.notify();
            }
            cx.stop_propagation();
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.session.selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let bracketed = self
            .session
            .mode()
            .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE);
        self.session.scroll_to_bottom();
        self.send(bracketed_paste(&text, bracketed));
        cx.notify();
    }

    fn cell_for(&self, position: Point<Pixels>) -> Option<(usize, usize, Side)> {
        let metrics = self.metrics?;
        let x: f32 = (position.x - self.origin.x).into();
        let y: f32 = (position.y - self.origin.y).into();
        Some(metrics.cell_at(x, y))
    }

    fn mouse_tracking(&self) -> bool {
        use alacritty_terminal::term::TermMode;
        let mode = self.session.mode();
        mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE)
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let Some((row, col, side)) = self.cell_for(event.position) else {
            return;
        };
        if event.button != MouseButton::Left {
            return;
        }
        if self.mouse_tracking() && !event.modifiers.shift {
            self.mouse_button_down = true;
            self.send(sgr_report(0, col, row, true));
            return;
        }
        match event.click_count {
            2 => self.session.start_semantic_selection(row, col),
            3 => self.session.start_line_selection(row, col),
            _ => self.session.start_selection(row, col, side),
        }
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some((row, col, side)) = self.cell_for(event.position) else {
            return;
        };
        if self.mouse_button_down {
            use alacritty_terminal::term::TermMode;
            if self
                .session
                .mode()
                .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
            {
                self.send(sgr_report(32, col, row, true));
            }
            return;
        }
        if self.selecting {
            // Dragging above/below the viewport scrolls the history.
            let y: f32 = (event.position.y - self.origin.y).into();
            let height = self
                .metrics
                .map(|m| m.cell_height * m.rows as f32)
                .unwrap_or(0.0);
            if y < 0.0 {
                self.session.scroll(1);
            } else if y > height {
                self.session.scroll(-1);
            }
            self.session.update_selection(row, col, side);
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.mouse_button_down {
            self.mouse_button_down = false;
            if let Some((row, col, _)) = self.cell_for(event.position) {
                self.send(sgr_report(0, col, row, false));
            }
            return;
        }
        if self.selecting {
            self.selecting = false;
            if !self.session.has_selection() {
                self.session.clear_selection();
            }
            cx.notify();
        }
    }

    fn on_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use alacritty_terminal::term::TermMode;
        let Some(metrics) = self.metrics else {
            return;
        };
        let delta = event.delta.pixel_delta(px(metrics.cell_height));
        let dy: f32 = delta.y.into();
        let lines = self.scroll.push(dy, metrics.cell_height);
        if lines == 0 {
            return;
        }
        let up = lines > 0;
        let count = lines.unsigned_abs() as usize;
        let mode = self.session.mode();

        if self.mouse_tracking() && !event.modifiers.shift {
            let (row, col, _) = self.cell_for(event.position).unwrap_or((0, 0, Side::Left));
            for _ in 0..count {
                self.send(sgr_report(wheel_button(up), col, row, true));
            }
        } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            self.send(wheel_as_arrows(
                up,
                count,
                mode.contains(TermMode::APP_CURSOR),
            ));
        } else {
            self.session.scroll(lines);
            cx.notify();
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let background = theme.background;
        let view = cx.entity();

        let surface = canvas(
            move |bounds, window, cx| view.update(cx, |this, cx| this.prepare(bounds, window, cx)),
            paint::paint,
        )
        .size_full();

        div()
            .id("terminal")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .overflow_hidden()
            .bg(background)
            .px_2()
            .py_1()
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(surface)
    }
}
