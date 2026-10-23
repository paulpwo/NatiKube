use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use natikube_core::Appearance;
use natikube_core::{
    AppCommand, ClusterId, ResourceKind, ResourceRef, age_since, format_cpu, format_memory, kinds,
};
use natikube_ui_kit::{ActiveTokens as _, Palette, Tokens, Tone, container_squares, link_text};

use crate::domain::{
    ColumnKind, ColumnSpec, ObjectRow, ObjectTable, SortDirection, StatusTone, Text,
    appearance_columns, bar_fill, fit_widths_with_mins, header_min_width, row_actions, row_ref,
};
use crate::ui::resource_list::ResourceList;

pub(crate) fn tone(tone: StatusTone) -> Tone {
    match tone {
        StatusTone::Ok => Tone::Ok,
        StatusTone::Done => Tone::Done,
        StatusTone::Warning => Tone::Warning,
        StatusTone::Error => Tone::Error,
    }
}

/// Width changes below this many px do not re-lay out the columns.
const FIT_TOLERANCE: f32 = 6.0;

/// Width of the trailing actions column.
const ACTIONS_WIDTH: f32 = 44.0;

/// Index in `layout` of the trailing actions column.
const ACTIONS: usize = usize::MAX;

/// The slice of the appearance tokens a cell needs, copied out once per cell so
/// no borrow of the app is held while elements are built.
struct Look {
    appearance: Appearance,
    stacked_names: bool,
    status_pills: bool,
    usage_bars: bool,
    palette: Palette,
    mono: SharedString,
}

impl Look {
    fn read(cx: &App) -> Self {
        let tokens = cx.tokens();
        Self {
            appearance: tokens.appearance,
            stacked_names: tokens.stacked_names,
            status_pills: tokens.status_pills,
            usage_bars: tokens.usage_bars,
            palette: tokens.palette,
            mono: tokens.fonts.mono.clone(),
        }
    }

    fn standard(&self) -> bool {
        self.appearance == Appearance::Standard
    }

    /// Native marks the status with a dot instead of colored text.
    fn status_dot(&self) -> bool {
        self.appearance == Appearance::Native
    }
}

/// Display columns for `specs`: the namespace column folds into the name when
/// names are stacked. Returns the GPUI columns and, per display column, the
/// index of its spec ([`ACTIONS`] for the trailing menu).
fn build_columns(
    specs: &[ColumnSpec],
    appearance: Appearance,
    stacked_names: bool,
    scale: f32,
    fit: Option<f32>,
) -> (Vec<Column>, Vec<usize>) {
    let mut columns = Vec::new();
    let mut layout = Vec::new();
    let shown: Vec<(usize, &ColumnSpec, String)> =
        appearance_columns(specs, appearance, stacked_names)
            .into_iter()
            .map(|(ix, title)| {
                let spec = &specs[ix];
                // The warning marker column is 36 px wide: a title would be cut
                // ("Wari"), so its header stays empty (the cells carry an icon and
                // a tooltip).
                let title = match (spec.kind, title) {
                    (ColumnKind::Warning, _) => String::new(),
                    (_, Some(title)) => title.to_string(),
                    (_, None) => spec.title.clone(),
                };
                (ix, spec, title)
            })
            .collect();
    // A header needs room for its title and sort icon: widen narrow designs
    // and never shrink below it.
    let mut mins: Vec<f32> = shown
        .iter()
        .map(|(_, spec, title)| header_min_width(title, spec.sortable) * scale)
        .collect();
    let mut widths: Vec<f32> = shown
        .iter()
        .zip(&mins)
        .map(|((_, spec, _), min)| (spec.width * scale).max(*min))
        .collect();
    widths.push(ACTIONS_WIDTH * scale);
    mins.push(0.0);
    if let Some(available) = fit {
        widths = fit_widths_with_mins(&widths, &mins, available);
    }
    for ((ix, spec, title), width) in shown.into_iter().zip(widths.iter().copied()) {
        let mut column = Column::new(spec.key.clone(), title).width(px(width));
        if spec.sortable {
            column = column.sortable();
        }
        if spec.align_right {
            column = column.text_right();
        }
        columns.push(column);
        layout.push(ix);
    }
    columns.push(
        Column::new("actions", "")
            .width(px(widths[widths.len() - 1]))
            .resizable(false)
            .movable(false),
    );
    layout.push(ACTIONS);
    (columns, layout)
}

