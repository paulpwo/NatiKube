use natikube_core::{AppCommand, ClusterId, PortForwardSpec, ResourceRef, ViewId, kinds};

fn pod() -> ResourceRef {
    ResourceRef::new(
        ClusterId::new("c"),
        kinds::POD,
        Some("default".into()),
        "api-1",
    )
}

#[test]
fn commands_are_cloneable_and_comparable() {
    let commands = vec![
        AppCommand::SelectCluster(ClusterId::new("c")),
        AppCommand::Navigate(ViewId::Resource(kinds::POD)),
        AppCommand::OpenDetail(pod()),
        AppCommand::OpenLogs {
            pod: pod(),
            container: Some("app".into()),
        },
        AppCommand::OpenExec {
            pod: pod(),
            container: None,
        },
        AppCommand::OpenLocalShell,
        AppCommand::StartPortForward(PortForwardSpec {
            target: pod(),
            remote_port: 8080,
            local_port: None,
        }),
        AppCommand::ShowError("boom".into()),
        AppCommand::CloseDetail,
        AppCommand::SetCustomResources(vec![kinds::POD]),
    ];
    assert_eq!(commands.clone(), commands);
}

#[test]
fn port_forward_spec_describes_itself() {
    let spec = PortForwardSpec {
        target: pod(),
        remote_port: 8080,
        local_port: Some(9000),
    };
    assert_eq!(spec.describe(), "Pod default/api-1 :9000 -> 8080");
    let auto = PortForwardSpec {
        target: pod(),
        remote_port: 80,
        local_port: None,
    };
    assert_eq!(auto.describe(), "Pod default/api-1 :auto -> 80");
}

#[test]
fn appearance_and_discovery_commands_exist() {
    use natikube_core::Appearance;
    let all = [
        AppCommand::SetAppearance(Appearance::Command),
        AppCommand::OpenPalette,
        AppCommand::ClosePalette,
        AppCommand::ImportFreelens,
        AppCommand::AddKubeconfig,
        AppCommand::ReloadClusters,
    ];
    for c in &all {
        assert_eq!(c, &c.clone());
    }
    assert_ne!(
        AppCommand::SetAppearance(Appearance::Native),
        AppCommand::SetAppearance(Appearance::Command)
    );
}

#[test]
fn settings_commands_exist_and_compare() {
    use natikube_core::{AppCommand, UserPrefs};
    use std::path::PathBuf;
    let a = AppCommand::SettingsChanged(Box::<UserPrefs>::default());
    assert_eq!(a, AppCommand::SettingsChanged(Box::<UserPrefs>::default()));
    assert_ne!(a, AppCommand::OpenSettings);
    assert_eq!(
        AppCommand::RemoveKubeconfig(PathBuf::from("/x")),
        AppCommand::RemoveKubeconfig(PathBuf::from("/x"))
    );
}
