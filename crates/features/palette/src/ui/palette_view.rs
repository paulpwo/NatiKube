use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::AppCommand;
use natikube_ui_kit::ActiveTokens;

use crate::domain::{PaletteContext, PaletteItem, build_items, rank};

const MAX_ROWS: usize = 50;

/// The ⌘K palette overlay.
///
/// Contract: the shell creates one instance (`cx.new(|cx| PaletteView::new(window, cx))`),
/// subscribes to its [`AppCommand`] events, renders it as the last child of the
/// window root (it paints nothing while closed), calls
/// [`open`](Self::open) on `AppCommand::OpenPalette` and
/// [`close`](Self::close) on `ClosePalette`. Choosing an item emits that item's
/// command and then `AppCommand::ClosePalette`. Escape closes it.
pub struct PaletteView {
    open: bool,
    context: PaletteContext,
    items: Vec<PaletteItem>,
    visible: Vec<PaletteItem>,
    selected: usize,
    input: Entity<InputState>,
    list_scroll: ScrollHandle,
    _subscription: Subscription,
}

impl EventEmitter<AppCommand> for PaletteView {}

impl PaletteView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Type a command or search…"));
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this: &mut Self, _state, event: &InputEvent, _window, cx| match event {
                InputEvent::Change => {
                    this.refilter(cx);
                }
                InputEvent::PressEnter { .. } => this.choose(this.selected, cx),
                _ => {}
            },
        );
        Self {
            open: false,
            context: PaletteContext::default(),
            items: Vec::new(),
            visible: Vec::new(),
            selected: 0,
            input,
            list_scroll: ScrollHandle::new(),
            _subscription: subscription,
        }
    }

    pub fn set_context(&mut self, context: PaletteContext, cx: &mut Context<Self>) {
        self.items = build_items(&context);
        self.context = context;
        self.refilter(cx);
    }

    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.items = build_items(&self.context);
        self.input.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.focus(window, cx);
        });
        self.refilter(cx);
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn context(&self) -> &PaletteContext {
        &self.context
    }

    fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.visible = rank(&self.items, &query, MAX_ROWS);
        self.selected = 0;
        self.list_scroll.scroll_to_item(0);
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.visible.is_empty() {
            return;
        }
        let last = self.visible.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        self.list_scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn choose(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(item) = self.visible.get(ix).cloned() else {
            return;
        };
        self.open = false;
        cx.emit(item.command);
        cx.emit(AppCommand::ClosePalette);
        cx.notify();
    }

    fn on_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "down" => self.move_selection(1, cx),
            "up" => self.move_selection(-1, cx),
            "escape" => {
                self.open = false;
                cx.emit(AppCommand::ClosePalette);
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl Render for PaletteView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let tokens = cx.tokens().clone();
        let p = tokens.palette;
        let radius = tokens.radii.lg;
        let rows = self.visible.iter().enumerate().map(|(ix, item)| {
            let selected = ix == self.selected;
            div()
                .id(("palette-row", ix))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .px_3()
                .h(tokens.row_height + px(6.))
                .rounded(tokens.radii.sm)
                .cursor_pointer()
                .text_color(p.text)
                .when(selected, |d| d.bg(p.accent))
                .when(!selected, |d| d.hover(move |d| d.bg(p.panel_raised)))
                .on_click(cx.listener(move |this, _, _, cx| this.choose(ix, cx)))
                .child(div().flex_1().truncate().child(item.label.clone()))
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(if selected { p.text } else { p.text_muted })
                        .child(item.hint.clone()),
                )
        });
        let empty = self.visible.is_empty();

        div()
            .id("palette-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(96.))
            .bg(gpui_kit::black().opacity(0.45))
            .occlude()
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.on_key(event, cx);
            }))
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = false;
                cx.emit(AppCommand::ClosePalette);
                cx.notify();
            }))
            .child(
                div()
                    .id("palette-panel")
                    .w(px(560.))
                    .max_h(px(440.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_2()
                    .bg(p.panel)
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius)
                    .shadow_lg()
                    .font_family(tokens.fonts.ui.clone())
                    .text_size(tokens.font_size)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(Input::new(&self.input).appearance(false).cleanable(false))
                    .child(
                        div()
                            .id("palette-list")
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .overflow_y_scroll()
                            .track_scroll(&self.list_scroll)
                            .children(rows)
                            .when(empty, |d| {
                                d.child(
                                    div()
                                        .px_3()
                                        .py_2()
                                        .text_color(p.text_muted)
                                        .child("No matches"),
                                )
                            }),
                    ),
            )
            .into_any_element()
    }
}
