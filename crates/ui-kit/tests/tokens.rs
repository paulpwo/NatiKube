use natikube_core::Appearance;
use natikube_ui_kit::Tokens;

#[test]
fn standard_has_no_variant_flags() {
    let t = Tokens::for_appearance(Appearance::Standard, false);
    assert!(t.dark);
    assert!(!t.stacked_names && !t.zebra && !t.usage_bars && !t.group_by_namespace);
}

#[test]
fn flags_match_each_design() {
    assert!(Tokens::for_appearance(Appearance::Command, true).stacked_names);
    assert!(Tokens::for_appearance(Appearance::Native, false).zebra);
    assert!(!Tokens::for_appearance(Appearance::Native, false).dark);
    assert!(Tokens::for_appearance(Appearance::Observatory, true).group_by_namespace);
}

use natikube_ui_kit::{Palette, bar_fraction, hex};

#[test]
fn standard_values_are_unchanged() {
    let t = Tokens::standard();
    assert_eq!(t.fonts.ui.as_ref(), ".SystemUIFont");
    assert_eq!(t.fonts.mono.as_ref(), "Menlo");
    assert_eq!(f32::from(t.font_size), 13.0);
    assert_eq!(f32::from(t.row_height), 28.0);
    assert_eq!(f32::from(t.header_height), 28.0);
    let p = t.palette;
    assert_eq!(p.background, hex(0x1e2124));
    assert_eq!(p.control_hover, hex(0x3a4148));
    assert_eq!(p.primary_hover, hex(0x58a3dd));
    assert_eq!(p.scrollbar_thumb, hex(0x4a5159));
    assert_eq!(p.sidebar, p.panel);
    assert_eq!(p.zebra, p.background);
}

#[test]
fn each_appearance_has_its_own_palette() {
    let standard = Tokens::standard().palette;
    let command = Tokens::for_appearance(Appearance::Command, true).palette;
    let observatory = Tokens::for_appearance(Appearance::Observatory, true).palette;
    assert_eq!(command.background, hex(0x0e0f12));
    assert_eq!(command.link, hex(0x9db0ff));
    assert_eq!(observatory.background, hex(0x16171a));
    assert_eq!(observatory.accent, hex(0xf5a524));
    assert_ne!(command.background, standard.background);
    assert_ne!(observatory.background, command.background);
}

#[test]
fn native_follows_system_appearance() {
    let light = Tokens::for_appearance(Appearance::Native, false);
    let dark = Tokens::for_appearance(Appearance::Native, true);
    assert!(!light.dark && dark.dark);
    assert_eq!(light.palette.background, hex(0xffffff));
    assert_eq!(light.palette.selected_row, hex(0x0a84ff));
    assert_ne!(dark.palette.background, light.palette.background);
    assert_eq!(dark.palette.accent, light.palette.accent);
}

#[test]
fn dark_only_designs_ignore_system_appearance() {
    for a in [Appearance::Command, Appearance::Observatory] {
        let light = Tokens::for_appearance(a, false);
        assert!(light.dark, "{a:?} is dark-only");
    }
    assert!(Tokens::for_appearance(Appearance::Standard, false).dark);
}

#[test]
fn for_appearance_reports_its_appearance() {
    for a in Appearance::all() {
        assert_eq!(Tokens::for_appearance(a, true).appearance, a);
    }
}

#[test]
fn palette_constructors_are_distinct() {
    let all = [
        Palette::freelens_dark(),
        Palette::command(),
        Palette::native_light(),
        Palette::native_dark(),
        Palette::observatory(),
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a.background, b.background);
        }
    }
}

#[test]
fn usage_bar_fraction_is_clamped() {
    assert_eq!(bar_fraction(f32::NAN), 0.0);
    assert_eq!(bar_fraction(-1.0), 0.0);
    assert_eq!(bar_fraction(0.0), 0.0);
    assert_eq!(bar_fraction(0.001), 0.02);
    assert_eq!(bar_fraction(0.5), 0.5);
    assert_eq!(bar_fraction(7.0), 1.0);
}

use natikube_core::{Density, UiPrefs};
use natikube_ui_kit::{BASE_FONT_SIZE, ui_scale};

#[test]
fn default_prefs_reproduce_the_designed_tokens() {
    for appearance in [
        Appearance::Standard,
        Appearance::Command,
        Appearance::Native,
        Appearance::Observatory,
    ] {
        let plain = Tokens::for_appearance(appearance, true);
        let with = Tokens::for_appearance_with(appearance, true, &UiPrefs::default());
        assert_eq!(plain.row_height, with.row_height);
        assert_eq!(plain.header_height, with.header_height);
        assert_eq!(plain.font_size, with.font_size);
        assert_eq!(f32::from(with.font_size), BASE_FONT_SIZE);
        assert_eq!(with.scale, 1.0);
    }
}

#[test]
fn font_size_and_zoom_scale_text_and_rows() {
    let ui = UiPrefs {
        font_size: 16,
        zoom_percent: 125,
        ..UiPrefs::default()
    };
    let t = Tokens::for_appearance_with(Appearance::Standard, true, &ui);
    let scale = 16.0 / 13.0 * 1.25;
    assert!((ui_scale(&ui) - scale).abs() < 1e-6);
    assert!((f32::from(t.font_size) - 13.0 * scale).abs() < 1e-4);
    assert_eq!(f32::from(t.row_height), (28.0 * scale).round());
    // Monospace size follows zoom only.
    assert!((f32::from(t.mono_size) - 12.0 * 1.25).abs() < 1e-4);
}

#[test]
fn compact_density_shrinks_rows_in_every_appearance() {
    let compact = UiPrefs {
        density: Density::Compact,
        ..UiPrefs::default()
    };
    for appearance in [
        Appearance::Standard,
        Appearance::Command,
        Appearance::Native,
        Appearance::Observatory,
    ] {
        let roomy = Tokens::for_appearance(appearance, true);
        let tight = Tokens::for_appearance_with(appearance, true, &compact);
        assert!(tight.row_height < roomy.row_height, "{appearance:?}");
        assert!(tight.header_height < roomy.header_height, "{appearance:?}");
        assert_eq!(tight.density, Density::Compact);
    }
}

#[test]
fn custom_monospace_family_replaces_menlo() {
    let ui = UiPrefs {
        mono_font: Some("JetBrains Mono".into()),
        ..UiPrefs::default()
    };
    let t = Tokens::for_appearance_with(Appearance::Command, true, &ui);
    assert_eq!(t.fonts.mono.as_ref(), "JetBrains Mono");
    assert_eq!(
        Tokens::for_appearance(Appearance::Command, true)
            .fonts
            .mono
            .as_ref(),
        "Menlo"
    );
}
