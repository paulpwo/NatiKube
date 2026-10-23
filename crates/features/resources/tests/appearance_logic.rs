use natikube_resources::{
    Capacity, ClusterKpis, ColumnKind, ColumnSpec, GroupItem, ObjectRow, ObjectTable, RowEvent,
    StatusFilter, StatusTone, Usage, UsageMap, bar_fill, cluster_kpis, flatten_groups,
    group_by_namespace, usage_key,
};

fn columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::new("name", "Name", ColumnKind::Name, 200.0),
        ColumnSpec::new("cpu", "CPU", ColumnKind::Cpu, 80.0),
    ]
}

fn pod(ns: &str, name: &str, health: StatusTone) -> ObjectRow {
    let mut row = ObjectRow::new(Some(ns.to_string()), name);
    row.health = health;
    row
}

fn usage(entries: &[(&str, &str, f64, u64)]) -> UsageMap {
    let mut map = UsageMap::default();
    for (ns, name, cpu, mem) in entries {
        map.insert(
            usage_key(Some(ns), name),
            Usage {
                cpu_cores: *cpu,
                memory_bytes: *mem,
            },
        );
    }
    map
}

fn sample() -> ObjectTable {
    let mut table = ObjectTable::new(columns());
    table.apply(RowEvent::Restarted(vec![
        pod("dev", "api", StatusTone::Ok),
        pod("dev", "worker", StatusTone::Error),
        pod("kube-system", "coredns", StatusTone::Ok),
        pod("kube-system", "job-x", StatusTone::Done),
        pod("monitoring", "vm", StatusTone::Warning),
    ]));
    table
}

fn names(table: &ObjectTable) -> Vec<String> {
    (0..table.len())
        .map(|i| table.row(i).unwrap().name.clone())
        .collect()
}

#[test]
fn status_filter_matches_by_health() {
    assert!(StatusFilter::All.matches(StatusTone::Done));
    assert!(StatusFilter::Running.matches(StatusTone::Ok));
    assert!(!StatusFilter::Running.matches(StatusTone::Done));
    assert!(!StatusFilter::Running.matches(StatusTone::Error));
    assert!(StatusFilter::Problems.matches(StatusTone::Warning));
    assert!(StatusFilter::Problems.matches(StatusTone::Error));
    assert!(!StatusFilter::Problems.matches(StatusTone::Ok));
    assert_eq!(
        StatusFilter::all(),
        [
            StatusFilter::All,
            StatusFilter::Running,
            StatusFilter::Problems
        ]
    );
    assert_eq!(StatusFilter::default(), StatusFilter::All);
}

#[test]
fn table_status_filter_really_filters_and_counts_ignore_it() {
    let mut table = sample();
    assert_eq!(table.len(), 5);

    table.set_status_filter(StatusFilter::Running);
    assert_eq!(names(&table), vec!["api", "coredns"]);
    table.set_status_filter(StatusFilter::Problems);
    assert_eq!(names(&table), vec!["worker", "vm"]);

    let counts = table.status_filter_counts();
    assert_eq!((counts.all, counts.running, counts.problems), (5, 2, 2));
    assert_eq!(table.status_filter(), StatusFilter::Problems);

    table.set_status_filter(StatusFilter::All);
    assert_eq!(table.len(), 5);
}

#[test]
fn status_filter_combines_with_search() {
    let mut table = sample();
    table.set_status_filter(StatusFilter::Running);
    table.set_search("core");
    assert_eq!(names(&table), vec!["coredns"]);
}