/// Value above a thin bar filled to `fill` (Command CPU / Memory cells).
fn usage_bar(text: String, fill: f32, look: &Look) -> AnyElement {
    v_flex()
        .w_full()
        .gap(px(5.))
        .child(
            div()
                .font_family(look.mono.clone())
                .text_size(px(12.))
                .child(text),
        )
        .child(
            div()
                .w_full()
                .h(px(3.))
                .rounded(px(2.))
                .bg(look.palette.border)
                .child(
                    div()
                        .h(px(3.))
                        .w(relative(fill))
                        .rounded(px(2.))
                        .bg(look.palette.accent),
                ),
        )
        .into_any_element()
}

/// Status as a tinted pill (Command) or a dot plus plain text (Native).
fn status_badge(text: SharedString, color: Hsla, foreground: Hsla, look: &Look) -> AnyElement {
    let dot = div()
        .flex_none()
        .size(px(if look.status_pills { 6. } else { 8. }))
        .rounded_full()
        .bg(color);
    if look.status_pills {
        h_flex()
            .gap(px(6.))
            .px(px(8.))
            .py(px(2.))
            .rounded_full()
            .bg(color.opacity(0.14))
            .text_color(color)
            .text_size(px(12.))
            .child(dot)
            .child(text)
            .into_any_element()
    } else {
        h_flex()
            .gap(px(6.))
            .text_color(foreground)
            .child(dot)
            .child(text)
            .into_any_element()
    }
}

fn is_status_column(spec: &ColumnSpec) -> bool {
    matches!(spec.kind, ColumnKind::Cell(_)) && spec.title == "Status"
}

/// Table delegate over the domain [`ObjectTable`]. Cells are built lazily, so
/// only the visible rows cost anything per frame.
pub struct ObjectsDelegate {
    pub table: ObjectTable,
    /// Unix seconds used for age columns; refreshed by the view.
    pub now: i64,
    /// The most recent timestamp whose age was drawn since the last
    /// [`take_rendered_newest`](Self::take_rendered_newest). Only what is on
    /// screen decides how often ages need redrawing.
    rendered_newest: std::cell::Cell<Option<i64>>,
    pub empty_message: SharedString,
    cluster: ClusterId,
    kind: ResourceKind,
    owner: WeakEntity<ResourceList>,
    columns: Vec<Column>,
    /// Display column to spec index (see [`build_columns`]).
    layout: Vec<usize>,
    /// The appearance the columns were last laid out for.
    look: Appearance,
    /// Width the columns were fitted to (`None` = designed widths).
    fit: Option<f32>,
    /// The UI scale the columns were laid out for.
    scale: f32,
}

impl ObjectsDelegate {
    pub fn new(
        table: ObjectTable,
        cluster: ClusterId,
        kind: ResourceKind,
        owner: WeakEntity<ResourceList>,
        now: i64,
    ) -> Self {
        let (columns, layout) =
            build_columns(table.columns(), Appearance::Standard, false, 1.0, None);
        Self {
            table,
            now,
            rendered_newest: std::cell::Cell::new(None),
            empty_message: "Loading…".into(),
            cluster,
            kind,
            owner,
            columns,
            layout,
            look: Appearance::Standard,
            fit: None,
            scale: 1.0,
        }
    }

    /// Records that an age computed from `timestamp` was drawn.
    pub(crate) fn note_age(&self, timestamp: Option<i64>) {
        if let Some(ts) = timestamp {
            self.rendered_newest
                .set(Some(self.rendered_newest.get().map_or(ts, |n| n.max(ts))));
        }
    }

    /// The newest timestamp drawn since the last call (clearing it).
    pub(crate) fn take_rendered_newest(&self) -> Option<i64> {
        self.rendered_newest.take()
    }

    /// The newest timestamp drawn so far, without clearing it.
    pub(crate) fn rendered_newest(&self) -> Option<i64> {
        self.rendered_newest.get()
    }

