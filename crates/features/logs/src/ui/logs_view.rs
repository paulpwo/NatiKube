use std::time::Duration;

use futures::StreamExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Selectable as _, h_flex, v_flex};
use gpui_kit::*;
use natikube_core::{AppCommand, LogPrefs, ResourceRef};
use natikube_kube_client::{ClusterConnection, spawn, spawn_stream};
use natikube_ui_kit::{ActiveTokens as _, empty_state};

use crate::adapters::{LogEvent, LogRequest, PodContainers, pod_containers, stream_logs};
use crate::domain::{DEFAULT_CAPACITY, LogBuffer, LogSearch};

/// Minimum time between two redraws caused by incoming lines.
const BATCH_WINDOW: Duration = Duration::from_millis(60);
/// In wrap mode rows have variable height and cannot be virtualized with a
/// uniform list, so only the newest lines are laid out.
const WRAP_WINDOW: usize = 1500;
const LINE_HEIGHT: f32 = 18.0;

/// A log tab for one pod (and optionally one container).
///
/// Contract: the shell creates one per `AppCommand::OpenLogs` and shows
/// [`title`](Self::title) on the dock tab. Dropping the view cancels its
/// stream. It may emit `AppCommand::ShowError`.
pub struct LogsView {
    connection: ClusterConnection,
    pod: ResourceRef,
    container: Option<String>,
    containers: PodContainers,
    previous: bool,
    show_timestamps: bool,
    wrap: bool,
    tail_lines: i64,
    follow: bool,
    buffer: LogBuffer,
    search: LogSearch,
    search_input: Entity<InputState>,
    matches: Vec<usize>,
    current_match: Option<usize>,
    status: SharedString,
    scroll: UniformListScrollHandle,
    wrap_scroll: ScrollHandle,
    /// Owns the stream: replacing or dropping it cancels the producer.
    _pump: Option<Task<()>>,
    _bootstrap: Option<Task<()>>,
    _search_events: Subscription,
}

impl EventEmitter<AppCommand> for LogsView {}

impl LogsView {
    pub fn new(
        connection: ClusterConnection,
        pod: ResourceRef,
        container: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_prefs(connection, pod, container, &LogPrefs::default(), window, cx)
    }

