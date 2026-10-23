//! Freelens-like dark theme: tokens plus the function that installs them into
//! GPUI Kit's global theme.

use gpui_kit::component::{ActiveTheme as _, Theme};
use gpui_kit::*;

/// Hex literal (`0xRRGGBB`) to a GPUI color.
pub fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// Raw design tokens. Widgets read the installed theme (`cx.theme()`) for the
/// common slots and `cx.tokens().palette` for the appearance-specific ones
/// (pills, usage bars, selection, zebra rows).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Window and content background (~#1e2124).
    pub background: Hsla,
    /// Panels, sidebar, table header (~#2b3035).
    pub panel: Hsla,
    /// Slightly lighter surface for hover and zebra rows.
    pub panel_raised: Hsla,
    /// Cluster rail background.
    pub rail: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    /// Selection / active highlight (#3d90ce).
    pub accent: Hsla,
    /// Secondary accent (#00a7a0).
    pub teal: Hsla,
    pub ok: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
    /// Completed / terminated, neither good nor bad.
    pub done: Hsla,
    pub link: Hsla,

    // ---- Additive fields (appearances) -------------------------------
    /// Sidebar background.
    pub sidebar: Hsla,
    /// Title bar and status bar background.
    pub chrome: Hsla,
    /// Card / table container surface (KPI cards, namespace groups).
    pub surface: Hsla,
    /// Header strip of a surface (group header, card header).
    pub surface_alt: Hsla,
    /// Buttons, inputs, chips, segmented-control track.
    pub control: Hsla,
    pub control_hover: Hsla,
    pub control_active: Hsla,
    pub control_border: Hsla,
    /// Divider between table rows.
    pub row_divider: Hsla,
    /// Brightest text (headings, active nav item).
    pub text_strong: Hsla,
    /// Selected chip / active filter background and its text.
    pub accent_soft: Hsla,
    pub accent_soft_text: Hsla,
    /// Text on a solid accent background.
    pub on_accent: Hsla,
    pub primary_hover: Hsla,
    pub primary_active: Hsla,
    /// Selected table row (solid in Native, tinted elsewhere) and its text.
    pub selected_row: Hsla,
    pub selected_row_text: Hsla,
    /// Alternate row background used when `Tokens::zebra` is on.
    pub zebra: Hsla,
    /// Active sidebar item background and text.
    pub sidebar_active: Hsla,
    pub sidebar_active_text: Hsla,
    /// Status pill backgrounds and text per tone.
    pub ok_soft: Hsla,
    pub ok_text: Hsla,
    pub warning_soft: Hsla,
    pub warning_text: Hsla,
    pub error_soft: Hsla,
    pub error_text: Hsla,
    pub done_soft: Hsla,
    /// Usage bar track and fill.
    pub bar_track: Hsla,
    pub bar_fill: Hsla,
    /// Outline of keyboard hint chips.
    pub kbd_border: Hsla,
    pub link_hover: Hsla,
    pub scrollbar_thumb: Hsla,
    pub scrollbar_thumb_hover: Hsla,
}

impl Palette {
    /// Today's UI. Every additive field reproduces the value that used to be
    /// hard-coded in `install_palette`, so Standard looks identical.
    pub fn freelens_dark() -> Self {
        let background = hex(0x1e2124);
        let panel = hex(0x2b3035);
        let panel_raised = hex(0x32383e);
        let border = hex(0x383d43);
        let text = hex(0xd5d9de);
        let accent = hex(0x3d90ce);
        let ok = hex(0x4caf50);
        let warning = hex(0xff9800);
        let error = hex(0xce3933);
        let done = hex(0x7d8590);
        Self {
            background,
            panel,
            panel_raised,
            rail: hex(0x181a1d),
            border,
            text,
            text_muted: hex(0x8b949e),
            accent,
            teal: hex(0x00a7a0),
            ok,
            warning,
            error,
            done,
            link: hex(0x5fb0e8),
            sidebar: panel,
            chrome: panel,
            surface: background,
            surface_alt: panel,
            control: panel_raised,
            control_hover: hex(0x3a4148),
            control_active: hex(0x2f353b),
            control_border: border,
            row_divider: border.opacity(0.6),
            text_strong: text,
            accent_soft: accent.opacity(0.22),
            accent_soft_text: text,
            on_accent: hex(0xffffff),
            primary_hover: hex(0x58a3dd),
            primary_active: hex(0x3480ba),
            selected_row: accent.opacity(0.22),
            selected_row_text: text,
            zebra: background,
            sidebar_active: accent.opacity(0.28),
            sidebar_active_text: hex(0xffffff),
            ok_soft: ok.opacity(0.15),
            ok_text: ok,
            warning_soft: warning.opacity(0.15),
            warning_text: warning,
            error_soft: error.opacity(0.15),
            error_text: error,
            done_soft: done.opacity(0.15),
            bar_track: border,
            bar_fill: accent,
            kbd_border: border,
            link_hover: hex(0x8cc8f2),
            scrollbar_thumb: hex(0x4a5159),
            scrollbar_thumb_hover: hex(0x626a73),
        }
    }