    /// The appearance the columns are laid out for.
    pub fn look(&self) -> Appearance {
        self.look
    }

    /// The UI scale the columns are laid out for.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Re-lays out the columns for `tokens`. Returns true when the set of
    /// columns changed, so the caller refreshes the table state.
    pub fn apply_look(&mut self, tokens: &Tokens) -> bool {
        let changed_look = self.look != tokens.appearance || self.scale != tokens.scale;
        self.look = tokens.appearance;
        self.scale = tokens.scale;
        let (columns, layout) = build_columns(
            self.table.columns(),
            tokens.appearance,
            tokens.stacked_names,
            tokens.scale,
            self.fit,
        );
        let changed = layout != self.layout;
        if changed {
            // Sort indices refer to specs, which the new layout may have moved.
            self.table.set_sort(None);
        }
        self.columns = columns;
        self.layout = layout;
        changed || changed_look
    }

    /// Fits the columns to `available` px (every appearance).
    /// Returns true when the widths changed, so the caller refreshes the
    /// table state.
    pub fn fit_to(&mut self, available: f32, tokens: &Tokens) -> bool {
        if self
            .fit
            .is_some_and(|f| (f - available).abs() < FIT_TOLERANCE)
        {
            return false;
        }
        self.fit = Some(available);
        let (columns, layout) = build_columns(
            self.table.columns(),
            tokens.appearance,
            tokens.stacked_names,
            tokens.scale,
            self.fit,
        );
        self.columns = columns;
        self.layout = layout;
        true
    }

    fn spec(&self, col_ix: usize) -> Option<&ColumnSpec> {
        match self.layout.get(col_ix) {
            Some(&ix) if ix != ACTIONS => self.table.columns().get(ix),
            _ => None,
        }
    }

    /// The row menu button, for layouts that draw their own rows.
    pub(crate) fn row_menu(&self, row_ix: usize, row: &ObjectRow) -> AnyElement {
        self.actions_menu(row_ix, row)
    }

    fn emit(owner: &WeakEntity<ResourceList>, command: AppCommand, cx: &mut App) {
        owner
            .update(cx, |list, cx| list.emit_command(command, cx))
            .ok();
    }

    fn link(
        &self,
        id: impl Into<ElementId>,
        label: SharedString,
        target: ResourceRef,
        cx: &App,
    ) -> AnyElement {
        let owner = self.owner.clone();
        link_text(
            id,
            label,
            move |_, _, cx| {
                cx.stop_propagation();
                Self::emit(&owner, AppCommand::OpenDetail(target.clone()), cx);
            },
            cx,
        )
        .into_any_element()
    }

    fn owner_target(&self, row: &ObjectRow, kind: &str, name: &str) -> Option<ResourceRef> {
        if name.is_empty() {
            return None;
        }
        let kind = kinds::find(&kind.to_ascii_lowercase())?;
        let namespace = kind
            .namespaced
            .then(|| row.namespace.as_deref().map(str::to_string))
            .flatten();
        Some(ResourceRef::new(
            self.cluster.clone(),
            kind,
            namespace,
            name.to_string(),
        ))
    }

    fn actions_menu(&self, row_ix: usize, row: &ObjectRow) -> AnyElement {
        let actions = row_actions(&self.cluster, &self.kind, row);
        let owner = self.owner.clone();
        let name = row.name.clone();
        Button::new(("row-menu", row_ix))
            .ghost()
            .xsmall()
            .label("⋮")
            .dropdown_menu(move |menu, _, _| fill_menu(menu, &actions, &owner, &name))
            .into_any_element()
    }
}

fn fill_menu(
    mut menu: PopupMenu,
    actions: &[crate::domain::RowAction],
    owner: &WeakEntity<ResourceList>,
    name: &str,
) -> PopupMenu {
    for action in actions {
        let (owner, command) = (owner.clone(), action.command.clone());
        menu = menu.item(
            PopupMenuItem::new(action.label.clone()).on_click(move |_, _, cx| {
                ObjectsDelegate::emit(&owner, command.clone(), cx);
            }),
        );
    }
    let name = name.to_string();
    menu.item(PopupMenuItem::separator())
        .item(PopupMenuItem::new("Copy name").on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
        }))
}

