//! Appearance-dependent design tokens: the single contract every crate uses to
//! render differently per [`Appearance`].
//!
//! Read tokens with `cx.tokens()` ([`ActiveTokens`]); never hard-code colors,
//! radii, fonts or row heights in features. Change appearance with
//! [`set_appearance`]. Standard tokens equal the values shipped before
//! appearances existed and must never change.

use std::sync::OnceLock;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use natikube_core::{Appearance, Density, UiPrefs};

use crate::theme::{Palette, install_palette};

/// Family of the embedded Command font (Hanken Grotesk, SIL OFL 1.1).
pub const COMMAND_FONT: &str = "Hanken Grotesk";

/// The embedded Hanken Grotesk variable font (133 KB, 0.65 % of the binary).
static HANKEN_GROTESK: &[u8] = include_bytes!("../assets/HankenGrotesk-Variable.ttf");

struct FontsLoaded;

impl Global for FontsLoaded {}

/// Registers the embedded fonts with GPUI. Command uses Hanken Grotesk only
/// when this succeeded; otherwise it keeps the system UI font. Call once
/// after `gpui_kit::init`, before the first [`set_appearance`].
pub fn register_fonts(cx: &mut App) {
    if cx
        .text_system()
        .add_fonts(vec![std::borrow::Cow::Borrowed(HANKEN_GROTESK)])
        .is_ok()
    {
        cx.set_global(FontsLoaded);
    }
}

/// The font size that is scale 1.0.
pub const BASE_FONT_SIZE: f32 = 13.0;
/// Compact density shrinks table rows and headers by these factors.
pub const COMPACT_ROW: f32 = 0.86;
pub const COMPACT_HEADER: f32 = 0.9;

/// UI scale from the user's font size and zoom (1.0 = 13 px at 100 %).
pub fn ui_scale(ui: &UiPrefs) -> f32 {
    f32::from(ui.font_size) / BASE_FONT_SIZE * f32::from(ui.zoom_percent) / 100.0
}

/// Corner radii.
#[derive(Debug, Clone, Copy)]
pub struct Radii {
    pub sm: Pixels,
    pub md: Pixels,
    pub lg: Pixels,
}

/// Font families. GPUI resolves these against system (or registered) fonts, so
/// always provide a family that exists on macOS.
#[derive(Debug, Clone)]
pub struct Fonts {
    pub ui: SharedString,
    pub mono: SharedString,
}

/// Everything that varies per appearance.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub appearance: Appearance,
    /// True when the tokens are the dark variant (Native follows macOS).
    pub dark: bool,
    pub palette: Palette,
    pub radii: Radii,
    pub fonts: Fonts,
    /// Base UI font size (user font size and zoom applied).
    pub font_size: Pixels,
    /// Monospace text size (user setting, zoom applied).
    pub mono_size: Pixels,
    /// Combined UI scale: font size relative to 13 px times zoom. 1.0 by
    /// default. Rows, headers and the GPUI rem size follow it.
    pub scale: f32,
    pub density: Density,
    /// Height of one table row.
    pub row_height: Pixels,
    /// Height of the table header row.
    pub header_height: Pixels,
    /// Name cell shows the namespace stacked under the name (Command).
    pub stacked_names: bool,
    /// Alternate row backgrounds (Native).
    pub zebra: bool,
    /// CPU/Memory cells show a value plus a mini usage bar (Command).
    pub usage_bars: bool,
    /// Rows are grouped in sections per namespace with totals (Observatory).
    pub group_by_namespace: bool,
    /// Status is a filled pill (Command) instead of colored text/dot.
    pub status_pills: bool,
    /// Right detail panel is an inspector that follows the selection (Native).
    pub inspector_detail: bool,
}

