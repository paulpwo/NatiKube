use gpui_kit::{Menu, MenuItem};
use natikube_core::Appearance;
use natikube_shell::menus::build_menus;

fn submenu<'a>(menu: &'a Menu, name: &str) -> &'a Menu {
    menu.items
        .iter()
        .find_map(|item| match item {
            MenuItem::Submenu(m) if m.name == name => Some(m),
            _ => None,
        })
        .unwrap_or_else(|| panic!("submenu {name} missing"))
}

fn action_names(menu: &Menu) -> Vec<String> {
    menu.items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Action { name, .. } => Some(name.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn menu_bar_has_app_file_view_window() {
    let menus = build_menus(Appearance::Standard);
    let names: Vec<_> = menus.iter().map(|m| m.name.to_string()).collect();
    assert_eq!(names, ["NatiKube", "File", "View", "Window"]);
}

#[test]
fn file_menu_imports_and_adds_kubeconfigs() {
    let menus = build_menus(Appearance::Standard);
    let names = action_names(&menus[1]);
    assert!(names.contains(&"Import from Freelens…".to_string()));
    assert!(names.contains(&"Add kubeconfig…".to_string()));
}

#[test]
fn appearance_submenu_checks_only_the_current_one() {
    for current in Appearance::all() {
        let menus = build_menus(current);
        let appearance = submenu(&menus[2], "Appearance");
        let rows: Vec<(String, bool)> = appearance
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, checked, .. } => Some((name.to_string(), *checked)),
                _ => None,
            })
            .collect();
        let labels: Vec<_> = rows.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            labels,
            ["Standard", "Command", "macOS Native", "Observatory"]
        );
        let checked: Vec<_> = rows
            .iter()
            .filter(|(_, c)| *c)
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(checked, [current.label()]);
    }
}

#[test]
fn view_menu_offers_the_palette() {
    let menus = build_menus(Appearance::Native);
    assert!(action_names(&menus[2]).contains(&"Command Palette…".to_string()));
    assert!(action_names(&menus[2]).contains(&"Notifications".to_string()));
}
