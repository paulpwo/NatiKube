use natikube_updates::{available_update, parse_releases, user_agent};

const FIXTURE: &str = include_str!("fixtures/releases.json");

#[test]
fn parses_the_fixture_skipping_drafts_odd_tags_and_bad_entries() {
    let releases = parse_releases(FIXTURE).unwrap();
    let tags: Vec<_> = releases.iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(tags, ["v0.2.0-alpha.2", "v0.2.0-alpha.1", "v0.1.0"]);

    let alpha2 = &releases[0];
    assert_eq!(alpha2.name.as_deref(), Some("NatiKube 0.2.0 alpha 2"));
    assert!(alpha2.notes.contains("Update check"));
    assert_eq!(
        alpha2.url,
        "https://github.com/paulpwo/NatiKube/releases/tag/v0.2.0-alpha.2"
    );
}

#[test]
fn a_foreign_release_url_falls_back_to_the_tag_page() {
    let releases = parse_releases(FIXTURE).unwrap();
    let alpha1 = &releases[1];
    assert_eq!(alpha1.name, None);
    assert_eq!(
        alpha1.url,
        "https://github.com/paulpwo/NatiKube/releases/tag/v0.2.0-alpha.1"
    );
}

#[test]
fn fixture_drives_the_offer_rules() {
    let releases = || parse_releases(FIXTURE).unwrap();
    let offer = available_update("0.1.0", releases(), None).unwrap();
    assert_eq!(offer.tag, "v0.2.0-alpha.2");
    assert!(available_update("0.1.0", releases(), Some("0.2.0-alpha.2")).is_none());
    assert!(available_update("0.2.0-alpha.2", releases(), None).is_none());
}

#[test]
fn rejects_non_array_json_and_keeps_empty_arrays() {
    assert!(parse_releases("{\"message\":\"API rate limit exceeded\"}").is_err());
    assert!(parse_releases("not json").is_err());
    assert!(parse_releases("[]").unwrap().is_empty());
}

#[test]
fn user_agent_carries_only_the_version() {
    assert_eq!(user_agent("0.1.0"), "NatiKube/0.1.0");
}