    /// Command direction (Main.dc.html): ink-black canvas, indigo accent,
    /// filled status pills, hairline borders.
    pub fn command() -> Self {
        let background = hex(0x0e0f12);
        let border = hex(0x1f2229);
        let ok = hex(0x3fb950);
        let warning = hex(0xe3b341);
        let error = hex(0xf85149);
        let muted = hex(0x8b9099);
        let indigo = hex(0x6e8bff);
        Self {
            background,
            panel: hex(0x121418),
            panel_raised: hex(0x1a1d24),
            rail: background,
            border,
            text: hex(0xe6e8eb),
            text_muted: muted,
            accent: indigo,
            teal: hex(0x39c5cf),
            ok,
            warning,
            error,
            done: muted,
            link: hex(0x9db0ff),
            sidebar: background,
            chrome: background,
            surface: hex(0x121418),
            surface_alt: hex(0x15171c),
            control: hex(0x15171c),
            control_hover: hex(0x1a1d24),
            control_active: hex(0x1f2229),
            control_border: hex(0x262a33),
            row_divider: hex(0x1b1e24),
            text_strong: hex(0xffffff),
            accent_soft: hex(0x2b3550),
            accent_soft_text: hex(0xc4d0ff),
            on_accent: hex(0x0e0f12),
            primary_hover: hex(0x8aa2ff),
            primary_active: hex(0x5a77ee),
            selected_row: hex(0x1a1d24),
            selected_row_text: hex(0xe6e8eb),
            zebra: background,
            sidebar_active: hex(0x1a1d24),
            sidebar_active_text: hex(0xffffff),
            ok_soft: hex(0x13261a),
            ok_text: hex(0x56d364),
            warning_soft: hex(0x2b2412),
            warning_text: hex(0xf0c987),
            error_soft: hex(0x2d1618),
            error_text: hex(0xff7b72),
            done_soft: hex(0x1f2229),
            bar_track: hex(0x1f2229),
            bar_fill: indigo,
            kbd_border: hex(0x2e323c),
            link_hover: hex(0xc4d0ff),
            scrollbar_thumb: hex(0x2e323c),
            scrollbar_thumb_hover: hex(0x3d4350),
        }
    }

    /// macOS Native, light (MacNative.dc.html): system grays, system blue
    /// solid selection, zebra rows.
    pub fn native_light() -> Self {
        let background = hex(0xffffff);
        let border = hex(0xe5e5ea);
        let ok = hex(0x34c759);
        let warning = hex(0xff9f0a);
        let error = hex(0xff3b30);
        let muted = hex(0x6e6e73);
        let blue = hex(0x0a84ff);
        Self {
            background,
            panel: hex(0xfafafb),
            panel_raised: hex(0xeeeef0),
            rail: hex(0xf2f2f4),
            border,
            text: hex(0x1d1d1f),
            text_muted: muted,
            accent: blue,
            teal: hex(0x30b0c7),
            ok,
            warning,
            error,
            done: muted,
            link: hex(0x0062cc),
            sidebar: hex(0xf2f2f4),
            chrome: hex(0xf2f2f4),
            surface: background,
            surface_alt: hex(0xfafafb),
            control: hex(0xeeeef0),
            control_hover: hex(0xe3e3e8),
            control_active: hex(0xd8d8de),
            control_border: hex(0xd1d1d6),
            row_divider: border,
            text_strong: hex(0x1d1d1f),
            accent_soft: hex(0xd6e9ff),
            accent_soft_text: hex(0x0062cc),
            on_accent: hex(0xffffff),
            primary_hover: hex(0x3a9bff),
            primary_active: hex(0x0072e6),
            selected_row: blue,
            selected_row_text: hex(0xffffff),
            zebra: hex(0xf7f7f9),
            sidebar_active: hex(0xe3e3e8),
            sidebar_active_text: hex(0x1d1d1f),
            ok_soft: hex(0xdff5e6),
            ok_text: hex(0x248a3d),
            warning_soft: hex(0xfff0d6),
            warning_text: hex(0xb25f00),
            error_soft: hex(0xffe1de),
            error_text: hex(0xd70015),
            done_soft: hex(0xeeeef0),
            bar_track: hex(0xe5e5ea),
            bar_fill: blue,
            kbd_border: hex(0xd1d1d6),
            link_hover: hex(0x004a99),
            scrollbar_thumb: hex(0xc7c7cc),
            scrollbar_thumb_hover: hex(0xa1a1a6),
        }
    }