impl Tokens {
    /// Tokens for `appearance`. `dark` is the macOS system appearance; only
    /// Native uses it (the others are dark-only designs).
    ///
    /// Fonts: the designs name Hanken Grotesk / IBM Plex Sans / SF Pro and
    /// JetBrains Mono / SF Mono. No TTFs are embedded; every appearance uses
    /// the macOS system UI font (SF Pro) and Menlo, which always exist, so a
    /// missing font can never break rendering. Proportions, weights, radii and
    /// colors come from the canvases.
    pub fn for_appearance(appearance: Appearance, dark: bool) -> Self {
        Self::for_appearance_with(appearance, dark, &UiPrefs::default())
    }

    /// Tokens for `appearance` with the user's font size, zoom, density and
    /// monospace font applied. Default prefs give exactly the designed values.
    pub fn for_appearance_with(appearance: Appearance, dark: bool, ui: &UiPrefs) -> Self {
        Self::designed(appearance, dark).with_prefs(ui)
    }

    /// Applies the user's [`UiPrefs`] on top of the designed values.
    pub fn with_prefs(mut self, ui: &UiPrefs) -> Self {
        let scale = ui_scale(ui);
        self.scale = scale;
        self.density = ui.density;
        self.font_size = px(BASE_FONT_SIZE * scale);
        self.mono_size = px(f32::from(ui.mono_font_size) * f32::from(ui.zoom_percent) / 100.0);
        if let Some(family) = &ui.mono_font {
            self.fonts.mono = family.clone().into();
        }
        let (rows, header) = match ui.density {
            Density::Comfortable => (1.0, 1.0),
            Density::Compact => (COMPACT_ROW, COMPACT_HEADER),
        };
        self.row_height = px((f32::from(self.row_height) * scale * rows).round());
        self.header_height = px((f32::from(self.header_height) * scale * header).round());
        self
    }

    fn designed(appearance: Appearance, dark: bool) -> Self {
        let base = Self::standard();
        match appearance {
            Appearance::Standard => base,
            Appearance::Command => Self {
                appearance,
                palette: Palette::command(),
                radii: Radii {
                    sm: px(4.),
                    md: px(8.),
                    lg: px(10.),
                },
                fonts: Fonts {
                    ui: COMMAND_FONT.into(),
                    ..base.fonts.clone()
                },
                row_height: px(48.),
                header_height: px(36.),
                stacked_names: true,
                usage_bars: true,
                status_pills: true,
                ..base
            },
            Appearance::Native => Self {
                appearance,
                dark,
                palette: if dark {
                    Palette::native_dark()
                } else {
                    Palette::native_light()
                },
                radii: Radii {
                    sm: px(4.),
                    md: px(6.),
                    lg: px(8.),
                },
                row_height: px(26.),
                header_height: px(28.),
                zebra: true,
                inspector_detail: true,
                ..base
            },
            Appearance::Observatory => Self {
                appearance,
                palette: Palette::observatory(),
                radii: Radii {
                    sm: px(4.),
                    md: px(8.),
                    lg: px(12.),
                },
                row_height: px(34.),
                header_height: px(40.),
                group_by_namespace: true,
                ..base
            },
        }
    }

    /// Today's UI. Do not edit values.
    pub fn standard() -> Self {
        Self {
            appearance: Appearance::Standard,
            dark: true,
            palette: Palette::freelens_dark(),
            radii: Radii {
                sm: px(3.),
                md: px(4.),
                lg: px(6.),
            },
            fonts: Fonts {
                ui: ".SystemUIFont".into(),
                mono: "Menlo".into(),
            },
            font_size: px(BASE_FONT_SIZE),
            mono_size: px(12.),
            scale: 1.0,
            density: Density::Comfortable,
            row_height: px(28.),
            header_height: px(28.),
            stacked_names: false,
            zebra: false,
            usage_bars: false,
            group_by_namespace: false,
            status_pills: false,
            inspector_detail: false,
        }
    }
}

/// The global holding the active tokens.
pub struct NatiTheme(pub Tokens);

impl Global for NatiTheme {}

/// The user's UI preferences, applied to every token set.
struct UiSettings(UiPrefs);

impl Global for UiSettings {}

