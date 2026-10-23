//! The macOS menu bar: App, File, View (Appearance, palette) and Window.
//! Rebuilt whenever the appearance changes so the checkmark follows it.

use gpui_kit::*;
use natikube_core::Appearance;

actions!(
    natikube,
    [
        Quit,
        SettingsOpen,
        PaletteOpen,
        FreelensImport,
        KubeconfigAdd,
        KubeconfigAddFolder,
        ClustersReload,
        AppearanceStandard,
        AppearanceCommand,
        AppearanceNative,
        AppearanceObservatory,
        WindowMinimize,
        WindowZoom,
    ]
);

fn appearance_item(appearance: Appearance, current: Appearance) -> MenuItem {
    let checked = appearance == current;
    let label = appearance.label();
    match appearance {
        Appearance::Standard => MenuItem::action(label, AppearanceStandard),
        Appearance::Command => MenuItem::action(label, AppearanceCommand),
        Appearance::Native => MenuItem::action(label, AppearanceNative),
        Appearance::Observatory => MenuItem::action(label, AppearanceObservatory),
    }
    .checked(checked)
}

/// The full menu bar with `current` checked under View → Appearance.
pub fn build_menus(current: Appearance) -> Vec<Menu> {
    vec![
        Menu::new("NatiKube").items([
            MenuItem::action("Settings…", SettingsOpen),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Quit NatiKube", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Import from Freelens…", FreelensImport),
            MenuItem::action("Add kubeconfig…", KubeconfigAdd),
            MenuItem::action("Add kubeconfig folder…", KubeconfigAddFolder),
            MenuItem::separator(),
            MenuItem::action("Reload clusters", ClustersReload),
        ]),
        Menu::new("View").items([
            MenuItem::action("Command Palette…", PaletteOpen),
            MenuItem::separator(),
            MenuItem::submenu(
                Menu::new("Appearance")
                    .items(Appearance::all().map(|a| appearance_item(a, current))),
            ),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", WindowMinimize),
            MenuItem::action("Zoom", WindowZoom),
        ]),
    ]
}

/// Installs the menu bar for `current`.
pub fn install_menus(current: Appearance, cx: &mut App) {
    cx.set_menus(build_menus(current));
}
