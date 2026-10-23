use natikube_resources::fit_widths;

const POD: [f32; 11] = [
    320.0, 36.0, 160.0, 100.0, 80.0, 90.0, 80.0, 140.0, 80.0, 150.0, 44.0,
];

#[test]
fn widths_that_fit_are_left_alone() {
    assert_eq!(fit_widths(&POD, 1300.0), POD.to_vec());
}

#[test]
fn small_deficits_are_paid_by_the_other_columns() {
    let fitted = fit_widths(&POD, 1180.0);
    assert!(fitted.iter().sum::<f32>() <= 1180.5);
    assert_eq!(fitted[0], 320.0, "the name column keeps its width");
    assert_eq!(fitted[1], 36.0);
    assert_eq!(fitted[10], 44.0);
    assert!(fitted[2] < 160.0);
}

#[test]
fn name_shrinks_only_after_the_others_hit_their_floor() {
    let fitted = fit_widths(&POD, 1000.0);
    assert!(fitted[0] < 320.0 && fitted[0] >= 320.0 * 0.6);
    assert_eq!(fitted[2], (160.0f32 * 0.75).floor());
}

#[test]
fn shrinking_stops_at_the_floors() {
    let widths = [320.0, 160.0, 150.0];
    let fitted = fit_widths(&widths, 100.0);
    assert_eq!(fitted[0], 192.0);
    assert_eq!(fitted[1], 120.0);
    assert_eq!(fitted[2], 112.0);
}

use natikube_core::{Appearance, kinds};
use natikube_resources::{appearance_columns, fit_widths_with_mins, header_min_width, printer_for};

#[test]
fn header_minimum_covers_title_and_sort_icon() {
    assert_eq!(header_min_width("", true), 0.0);
    assert!(header_min_width("Restarts", true) > header_min_width("Restarts", false));
    assert!(header_min_width("Containers", true) > 100.0 * 0.75);
}

#[test]
fn minimums_hold_where_the_plain_floor_would_cut_a_header() {
    let widths = [320.0, 100.0, 80.0, 150.0];
    let mins = [
        0.0,
        header_min_width("Containers", true),
        header_min_width("Restarts", true),
        0.0,
    ];
    let plain = fit_widths(&widths, 560.0);
    assert!(
        plain[1] < mins[1] || plain[2] < mins[2],
        "scenario must clip"
    );
    let fitted = fit_widths_with_mins(&widths, &mins, 560.0);
    assert!(fitted[1] >= mins[1].min(widths[1]));
    assert!(fitted[2] >= mins[2].min(widths[2]));
    assert!(fitted[3] < 150.0, "unconstrained columns still give way");
}

#[test]
fn minimums_never_widen_or_break_fitting_tables() {
    let widths = [320.0, 100.0, 80.0];
    let mins = [0.0, 500.0, 500.0];
    assert_eq!(
        fit_widths_with_mins(&widths, &mins, 1000.0),
        widths.to_vec()
    );
    let fitted = fit_widths_with_mins(&widths, &mins, 400.0);
    assert_eq!(
        &fitted[1..],
        &widths[1..],
        "designed width is the cap on a minimum"
    );
}

fn pod_keys(appearance: Appearance, stacked: bool) -> Vec<String> {
    let printer = printer_for(&kinds::POD);
    appearance_columns(printer.columns(), appearance, stacked)
        .into_iter()
        .map(|(ix, title)| {
            title
                .map(str::to_string)
                .unwrap_or_else(|| printer.columns()[ix].title.clone())
        })
        .collect()
}

#[test]
fn pod_columns_follow_the_design_per_appearance() {
    assert_eq!(
        pod_keys(Appearance::Command, true),
        [
            "Name", "Status", "CPU", "Memory", "Restarts", "Owner", "Age"
        ]
    );
    assert_eq!(
        pod_keys(Appearance::Native, false),
        ["Name", "Namespace", "CPU", "Memory", "Age", "Status"]
    );
    let standard = pod_keys(Appearance::Standard, false);
    assert_eq!(standard.len(), 10);
    assert!(standard.contains(&"Containers".to_string()));
}

#[test]
fn other_kinds_only_fold_the_namespace() {
    let printer = printer_for(&kinds::DEPLOYMENT);
    let plain = appearance_columns(printer.columns(), Appearance::Command, false);
    let folded = appearance_columns(printer.columns(), Appearance::Command, true);
    assert_eq!(plain.len(), printer.columns().len());
    assert_eq!(folded.len(), plain.len() - 1);
}