    /// Like [`new`](Self::new) with the user's defaults (Settings → Logs):
    /// lines to tail, timestamps and wrap on open.
    pub fn with_prefs(
        connection: ClusterConnection,
        pod: ResourceRef,
        container: Option<String>,
        prefs: &LogPrefs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search logs…"));
        let _search_events = cx.subscribe(
            &search_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    this.set_query(query, cx);
                }
                InputEvent::PressEnter { shift, .. } => this.step_match(!*shift, cx),
                _ => {}
            },
        );

        let mut view = Self {
            connection,
            pod,
            container,
            containers: PodContainers::default(),
            previous: false,
            show_timestamps: prefs.timestamps,
            wrap: prefs.wrap,
            tail_lines: i64::from(prefs.tail_lines),
            follow: true,
            buffer: LogBuffer::new(DEFAULT_CAPACITY.max(prefs.tail_lines as usize)),
            search: LogSearch::default(),
            search_input,
            matches: Vec::new(),
            current_match: None,
            status: "Connecting…".into(),
            scroll: UniformListScrollHandle::new(),
            wrap_scroll: ScrollHandle::new(),
            _pump: None,
            _bootstrap: None,
            _search_events,
        };
        view.bootstrap(cx);
        view
    }

    pub fn connection(&self) -> &ClusterConnection {
        &self.connection
    }

    /// Dock tab label, e.g. `api-7d9f/app`.
    pub fn title(&self) -> SharedString {
        match &self.container {
            Some(container) => format!("{}/{container}", self.pod.name).into(),
            None => self.pod.name.clone().into(),
        }
    }

    /// Number of lines currently buffered.
    pub fn line_count(&self) -> usize {
        self.buffer.len()
    }

    /// Reads the container list, then starts streaming. A failed lookup is not
    /// fatal: the stream starts with whatever container was requested.
    fn bootstrap(&mut self, cx: &mut Context<Self>) {
        let client = self.connection.client().clone();
        let pod = self.pod.clone();
        let task = spawn(async move { pod_containers(client, &pod).await });
        self._bootstrap = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(Ok(containers)) => {
                        if view.container.is_none() {
                            view.container = containers.default_container().map(str::to_string);
                        }
                        view.containers = containers;
                    }
                    Ok(Err(message)) => cx.emit(AppCommand::ShowError(format!(
                        "Could not read containers of {}: {message}",
                        view.pod.name
                    ))),
                    Err(_) => {}
                }
                view.restart(cx);
            });
        }));
    }

    /// (Re)opens the stream for the current container/previous settings.
    fn restart(&mut self, cx: &mut Context<Self>) {
        self.buffer.clear();
        self.matches.clear();
        self.current_match = None;
        self.status = if self.previous {
            "Loading previous container logs…"
        } else {
            "Streaming…"
        }
        .into();

        let mut request = LogRequest::new(self.pod.clone(), self.container.clone());
        request.previous = self.previous;
        request.tail_lines = Some(self.tail_lines);
        let mut events = spawn_stream(stream_logs(self.connection.client().clone(), request));
        self._pump = Some(cx.spawn(async move |this, cx| {
            while let Some(first) = events.next().await {
                let mut batch = vec![first];
                while let Some(next) = events.try_recv() {
                    batch.push(next);
                }
                if this.update(cx, |view, cx| view.apply(batch, cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(BATCH_WINDOW).await;
            }
        }));
        cx.notify();
    }

    fn apply(&mut self, batch: Vec<LogEvent>, cx: &mut Context<Self>) {
        let before = self.buffer.len();
        let evicted_before = self.buffer.evicted();
        for event in batch {
            match event {
                LogEvent::Line(line) => self.buffer.push_line(&line),
                LogEvent::Ended => {
                    self.status = if self.previous {
                        "Previous container log loaded".into()
                    } else {
                        "Stream ended".into()
                    };
                }
                LogEvent::Error(message) => {
                    self.status = format!("Error: {message}").into();
                    cx.emit(AppCommand::ShowError(format!(
                        "Logs of {}: {message}",
                        self.pod.name
                    )));
                }
            }
        }
        if self.buffer.len() != before || self.buffer.evicted() != evicted_before {
            if !self.search.is_empty() {
                // Eviction shifts indexes, so recompute rather than patch.
                self.recompute_matches();
            }
            if self.follow {
                self.scroll_to_end();
            }
        }
        cx.notify();
    }

    fn scroll_to_end(&mut self) {
        let last = self.visible_count().saturating_sub(1);
        self.scroll.scroll_to_item(last, ScrollStrategy::Bottom);
        self.wrap_scroll.scroll_to_bottom();
    }

    fn visible_count(&self) -> usize {
        self.buffer.len()
    }

    fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.search = LogSearch::new(query);
        self.recompute_matches();
        self.current_match = if self.matches.is_empty() {
            None
        } else {
            Some(self.matches.len() - 1)
        };
        self.reveal_current();
        cx.notify();
    }

    fn recompute_matches(&mut self) {
        self.matches = self
            .search
            .matching_lines(&self.buffer, self.show_timestamps);
        self.current_match = self
            .current_match
            .filter(|_| !self.matches.is_empty())
            .map(|i| i.min(self.matches.len() - 1));
    }

    fn step_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len();
        self.current_match = Some(match (self.current_match, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        });
        self.follow = false;
        self.reveal_current();
        cx.notify();
    }

    fn reveal_current(&mut self) {
        if let Some(line) = self.current_match.and_then(|i| self.matches.get(i)) {
            self.scroll.scroll_to_item(*line, ScrollStrategy::Center);
        }
    }

    fn select_container(&mut self, name: String, cx: &mut Context<Self>) {
        if self.container.as_deref() != Some(&name) {
            self.container = Some(name);
            self.restart(cx);
        }
    }

    fn toggle_previous(&mut self, cx: &mut Context<Self>) {
        self.previous = !self.previous;
        self.restart(cx);
    }

    fn toggle_timestamps(&mut self, cx: &mut Context<Self>) {
        self.show_timestamps = !self.show_timestamps;
        self.recompute_matches();
        cx.notify();
    }

    fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        self.follow = !self.follow;
        if self.follow {
            self.scroll_to_end();
        }
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.buffer.to_text(self.show_timestamps);
        let suggested = format!(
            "{}{}.log",
            self.pod.name,
            self.container
                .as_deref()
                .map(|c| format!("-{c}"))
                .unwrap_or_default()
        );
        cx.spawn(async move |this, cx| {
            let Some(file) = rfd::AsyncFileDialog::new()
                .set_file_name(suggested)
                .save_file()
                .await
            else {
                return;
            };
            let path = file.path().to_path_buf();
            let result = cx
                .background_executor()
                .spawn(async move { std::fs::write(&path, text).map(|_| path) })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok(path) => {
                    view.status = format!("Saved to {}", path.display()).into();
                    cx.notify();
                }
                Err(e) => cx.emit(AppCommand::ShowError(format!("Could not save logs: {e}"))),
            });
        })
        .detach();
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let containers = self.containers.all();
        let match_label = match (self.current_match, self.matches.len()) {
            (_, 0) if self.search.is_empty() => String::new(),
            (_, 0) => "No matches".to_string(),
            (Some(i), n) => format!("{}/{n}", i + 1),
            (None, n) => format!("{n} matches"),
        };
        h_flex()
            .w_full()
            .flex_none()
            .px_3()
            .py_1()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .children(containers.into_iter().map(|name| {
                let selected = self.container.as_deref() == Some(name.as_str());
                let target = name.clone();
                Button::new(SharedString::from(format!("container-{name}")))
                    .label(name)
                    .ghost()
                    .compact()
                    .selected(selected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_container(target.clone(), cx);
                    }))
            }))
            .child(
                Button::new("previous")
                    .label("Previous")
                    .tooltip("Show the previous (terminated) container's logs")
                    .ghost()
                    .compact()
                    .selected(self.previous)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_previous(cx))),
            )
            .child(
                Button::new("timestamps")
                    .label("Timestamps")
                    .ghost()
                    .compact()
                    .selected(self.show_timestamps)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_timestamps(cx))),
            )
            .child(
                Button::new("wrap")
                    .label("Wrap")
                    .ghost()
                    .compact()
                    .selected(self.wrap)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.wrap = !this.wrap;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("follow")
                    .label("Follow")
                    .ghost()
                    .compact()
                    .selected(self.follow)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_follow(cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .max_w(px(320.))
                    .child(Input::new(&self.search_input)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(match_label),
            )
            .child(
                Button::new("prev-match")
                    .label("↑")
                    .ghost()
                    .compact()
                    .on_click(cx.listener(|this, _, _, cx| this.step_match(false, cx))),
            )
            .child(
                Button::new("next-match")
                    .label("↓")
                    .ghost()
                    .compact()
                    .on_click(cx.listener(|this, _, _, cx| this.step_match(true, cx))),
            )
            .child(div().flex_1())
            .child(
                Button::new("save")
                    .label("Save…")
                    .tooltip("Save the buffered logs to a file")
                    .ghost()
                    .compact()
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
    }

    /// One rendered line, with search hits highlighted.
    fn line_element(&self, index: usize, cx: &App) -> Div {
        let theme = cx.theme();
        let Some(line) = self.buffer.get(index) else {
            return div();
        };
        let text = line.display(self.show_timestamps);
        let is_current = self
            .current_match
            .and_then(|i| self.matches.get(i))
            .is_some_and(|m| *m == index);
        let hit = HighlightStyle {
            color: Some(theme.foreground),
            background_color: Some(if is_current {
                theme.warning
            } else {
                theme.warning.opacity(0.35)
            }),
            ..Default::default()
        };
        let highlights: Vec<_> = self
            .search
            .ranges(&text)
            .into_iter()
            .map(|r| (r, hit))
            .collect();
        let (mono_family, mono_size) = {
            let tokens = cx.tokens();
            (tokens.fonts.mono.clone(), tokens.mono_size)
        };
        let line_height = px(LINE_HEIGHT.max((f32::from(mono_size) * 1.5).ceil()));
        let base = div()
            .w_full()
            .px_3()
            .text_size(mono_size)
            .font_family(mono_family)
            .text_color(theme.foreground);
        let styled = StyledText::new(text).with_highlights(highlights);
        if self.wrap {
            base.min_h(line_height).child(styled)
        } else {
            base.h(line_height).whitespace_nowrap().child(styled)
        }
    }
}

impl Render for LogsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = cx.theme().background;
        let fg = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;

        let body = if self.buffer.is_empty() {
            div()
                .flex_1()
                .child(empty_state(
                    format!("Logs: {}", self.title()),
                    self.status.to_string(),
                    cx,
                ))
                .into_any_element()
        } else if self.wrap {
            let start = self.buffer.len().saturating_sub(WRAP_WINDOW);
            div()
                .id("logs-wrap")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.wrap_scroll)
                .children((start..self.buffer.len()).map(|i| self.line_element(i, cx)))
                .into_any_element()
        } else {
            uniform_list(
                "logs-lines",
                self.buffer.len(),
                cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                    range.map(|i| this.line_element(i, cx)).collect::<Vec<_>>()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .into_any_element()
        };

        let mut footer = format!("{} lines", self.buffer.len());
        if self.buffer.evicted() > 0 {
            footer.push_str(&format!(" ({} older dropped)", self.buffer.evicted()));
        }
        if self.wrap && self.buffer.len() > WRAP_WINDOW {
            footer.push_str(&format!(" · wrap shows the last {WRAP_WINDOW}"));
        }

        v_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .child(self.toolbar(cx))
            .child(body)
            .child(
                h_flex()
                    .w_full()
                    .flex_none()
                    .px_3()
                    .py_0p5()
                    .gap_3()
                    .border_t_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted)
                    .child(footer)
                    .child(div().flex_1().text_right().child(self.status.clone())),
            )
    }
}