    /// macOS Native, dark: the same structure with macOS dark system grays.
    pub fn native_dark() -> Self {
        let background = hex(0x1e1e1e);
        let border = hex(0x3a3a3c);
        let ok = hex(0x30d158);
        let warning = hex(0xff9f0a);
        let error = hex(0xff453a);
        let muted = hex(0x98989d);
        let blue = hex(0x0a84ff);
        Self {
            background,
            panel: hex(0x252527),
            panel_raised: hex(0x2c2c2e),
            rail: hex(0x2a2a2c),
            border,
            text: hex(0xf5f5f7),
            text_muted: muted,
            accent: blue,
            teal: hex(0x40c8e0),
            ok,
            warning,
            error,
            done: muted,
            link: hex(0x64a8ff),
            sidebar: hex(0x2a2a2c),
            chrome: hex(0x2a2a2c),
            surface: background,
            surface_alt: hex(0x252527),
            control: hex(0x3a3a3c),
            control_hover: hex(0x48484a),
            control_active: hex(0x545456),
            control_border: hex(0x48484a),
            row_divider: border,
            text_strong: hex(0xffffff),
            accent_soft: hex(0x0a3a6b),
            accent_soft_text: hex(0x9fd0ff),
            on_accent: hex(0xffffff),
            primary_hover: hex(0x3a9bff),
            primary_active: hex(0x0072e6),
            selected_row: blue,
            selected_row_text: hex(0xffffff),
            zebra: hex(0x262628),
            sidebar_active: hex(0x3a3a3c),
            sidebar_active_text: hex(0xf5f5f7),
            ok_soft: hex(0x12361f),
            ok_text: hex(0x32d74b),
            warning_soft: hex(0x3d2c0a),
            warning_text: hex(0xffb340),
            error_soft: hex(0x3d1512),
            error_text: hex(0xff6961),
            done_soft: hex(0x3a3a3c),
            bar_track: hex(0x3a3a3c),
            bar_fill: blue,
            kbd_border: hex(0x545456),
            link_hover: hex(0x9fd0ff),
            scrollbar_thumb: hex(0x545456),
            scrollbar_thumb_hover: hex(0x6e6e73),
        }
    }

    /// Observatory direction (Observatory.dc.html): graphite background, amber
    /// accent, card surfaces.
    pub fn observatory() -> Self {
        let background = hex(0x16171a);
        let border = hex(0x26272c);
        let ok = hex(0x3fcb7e);
        let warning = hex(0xf5a524);
        let error = hex(0xf2555a);
        let muted = hex(0xa3a4ab);
        let amber = hex(0xf5a524);
        Self {
            background,
            panel: hex(0x1b1c20),
            panel_raised: hex(0x202126),
            rail: background,
            border,
            text: hex(0xececee),
            text_muted: muted,
            accent: amber,
            teal: hex(0x4cc9d8),
            ok,
            warning,
            error,
            done: muted,
            link: hex(0xf5b547),
            sidebar: background,
            chrome: background,
            surface: hex(0x1b1c20),
            surface_alt: hex(0x1f2025),
            control: hex(0x202126),
            control_hover: hex(0x2a2b31),
            control_active: hex(0x33343a),
            control_border: hex(0x2e2f35),
            row_divider: hex(0x24252a),
            text_strong: hex(0xffffff),
            accent_soft: hex(0x2b2414),
            accent_soft_text: hex(0xffd08a),
            on_accent: hex(0x16171a),
            primary_hover: hex(0xffb94d),
            primary_active: hex(0xdb931a),
            selected_row: hex(0x2b2414),
            selected_row_text: hex(0xffd08a),
            zebra: background,
            sidebar_active: amber,
            sidebar_active_text: hex(0x16171a),
            ok_soft: hex(0x14291d),
            ok_text: hex(0x7ee2a8),
            warning_soft: hex(0x2b2414),
            warning_text: hex(0xffd08a),
            error_soft: hex(0x2f171a),
            error_text: hex(0xff8a8e),
            done_soft: hex(0x2a2b31),
            bar_track: hex(0x2a2b31),
            bar_fill: amber,
            kbd_border: hex(0x3a3b42),
            link_hover: hex(0xffd08a),
            scrollbar_thumb: hex(0x3a3b42),
            scrollbar_thumb_hover: hex(0x4a4b53),
        }
    }
}

