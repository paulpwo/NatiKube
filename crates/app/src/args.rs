pub const USAGE: &str = "\
Usage: natikube [OPTIONS]

Options:
  --context <NAME>          kubeconfig context (default: current-context)
  --fake-rows <N>           show N synthetic pods instead of a cluster
  --exit-after-first-frame  print first_frame_ms / first_rows_frame_ms on stderr and exit
  --dump-rows               print the loaded pod count and names on stderr
  --dump <KIND>             print the objects of KIND (e.g. pods) as namespace/name
                            lines on stdout and exit, without opening a window
  --appearance <NAME>       standard | command | native | observatory; overrides the
                            saved appearance for this run only (not persisted)
  --native-mode <MODE>      light | dark; forces the Native appearance variant for
                            this run only (not persisted; default follows the system setting)
  --open-settings           open the Settings window at startup
  --demo                    show a fictional demo cluster (no kubeconfig, no network,
                            settings kept in memory); used for screenshots
  --fake-alerts <N>         seed N sample notifications into the history and never watch a
                            cluster; needs NATIKUBE_CONFIG_DIR so the real history stays
                            untouched (use with --demo)
  --open-alerts             open the notifications panel at startup
  --settings-section <NAME> with --open-settings: show this page (appearance, clusters,
                            terminal, logs, data, alerts)
  --dump-clusters           print every discovered kubeconfig context (name, source
                            file name, dedupe status; never credentials) and exit
  -h, --help                show this help";

use natikube_core::Appearance;
use natikube_settings::SettingsSection;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Args {
    pub context: Option<String>,
    pub fake_rows: Option<usize>,
    pub exit_after_first_frame: bool,
    pub dump_rows: bool,
    pub dump: Option<String>,
    pub appearance: Option<Appearance>,
    pub dump_clusters: bool,
    /// `--native-mode`: `Some(true)` = dark, `Some(false)` = light.
    pub native_dark: Option<bool>,
    pub open_settings: bool,
    /// `--demo`: fictional cluster catalog, no kubeconfig, no network.
    pub demo: bool,
    /// `--fake-alerts N`: seed N sample alerts, no monitoring.
    pub fake_alerts: Option<usize>,
    /// `--open-alerts`: open the bell panel at startup.
    pub open_alerts: bool,
    /// `--settings-section`: page shown by `--open-settings`.
    pub settings_section: Option<SettingsSection>,
    /// `--fake-update <version>` (hidden): show the update modal for this
    /// version without contacting GitHub.
    pub fake_update: Option<String>,
}

