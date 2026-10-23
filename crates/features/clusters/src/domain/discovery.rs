//! Pure dedupe of contexts found across several kubeconfig files.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use natikube_core::ClusterInfo;

/// Source rank of `~/.kube/config`: always preferred.
pub const RANK_DEFAULT_CONFIG: usize = 0;
/// Source rank of the first `$KUBECONFIG` entry (later entries add their index).
pub const RANK_ENV_BASE: usize = 1;
/// Source rank of the other top-level files in `~/.kube`.
pub const RANK_KUBE_DIR: usize = 1_000;
/// Source rank of files that come from the settings extras.
pub const RANK_EXTRA: usize = 2_000;

/// One context read from one file, with what dedupe needs to rank it.
#[derive(Debug, Clone)]
pub struct Found {
    pub info: ClusterInfo,
    /// Name of the user the context points at (part of the identity).
    pub user: Option<String>,
    /// Lower is preferred (see the `RANK_*` constants).
    pub rank: usize,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Identity {
    /// Only part of the identity while the server is unknown; with a known
    /// server, contexts that point at the same server, user and namespace are
    /// aliases of one cluster (an EKS ARN context and its short name).
    context: Option<String>,
    server: Option<String>,
    user: Option<String>,
    namespace: Option<String>,
}

fn identity(found: &Found) -> Identity {
    Identity {
        context: found
            .info
            .server
            .is_none()
            .then(|| found.info.context.clone()),
        server: found.info.server.clone(),
        user: found.user.clone(),
        namespace: found.info.namespace.clone(),
    }
}

/// Folds identical contexts (same server, user and namespace) into one entry.
///
/// The winner is the one from the best source (lowest rank), then the
/// shortest context name, then the most recently modified file, then the first
/// in input order. The other
/// files go to `also_in` and the other context names to `aliases`. Output follows
/// (rank, input order). Distinct contexts that share a name keep their own
/// entry; every one after the first gets `name (source file)` so the user can
/// tell them apart.
pub fn dedupe(found: Vec<Found>) -> Vec<ClusterInfo> {
    let mut groups: Vec<(Identity, Vec<(usize, Found)>)> = Vec::new();
    let mut index: HashMap<Identity, usize> = HashMap::new();
    for (position, item) in found.into_iter().enumerate() {
        let key = identity(&item);
        match index.get(&key) {
            Some(&slot) => groups[slot].1.push((position, item)),
            None => {
                index.insert(key.clone(), groups.len());
                groups.push((key, vec![(position, item)]));
            }
        }
    }

    let mut winners: Vec<(usize, usize, ClusterInfo)> = Vec::new();
    for (_, mut members) in groups {
        members.sort_by_key(|(position, f)| {
            (f.rank, f.info.context.len(), Reverse(f.modified), *position)
        });
        let mut iter = members.into_iter();
        let Some((position, winner)) = iter.next() else {
            continue;
        };
        let mut info = winner.info;
        for (_, other) in iter {
            if other.info.context != info.context && !info.aliases.contains(&other.info.context) {
                info.aliases.push(other.info.context.clone());
            }
            if let Some(path) = other.info.kubeconfig
                && !info.also_in.contains(&path)
                && info.kubeconfig.as_ref() != Some(&path)
            {
                info.also_in.push(path);
            }
        }
        winners.push((winner.rank, position, info));
    }
    winners.sort_by_key(|(rank, position, _)| (*rank, *position));

    let mut seen_names: Vec<String> = Vec::new();
    winners
        .into_iter()
        .map(|(_, _, mut info)| {
            if seen_names.contains(&info.name)
                && let Some(file) = info.source_file_name()
            {
                info.name = format!("{} ({file})", info.name);
            }
            seen_names.push(info.name.clone());
            info
        })
        .collect()
}

/// Finds the cluster a user-supplied name refers to (`--context`, a
/// kubeconfig `current-context`, a saved last cluster). Tiers, in order: id,
/// context name, display name, then the aliases folded in by [`dedupe`]. The
/// first match of the best tier wins.
pub fn find_cluster<'a>(clusters: &'a [ClusterInfo], query: &str) -> Option<&'a ClusterInfo> {
    clusters
        .iter()
        .find(|c| c.id.as_str() == query)
        .or_else(|| clusters.iter().find(|c| c.context == query))
        .or_else(|| clusters.iter().find(|c| c.name == query))
        .or_else(|| {
            clusters
                .iter()
                .find(|c| c.aliases.iter().any(|a| a == query))
        })
}