/// Installs the Freelens-like dark theme. Call once after `gpui_kit::init`.
pub fn apply(cx: &mut App) {
    crate::tokens::set_appearance(natikube_core::Appearance::Standard, cx);
}

/// Pushes `p` into GPUI Kit's global theme.
pub(crate) fn install_palette(p: &Palette, cx: &mut App) {
    let p = *p;
    Theme::update(cx, |t| {
        t.background = p.background;
        t.foreground = p.text;
        t.muted = p.panel;
        t.muted_foreground = p.text_muted;
        t.border = p.border;
        t.input = p.control_border;
        t.ring = p.accent;
        t.caret = p.text;
        t.selection = p.accent.opacity(0.35);
        t.accent = p.control;
        t.accent_foreground = p.text;
        t.primary = p.accent;
        t.primary_hover = p.primary_hover;
        t.primary_active = p.primary_active;
        t.primary_foreground = p.on_accent;
        t.secondary = p.control;
        t.secondary_hover = p.control_hover;
        t.secondary_active = p.control_active;
        t.secondary_foreground = p.text;
        t.popover = p.panel;
        t.popover_foreground = p.text;
        t.link = p.link;
        t.link_hover = p.link_hover;
        t.link_active = p.accent;
        t.colors.list = p.background;
        t.list_even = p.background;
        t.list_head = p.panel;
        t.list_hover = p.panel_raised;
        t.list_active = p.selected_row;
        t.list_active_border = p.accent;
        t.table = p.background;
        t.table_even = p.zebra;
        t.table_head = p.panel;
        t.table_head_foreground = p.text_muted;
        t.table_hover = p.panel_raised;
        t.table_active = p.selected_row;
        t.table_active_border = p.accent;
        t.table_row_border = p.border.opacity(0.6);
        t.sidebar = p.sidebar;
        t.sidebar_foreground = p.text;
        t.sidebar_border = p.border;
        t.sidebar_accent = p.sidebar_active;
        t.sidebar_accent_foreground = p.sidebar_active_text;
        t.sidebar_primary = p.accent;
        t.sidebar_primary_foreground = p.on_accent;
        t.tab = p.background;
        t.tab_active = p.background;
        t.tab_active_foreground = p.text;
        t.tab_foreground = p.text_muted;
        t.tab_bar = p.chrome;
        t.title_bar = p.chrome;
        t.title_bar_border = p.border;
        t.status_bar = p.chrome;
        t.status_bar_border = p.border;
        t.scrollbar = p.background.opacity(0.0);
        t.scrollbar_thumb = p.scrollbar_thumb;
        t.scrollbar_thumb_hover = p.scrollbar_thumb_hover;
        t.success = p.ok;
        t.success_foreground = hex(0xffffff);
        t.warning = p.warning;
        t.warning_foreground = hex(0x1e2124);
        t.danger = p.error;
        t.danger_foreground = hex(0xffffff);
        t.info = p.accent;
        t.green = p.ok;
        t.green_light = p.ok;
        t.red = p.error;
        t.red_light = p.error;
        t.yellow = p.warning;
        t.yellow_light = p.warning;
        t.blue = p.accent;
        t.blue_light = p.link;
        t.cyan = p.teal;
        t.cyan_light = p.teal;
    });
}

/// Semantic color for status-like values (pod phases, container states,
/// connection health).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tone {
    Ok,
    Done,
    Warning,
    Error,
    Muted,
}

impl Tone {
    pub fn color(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Self::Ok => theme.green,
            Self::Done => theme.muted_foreground,
            Self::Warning => theme.yellow,
            Self::Error => theme.red,
            Self::Muted => theme.muted_foreground.opacity(0.6),
        }
    }
}