impl TableDelegate for ObjectsDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.table.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let Some(&spec_ix) = self.layout.get(col_ix).filter(|&&ix| ix != ACTIONS) else {
            return;
        };
        self.table.set_sort(match sort {
            ColumnSort::Default => None,
            ColumnSort::Ascending => Some((spec_ix, SortDirection::Ascending)),
            ColumnSort::Descending => Some((spec_ix, SortDirection::Descending)),
        });
        cx.notify();
    }

    /// Zebra striping per real row. The table's own `stripe` option also
    /// paints fake rows below the last item, so it stays off.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let stripe = natikube_ui_kit::zebra_background(row_ix, cx);
        div()
            .id(("row", row_ix))
            .when_some(stripe, |this, bg| this.bg(bg))
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(row) = self.table.row(row_ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (foreground, muted) = (theme.foreground, theme.muted_foreground);
        let look = Look::read(cx);
        let Some(spec) = self.spec(col_ix) else {
            // Trailing actions column.
            return h_flex()
                .size_full()
                .justify_center()
                .child(self.actions_menu(row_ix, row))
                .into_any_element();
        };
        let align_right = spec.align_right;
        let id = |what: &str| SharedString::from(format!("{what}-{row_ix}-{col_ix}"));

        let content: AnyElement = match spec.kind {
            ColumnKind::Name if look.stacked_names => v_flex()
                .min_w_0()
                .gap(px(2.))
                .child(
                    div()
                        .font_family(look.mono.clone())
                        .text_size(px(12.5))
                        .text_color(foreground)
                        .truncate()
                        .child(row.name.clone()),
                )
                .when_some(row.namespace.as_ref().map(shared), |this, ns| {
                    this.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(muted)
                            .truncate()
                            .child(ns),
                    )
                })
                .into_any_element(),
            ColumnKind::Name if !look.standard() => div()
                .font_family(look.mono.clone())
                .text_size(if look.status_dot() { px(11.) } else { px(12.5) })
                .text_color(foreground)
                .child(row.name.clone())
                .into_any_element(),
            ColumnKind::Name => div()
                .text_color(foreground)
                .child(row.name.clone())
                .into_any_element(),
            ColumnKind::Namespace => match &row.namespace {
                Some(ns) => {
                    let target = ResourceRef::new(
                        self.cluster.clone(),
                        kinds::NAMESPACE,
                        None,
                        ns.to_string(),
                    );
                    self.link(id("ns"), shared(ns), target, cx)
                }
                None => div().into_any_element(),
            },
            ColumnKind::Age => div()
                .text_color(muted)
                .child({
                    self.note_age(row.created_at);
                    age_since(row.created_at, self.now)
                })
                .into_any_element(),
            ColumnKind::LastSeen => div()
                .text_color(muted)
                .child({
                    self.note_age(row.updated_at);
                    age_since(row.updated_at, self.now)
                })
                .into_any_element(),
            ColumnKind::Cpu | ColumnKind::Memory => {
                let usage = self
                    .table
                    .usage()
                    .for_object(row.namespace.as_deref(), &row.name);
                let text = usage
                    .map(|u| match spec.kind {
                        ColumnKind::Cpu => format_cpu(u.cpu_cores),
                        _ => format_memory(u.memory_bytes),
                    })
                    .unwrap_or_default();
                if look.usage_bars && usage.is_some() {
                    let (peak_cpu, peak_mem) = self.table.usage_peak();
                    let fill = usage.map_or(0.0, |u| match spec.kind {
                        ColumnKind::Cpu => bar_fill(u.cpu_cores, peak_cpu),
                        _ => bar_fill(u.memory_bytes as f64, peak_mem as f64),
                    });
                    usage_bar(text, fill, &look)
                } else if !look.standard() {
                    div()
                        .font_family(look.mono.clone())
                        .text_size(px(12.))
                        .text_color(foreground)
                        .child(text)
                        .into_any_element()
                } else {
                    div().text_color(foreground).child(text).into_any_element()
                }
            }
            ColumnKind::Owner => match &row.owner {
                Some(owner) => match self.owner_target(row, &owner.kind, &owner.name) {
                    Some(target) => self.link(id("owner"), shared(&owner.kind), target, cx),
                    None => div()
                        .text_color(muted)
                        .child(shared(&owner.kind))
                        .into_any_element(),
                },
                None => div().into_any_element(),
            },
            ColumnKind::Object => match &row.owner {
                Some(owner) => {
                    let label = SharedString::from(format!("{}/{}", owner.kind, owner.name));
                    match self.owner_target(row, &owner.kind, &owner.name) {
                        Some(target) => self.link(id("object"), label.clone(), target, cx),
                        None => div().text_color(muted).child(label).into_any_element(),
                    }
                }
                None => div().into_any_element(),
            },
            ColumnKind::Warning => match row.health {
                StatusTone::Warning | StatusTone::Error => div()
                    .text_color(tone(row.health).color(cx))
                    .child("⚠")
                    .into_any_element(),
                _ => div().into_any_element(),
            },
            ColumnKind::Cell(ix) => match row.cells.get(ix) {
                Some(cell) if !cell.squares.is_empty() => {
                    let squares: Vec<Tone> = cell.squares.iter().copied().map(tone).collect();
                    container_squares(&squares, cx).into_any_element()
                }
                Some(cell)
                    if is_status_column(spec) && (look.status_pills || look.status_dot()) =>
                {
                    let color = cell.tone.map_or(muted, |t| tone(t).color(cx));
                    status_badge(shared(&cell.text), color, foreground, &look)
                }
                Some(cell) => {
                    let color = cell.tone.map(|t| tone(t).color(cx)).unwrap_or(foreground);
                    div()
                        .text_color(color)
                        .child(shared(&cell.text))
                        .into_any_element()
                }
                None => div().into_any_element(),
            },
        };

        h_flex()
            .size_full()
            .overflow_hidden()
            .whitespace_nowrap()
            .when(align_right, |this| this.justify_end())
            .child(content)
            .into_any_element()
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        match self.table.row(row_ix) {
            Some(row) => {
                let actions = row_actions(&self.cluster, &self.kind, row);
                fill_menu(menu, &actions, &self.owner, &row.name)
            }
            None => menu,
        }
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        h_flex()
            .size_full()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(self.empty_message.clone())
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        let Some(row) = self.table.row(row_ix) else {
            return String::new();
        };
        let Some(spec) = self.spec(col_ix) else {
            return String::new();
        };
        match spec.kind {
            ColumnKind::Name => row.name.clone(),
            ColumnKind::Namespace => row.namespace.as_deref().unwrap_or_default().to_string(),
            ColumnKind::Age => age_since(row.created_at, self.now),
            ColumnKind::LastSeen => age_since(row.updated_at, self.now),
            ColumnKind::Cpu | ColumnKind::Memory => self
                .table
                .usage()
                .for_object(row.namespace.as_deref(), &row.name)
                .map(|u| match spec.kind {
                    ColumnKind::Cpu => format_cpu(u.cpu_cores),
                    _ => format_memory(u.memory_bytes),
                })
                .unwrap_or_default(),
            ColumnKind::Owner => row
                .owner
                .as_ref()
                .map(|o| o.kind.to_string())
                .unwrap_or_default(),
            ColumnKind::Object => row
                .owner
                .as_ref()
                .map(|o| format!("{}/{}", o.kind, o.name))
                .unwrap_or_default(),
            ColumnKind::Warning => String::new(),
            ColumnKind::Cell(ix) => row
                .cells
                .get(ix)
                .map(|c| c.text.to_string())
                .unwrap_or_default(),
        }
    }
}

/// Zero-copy conversion of a shared row string into a UI string.
fn shared(text: &Text) -> SharedString {
    SharedString::from(text.as_arc().clone())
}

/// The reference of the row at `visible_ix` (the table must know the kind).
pub(crate) fn ref_at(delegate: &ObjectsDelegate, visible_ix: usize) -> Option<ResourceRef> {
    delegate
        .table
        .row(visible_ix)
        .map(|row| row_ref(&delegate.cluster, &delegate.kind, row))
}
