use super::*;
use serde_json::json;

#[test]
fn entries_decode_base64_values_sorted_by_key() {
    let obj = json!({"data": {"user": "YWRtaW4=", "bin": "/wA="}});
    let entries = secret_entries(&obj);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].key, "bin");
    assert_eq!(entries[0].decoded(), None);
    assert_eq!(entries[0].byte_len(), 2);
    assert_eq!(entries[1].key, "user");
    assert_eq!(entries[1].decoded().as_deref(), Some("admin"));
}

#[test]
fn display_is_masked_until_revealed() {
    let obj = json!({"data": {"user": "YWRtaW4=", "bin": "/wA="}});
    let entries = secret_entries(&obj);
    assert_eq!(display_value(&entries[1], false), MASK);
    assert_eq!(display_value(&entries[1], true), "admin");
    assert_eq!(display_value(&entries[0], false), MASK);
    assert_eq!(display_value(&entries[0], true), "<2 bytes of binary data>");
}

#[test]
fn string_data_is_listed_as_plain_text() {
    let obj = json!({"stringData": {"token": "abc"}});
    let entries = secret_entries(&obj);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].decoded().as_deref(), Some("abc"));
}

#[test]
fn mask_replaces_every_value_and_leaves_input_untouched() {
    let obj = json!({
        "metadata": {"name": "s"},
        "data": {"user": "YWRtaW4="},
        "stringData": {"x": "plain-secret"}
    });
    let masked = mask_secret_data(&obj);
    assert_eq!(masked["data"]["user"], MASK);
    assert_eq!(masked["stringData"]["x"], MASK);
    assert_eq!(masked["metadata"]["name"], "s");
    let text = masked.to_string();
    assert!(!text.contains("YWRtaW4="));
    assert!(!text.contains("plain-secret"));
    assert_eq!(obj["data"]["user"], "YWRtaW4=");
}

#[test]
fn contains_mask_detects_the_sentinel() {
    let obj = json!({"data": {"user": "YWRtaW4="}});
    assert!(!contains_mask(&obj));
    assert!(contains_mask(&mask_secret_data(&obj)));
}

#[test]
fn mask_hides_last_applied_annotation_values() {
    let obj = json!({
        "metadata": {"annotations": {
            "kubectl.kubernetes.io/last-applied-configuration": "{\"data\":{\"user\":\"YWRtaW4=\"}}",
            "note": "keep"
        }},
        "data": {"user": "YWRtaW4="}
    });
    let masked = mask_secret_data(&obj);
    let ann = &masked["metadata"]["annotations"];
    assert_eq!(ann[LAST_APPLIED], MASK);
    assert_eq!(ann["note"], "keep");
    assert!(!masked.to_string().contains("YWRtaW4="));
    assert!(contains_mask(&masked));
}