impl Args {
    /// Parses command-line flags. `Ok(None)` means help was requested.
    pub fn parse(mut raw: impl Iterator<Item = String>) -> Result<Option<Self>, String> {
        let mut args = Self::default();
        while let Some(flag) = raw.next() {
            match flag.as_str() {
                "--context" => {
                    args.context = Some(raw.next().ok_or("--context needs a value")?);
                }
                "--fake-rows" => {
                    let value = raw.next().ok_or("--fake-rows needs a value")?;
                    let count = value
                        .parse()
                        .map_err(|_| format!("--fake-rows expects a number, got {value:?}"))?;
                    args.fake_rows = Some(count);
                }
                "--exit-after-first-frame" => args.exit_after_first_frame = true,
                "--dump-rows" => args.dump_rows = true,
                "--dump" => {
                    args.dump = Some(raw.next().ok_or("--dump needs a kind, e.g. pods")?);
                }
                "--appearance" => {
                    let value = raw.next().ok_or("--appearance needs a value")?;
                    args.appearance = Some(Appearance::from_slug(&value).ok_or_else(|| {
                        format!(
                            "--appearance expects standard|command|native|observatory, got {value:?}"
                        )
                    })?);
                }
                "--native-mode" => {
                    let value = raw.next().ok_or("--native-mode needs a value")?;
                    args.native_dark = match value.to_lowercase().as_str() {
                        "light" => Some(false),
                        "dark" => Some(true),
                        _ => {
                            return Err(format!("--native-mode expects light|dark, got {value:?}"));
                        }
                    };
                }
                "--fake-update" => {
                    let value = raw.next().ok_or("--fake-update needs a version")?;
                    if natikube_updates::parse_tag(&value).is_none() {
                        return Err(format!("--fake-update expects a version, got {value:?}"));
                    }
                    args.fake_update = Some(value);
                }
                "--open-settings" => args.open_settings = true,
                "--demo" => args.demo = true,
                "--open-alerts" => args.open_alerts = true,
                "--settings-section" => {
                    let value = raw.next().ok_or("--settings-section needs a value")?;
                    args.settings_section = Some(
                        SettingsSection::all()
                            .into_iter()
                            .find(|s| s.label().eq_ignore_ascii_case(&value))
                            .ok_or_else(|| format!("unknown settings section {value:?}"))?,
                    );
                }
                "--fake-alerts" => {
                    let value = raw.next().ok_or("--fake-alerts needs a value")?;
                    let count = value
                        .parse()
                        .map_err(|_| format!("--fake-alerts expects a number, got {value:?}"))?;
                    args.fake_alerts = Some(count);
                }
                "--dump-clusters" => args.dump_clusters = true,
                "-h" | "--help" => return Ok(None),
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        Ok(Some(args))
    }
}

#[cfg(test)]
mod tests {
    use super::Args;

    fn parse(list: &[&str]) -> Result<Option<Args>, String> {
        Args::parse(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_all_flags() {
        let args = parse(&[
            "--context",
            "dev-cluster",
            "--fake-rows",
            "10000",
            "--exit-after-first-frame",
            "--dump-rows",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(args.context.as_deref(), Some("dev-cluster"));
        assert_eq!(args.fake_rows, Some(10_000));
        assert!(args.exit_after_first_frame);
        assert!(args.dump_rows);
        assert_eq!(args.dump, None);
    }

    #[test]
    fn parses_dump_kind() {
        let args = parse(&["--context", "dev-cluster", "--dump", "pods"])
            .unwrap()
            .unwrap();
        assert_eq!(args.dump.as_deref(), Some("pods"));
        assert_eq!(args.context.as_deref(), Some("dev-cluster"));
        assert!(parse(&["--dump"]).is_err());
    }

    #[test]
    fn parses_appearance_and_dump_clusters() {
        use natikube_core::Appearance;
        let args = parse(&["--appearance", "Observatory", "--dump-clusters"])
            .unwrap()
            .unwrap();
        assert_eq!(args.appearance, Some(Appearance::Observatory));
        assert!(args.dump_clusters);
        assert!(parse(&["--appearance", "neon"]).is_err());
        assert!(parse(&["--appearance"]).is_err());
    }

    #[test]
    fn parses_native_mode() {
        assert_eq!(
            parse(&["--native-mode", "dark"])
                .unwrap()
                .unwrap()
                .native_dark,
            Some(true)
        );
        assert_eq!(
            parse(&["--native-mode", "Light"])
                .unwrap()
                .unwrap()
                .native_dark,
            Some(false)
        );
        assert_eq!(parse(&[]).unwrap().unwrap().native_dark, None);
        assert!(parse(&["--native-mode", "blue"]).is_err());
        assert!(parse(&["--native-mode"]).is_err());
    }

    #[test]
    fn parses_demo() {
        assert!(parse(&["--demo"]).unwrap().unwrap().demo);
        assert!(!parse(&[]).unwrap().unwrap().demo);
    }

    #[test]
    fn parses_open_alerts_and_settings_section() {
        let args = parse(&["--open-alerts", "--settings-section", "Alerts"])
            .unwrap()
            .unwrap();
        assert!(args.open_alerts);
        assert_eq!(
            args.settings_section,
            Some(natikube_settings::SettingsSection::Alerts)
        );
        assert!(parse(&["--settings-section", "nope"]).is_err());
    }

    #[test]
    fn parses_fake_alerts() {
        assert_eq!(
            parse(&["--fake-alerts", "12"])
                .unwrap()
                .unwrap()
                .fake_alerts,
            Some(12)
        );
        assert_eq!(parse(&[]).unwrap().unwrap().fake_alerts, None);
        assert!(parse(&["--fake-alerts"]).is_err());
        assert!(parse(&["--fake-alerts", "lots"]).is_err());
    }

    #[test]
    fn parses_the_hidden_fake_update_flag() {
        let args = parse(&["--demo", "--fake-update", "9.9.9"])
            .unwrap()
            .unwrap();
        assert_eq!(args.fake_update.as_deref(), Some("9.9.9"));
        assert!(args.demo);
        assert_eq!(parse(&[]).unwrap().unwrap().fake_update, None);
        assert!(parse(&["--fake-update"]).is_err());
        assert!(parse(&["--fake-update", "soon"]).is_err());
    }

    #[test]
    fn defaults_to_current_context() {
        assert_eq!(parse(&[]).unwrap().unwrap(), Args::default());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&["--fake-rows", "many"]).is_err());
        assert!(parse(&["--context"]).is_err());
        assert!(parse(&["--watch-everything"]).is_err());
        assert_eq!(parse(&["--help"]).unwrap(), None);
    }
}