#[test]
fn groups_follow_visible_rows_with_totals() {
    let mut table = sample();
    table.set_usage(usage(&[
        ("dev", "api", 0.5, 100),
        ("dev", "worker", 0.25, 50),
        ("kube-system", "coredns", 0.1, 10),
    ]));
    let groups = group_by_namespace(&table);
    let summary: Vec<(&str, usize)> = groups
        .iter()
        .map(|g| (g.namespace.as_str(), g.rows.len()))
        .collect();
    assert_eq!(
        summary,
        vec![("dev", 2), ("kube-system", 2), ("monitoring", 1)]
    );
    assert!((groups[0].cpu_cores - 0.75).abs() < 1e-9);
    assert_eq!(groups[0].memory_bytes, 150);
    assert!(groups[0].has_usage);
    // `monitoring` has no metrics at all.
    assert!(!groups[2].has_usage);
    assert_eq!(groups[2].cpu_cores, 0.0);

    // Groups reflect the status filter: only problems remain.
    table.set_status_filter(StatusFilter::Problems);
    let groups = group_by_namespace(&table);
    let summary: Vec<(&str, usize)> = groups
        .iter()
        .map(|g| (g.namespace.as_str(), g.rows.len()))
        .collect();
    assert_eq!(summary, vec![("dev", 1), ("monitoring", 1)]);
    assert!((groups[0].cpu_cores - 0.25).abs() < 1e-9);
}

#[test]
fn flatten_emits_header_then_rows_and_skips_collapsed_rows() {
    let table = sample();
    let groups = group_by_namespace(&table);
    let open = flatten_groups(&groups, &Default::default());
    assert_eq!(open.len(), 3 + 5);
    assert_eq!(open[0], GroupItem::Header(0));
    assert_eq!(open[1], GroupItem::Row(0));
    assert_eq!(open[3], GroupItem::Header(1));

    let collapsed = ["kube-system".to_string()].into_iter().collect();
    let items = flatten_groups(&groups, &collapsed);
    assert_eq!(items.len(), 3 + 3);
    assert!(items.contains(&GroupItem::Header(1)));
    assert!(!items.contains(&GroupItem::Row(2)));
}

#[test]
fn cluster_kpis_use_pods_nodes_and_capacity() {
    let table = sample();
    let mut node = ObjectRow::new(None, "node-1");
    node.capacity = Some(Capacity {
        cpu_cores: 4.0,
        memory_bytes: 1000,
    });
    let mut down = ObjectRow::new(None, "node-2");
    down.health = StatusTone::Error;
    down.capacity = Some(Capacity {
        cpu_cores: 4.0,
        memory_bytes: 1000,
    });
    let pods_usage = usage(&[("dev", "api", 1.0, 200), ("dev", "worker", 1.0, 300)]);

    let kpis = cluster_kpis(table.all_rows(), &pods_usage, &[node, down]);
    assert_eq!(
        (kpis.pods_total, kpis.pods_running, kpis.pods_problems),
        (5, 2, 2)
    );
    assert_eq!((kpis.nodes_total, kpis.nodes_ready), (2, 1));
    assert_eq!(kpis.cpu_used, Some(2.0));
    assert_eq!(kpis.memory_used, Some(500));
    assert_eq!(kpis.cpu_capacity, Some(8.0));
    assert_eq!(kpis.memory_capacity, Some(2000));
    assert_eq!(kpis.cpu_ratio(), Some(0.25));
    assert_eq!(kpis.memory_ratio(), Some(0.25));
}

#[test]
fn cluster_kpis_without_metrics_or_capacity_have_no_ratios() {
    let table = sample();
    let kpis: ClusterKpis = cluster_kpis(table.all_rows(), &UsageMap::default(), &[]);
    assert_eq!(kpis.cpu_used, None);
    assert_eq!(kpis.memory_used, None);
    assert_eq!(kpis.cpu_capacity, None);
    assert_eq!(kpis.cpu_ratio(), None);
    assert_eq!(kpis.memory_ratio(), None);
}

#[test]
fn bar_fill_is_clamped_and_visible_for_small_values() {
    assert_eq!(bar_fill(0.0, 10.0), 0.0);
    assert_eq!(bar_fill(5.0, 0.0), 0.0);
    assert_eq!(bar_fill(5.0, 10.0), 0.5);
    assert_eq!(bar_fill(20.0, 10.0), 1.0);
    assert!(bar_fill(0.001, 10.0) >= 0.04);
}

#[test]
fn usage_peak_tracks_the_largest_pod() {
    let mut table = sample();
    assert_eq!(table.usage_peak(), (0.0, 0));
    table.set_usage(usage(&[
        ("dev", "api", 0.5, 100),
        ("dev", "worker", 0.25, 900),
    ]));
    assert_eq!(table.usage_peak(), (0.5, 900));
}