/// Paths of `$KUBECONFIG` entries, in order, empty entries dropped.
pub fn env_entries(value: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(value)
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn found(ctx: &str, server: &str, user: &str, file: &str, rank: usize) -> Found {
        let mut info = ClusterInfo::new(ctx, Some(PathBuf::from(file)));
        info.server = Some(server.to_string());
        Found {
            info,
            user: Some(user.to_string()),
            rank,
            modified: None,
        }
    }

    #[test]
    fn lower_rank_wins_and_keeps_the_others_in_also_in() {
        let out = dedupe(vec![
            found("a", "s", "u", "/k/old", RANK_KUBE_DIR),
            found("a", "s", "u", "/k/config", RANK_DEFAULT_CONFIG),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kubeconfig, Some(PathBuf::from("/k/config")));
        assert_eq!(out[0].also_in, vec![PathBuf::from("/k/old")]);
    }

    #[test]
    fn newest_mtime_breaks_rank_ties() {
        let mut old = found("a", "s", "u", "/k/old", RANK_KUBE_DIR);
        old.modified = Some(SystemTime::UNIX_EPOCH);
        let mut new = found("a", "s", "u", "/k/new", RANK_KUBE_DIR);
        new.modified = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(10));
        let out = dedupe(vec![old, new]);
        assert_eq!(out[0].kubeconfig, Some(PathBuf::from("/k/new")));
    }

    #[test]
    fn different_server_or_user_stays_apart_with_disambiguated_name() {
        let out = dedupe(vec![
            found("a", "s1", "u", "/k/x", RANK_KUBE_DIR),
            found("a", "s2", "u", "/k/y", RANK_KUBE_DIR),
            found("a", "s1", "v", "/k/z", RANK_KUBE_DIR),
        ]);
        let names: Vec<_> = out.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a", "a (y)", "a (z)"]);
        assert!(out.iter().all(|c| c.context == "a"));
    }

    #[test]
    fn output_follows_rank_then_input_order() {
        let out = dedupe(vec![
            found("x", "sx", "u", "/k/extra", RANK_EXTRA),
            found("y", "sy", "u", "/k/config", RANK_DEFAULT_CONFIG),
            found("z", "sz", "u", "/k/dir", RANK_KUBE_DIR),
        ]);
        let names: Vec<_> = out.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["y", "z", "x"]);
    }

    #[test]
    fn aliases_of_one_cluster_fold_into_the_shortest_name() {
        let arn = "arn:aws:eks:eu-central-1:1:cluster/prod-cluster";
        let out = dedupe(vec![
            found(arn, "https://eks", "aws", "/k/bak1", RANK_KUBE_DIR),
            found(
                "prod-cluster",
                "https://eks",
                "aws",
                "/k/bak2",
                RANK_KUBE_DIR,
            ),
            found("other", "https://eks", "aws2", "/k/bak2", RANK_KUBE_DIR),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].context, "prod-cluster");
        assert_eq!(out[0].aliases, vec![arn.to_string()]);
        assert_eq!(out[0].also_in, vec![PathBuf::from("/k/bak1")]);
        assert_eq!(out[1].context, "other");
    }

    #[test]
    fn same_server_and_user_with_other_namespace_stays_apart() {
        let mut a = found("a", "s", "u", "/k/x", RANK_KUBE_DIR);
        a.info.namespace = Some("one".into());
        let mut b = found("b", "s", "u", "/k/x", RANK_KUBE_DIR);
        b.info.namespace = Some("two".into());
        assert_eq!(dedupe(vec![a, b]).len(), 2);
    }

    #[test]
    fn source_rank_beats_name_length() {
        let out = dedupe(vec![
            found("long-name", "s", "u", "/k/bak", RANK_KUBE_DIR),
            found("long-name-x", "s", "u", "/k/config", RANK_DEFAULT_CONFIG),
        ]);
        assert_eq!(out[0].context, "long-name-x");
        assert_eq!(out[0].aliases, vec!["long-name".to_string()]);
    }

    #[test]
    fn env_entries_split_and_drop_empties() {
        let value = std::env::join_paths(["/a", "", "/b"]).unwrap();
        assert_eq!(
            env_entries(&value),
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }
}
