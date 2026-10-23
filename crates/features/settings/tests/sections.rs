use natikube_settings::SettingsSection;

#[test]
fn sections_are_in_sidebar_order_with_labels() {
    let labels: Vec<_> = SettingsSection::all().map(|s| s.label()).to_vec();
    assert_eq!(
        labels,
        ["Appearance", "Clusters", "Terminal", "Logs", "Data"]
    );
    assert_eq!(SettingsSection::default(), SettingsSection::Appearance);
}