fn current_ui(cx: &App) -> UiPrefs {
    cx.try_global::<UiSettings>()
        .map(|g| g.0.clone())
        .unwrap_or_default()
}

/// Stores `ui` and re-applies the active appearance so every window restyles
/// live (fonts, zoom, density). No-op when nothing changed.
pub fn set_ui_prefs(ui: UiPrefs, cx: &mut App) {
    if cx.try_global::<UiSettings>().map(|g| &g.0) == Some(&ui) {
        return;
    }
    cx.set_global(UiSettings(ui));
    let appearance = cx.tokens().appearance;
    set_appearance(appearance, cx);
}

fn fallback() -> &'static Tokens {
    static STANDARD: OnceLock<Tokens> = OnceLock::new();
    STANDARD.get_or_init(Tokens::standard)
}

/// `cx.tokens()` on any `App`/`Context`.
pub trait ActiveTokens {
    fn tokens(&self) -> &Tokens;
}

impl ActiveTokens for App {
    fn tokens(&self) -> &Tokens {
        self.try_global::<NatiTheme>()
            .map(|t| &t.0)
            .unwrap_or_else(|| fallback())
    }
}

/// Forces Native's light or dark variant for this run (screenshots, tests)
/// instead of following macOS. Never persisted.
struct ForcedDark(bool);

impl Global for ForcedDark {}

/// Makes Native use the light (`false`) or dark (`true`) variant regardless of
/// the macOS setting. Call before [`set_appearance`].
pub fn force_native_dark(dark: bool, cx: &mut App) {
    cx.set_global(ForcedDark(dark));
}

fn system_is_dark(cx: &App) -> bool {
    if let Some(forced) = cx.try_global::<ForcedDark>() {
        return forced.0;
    }
    matches!(
        cx.window_appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// Subscribes `window` to macOS light/dark changes and re-applies the Native
/// tokens when they happen. Keep the returned subscription alive for the
/// window's lifetime (e.g. `.detach()` it).
pub fn watch_system_appearance(window: &Window) -> Subscription {
    window.observe_window_appearance(|_window, cx| sync_system_appearance(cx))
}

/// Switches the whole app to `appearance` live: replaces the token global,
/// reinstalls the GPUI Kit theme and refreshes every window. Callers inside a
/// window can pass `cx` directly (`Context<T>` derefs to `App`).
pub fn set_appearance(appearance: Appearance, cx: &mut App) {
    let ui = current_ui(cx);
    let mut tokens = Tokens::for_appearance_with(appearance, system_is_dark(cx), &ui);
    if tokens.fonts.ui.as_ref() == COMMAND_FONT && cx.try_global::<FontsLoaded>().is_none() {
        tokens.fonts.ui = Tokens::standard().fonts.ui;
    }
    let mode = if tokens.dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, None, cx);
    install_palette(&tokens.palette, cx);
    install_typography(&tokens, ui.mono_font.is_some(), cx);
    cx.set_global(NatiTheme(tokens));
    cx.refresh_windows();
}

/// GPUI Kit sizes everything in rems: its font size is the rem, so scaling it
/// scales spacing and text together.
fn install_typography(tokens: &Tokens, custom_mono: bool, cx: &mut App) {
    let rem = px(GPUI_BASE_REM * tokens.scale);
    let mono = tokens.fonts.mono.clone();
    let mono_size = tokens.mono_size;
    Theme::update(cx, |t| {
        t.font_size = rem;
        // GPUI Kit's own default is 13 px against our 12 px default.
        t.mono_font_size = mono_size * (13.0 / 12.0);
        if custom_mono {
            t.mono_font_family = mono;
        }
    });
}

/// GPUI Kit's default rem (its theme font size).
const GPUI_BASE_REM: f32 = 16.0;

/// Re-evaluates tokens after the macOS light/dark setting changed. No-op
/// unless the active appearance is Native.
pub fn sync_system_appearance(cx: &mut App) {
    if cx.tokens().appearance == Appearance::Native {
        set_appearance(Appearance::Native, cx);
    }
}
