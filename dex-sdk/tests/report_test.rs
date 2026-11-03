// Modified by Jatin Bharati (2026-11-01): new test file (fork M5); review fixes (2026-11-02). See NOTICE.
//! Building an [`SdkReport`] from DEX buffers, a manifest and a catalogue.

use std::fs;

use dexpa_core::axml::XmlValue;
use dexpa_core::manifest::Manifest;
use dexpa_sdk::catalogue::{bundled, load_exodus_json, load_toml};
use dexpa_sdk::report::{EXODUS_ATTRIBUTION, SCHEMA_VERSION, build_report, value_text};

fn positive() -> Vec<Vec<u8>> {
    vec![
        fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/sdk-positive.dex"
        ))
        .unwrap(),
    ]
}

#[test]
fn report_counts_and_matches_without_a_manifest() {
    let catalogue = bundled();
    let report = build_report(&catalogue, &positive(), None).unwrap();
    assert_eq!(report.schema_version, SCHEMA_VERSION);
    assert_eq!(report.dex_count, 1);
    assert_eq!(report.class_count, 5);
    assert_eq!(report.catalogue_size, catalogue.sdks.len());
    assert_eq!(report.package, None);
    assert_eq!(report.attribution, None);
    let ids: Vec<&str> = report.matches.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "firebase-analytics",
            "google-mobile-ads",
            "google-play-services",
            "sentry"
        ]
    );
}

#[test]
fn report_takes_package_and_versions_from_the_manifest() {
    let manifest = Manifest {
        package: Some("app.fixture".into()),
        version_code: Some(XmlValue::Int(7)),
        version_name: Some(XmlValue::Reference(0x7f0f_0001)),
        ..Manifest::default()
    };
    let report = build_report(&bundled(), &positive(), Some(&manifest)).unwrap();
    assert_eq!(report.package.as_deref(), Some("app.fixture"));
    assert_eq!(report.version_code.as_deref(), Some("7"));
    assert_eq!(report.version_name.as_deref(), Some("@0x7f0f0001"));
}

#[test]
fn exodus_entries_bring_the_attribution_and_skipped_count() {
    let exodus = load_exodus_json(
        r#"{"trackers": {"1": {"name": "S", "code_signature": "io.sentry.", "categories": []},
                         "2": {"name": "B", "code_signature": "(", "categories": []}}}"#,
    )
    .unwrap();
    let catalogue = bundled().merge(exodus).unwrap();
    let report = build_report(&catalogue, &positive(), None).unwrap();
    assert_eq!(report.attribution.as_deref(), Some(EXODUS_ATTRIBUTION));
    assert_eq!(report.skipped_signatures, 1);
}

#[test]
fn value_text_renders_each_value_type() {
    assert_eq!(value_text(&XmlValue::Int(-3)).as_deref(), Some("-3"));
    assert_eq!(value_text(&XmlValue::Hex(0x10)).as_deref(), Some("16"));
    assert_eq!(
        value_text(&XmlValue::String("1.0".into())).as_deref(),
        Some("1.0")
    );
    assert_eq!(value_text(&XmlValue::Bool(true)).as_deref(), Some("true"));
    assert_eq!(
        value_text(&XmlValue::Reference(0x7f01_0002)).as_deref(),
        Some("@0x7f010002")
    );
    assert_eq!(
        value_text(&XmlValue::AttributeRef(0x0101_0000)).as_deref(),
        Some("?0x01010000")
    );
    assert_eq!(value_text(&XmlValue::Null), None);
    assert_eq!(
        value_text(&XmlValue::Other {
            data_type: 4,
            data: 1
        })
        .as_deref(),
        Some("type 0x04: 0x00000001")
    );
}

#[test]
fn unreadable_dex_is_an_error() {
    assert!(build_report(&bundled(), &[b"dex\n035\0".to_vec()], None).is_err());
}

/// /review (2026-11-02): `sdks diff` must tell a catalogue change from an app change, so a
/// report names the catalogue that made it. Same rules, same fingerprint; any rule change
/// (a prefix, a key, an Exodus file added) gives another.
#[test]
fn the_report_fingerprints_its_catalogue() {
    let report = build_report(&bundled(), &positive(), None).unwrap();
    let fp = report.catalogue_fingerprint.clone().expect("fingerprint");
    assert_eq!(fp, bundled().fingerprint(), "deterministic");
    assert_eq!(report.manifest_error, None);
    let toml = |prefix: &str| {
        load_toml(&format!(
            "[[sdk]]\nid = \"x\"\nname = \"X\"\ncategory = \"c\"\npackages = [\"{prefix}\"]\n"
        ))
        .unwrap()
        .fingerprint()
    };
    assert_eq!(toml("com.x"), toml("com.x"));
    assert_ne!(toml("com.x"), toml("com.y"), "a changed prefix changes it");
    let with_key = load_toml(
        "[[sdk]]\nid = \"x\"\nname = \"X\"\ncategory = \"c\"\npackages = [\"com.x\"]\n\
         meta_data_keys = [\"com.x.KEY\"]\n",
    )
    .unwrap()
    .fingerprint();
    assert_ne!(toml("com.x"), with_key, "a meta-data key changes it");
    let exodus = load_exodus_json(
        r#"{"trackers": {"1": {"name": "S", "code_signature": "io.sentry.", "categories": []}}}"#,
    )
    .unwrap();
    assert_ne!(bundled().merge(exodus).unwrap().fingerprint(), fp);
}

/// Re-review (2026-11-02): a class definition whose descriptor is not a class (here, `V`) is not
/// counted, and the report says how many there were instead of dropping them silently.
#[test]
fn the_report_counts_unreadable_classes() {
    use dexpa_core::format::TypeIdx;
    use dexpa_core::parser::parse_dex;
    let mut bytes = positive().remove(0);
    let (void, defs_off) = {
        let dex = parse_dex(&bytes).unwrap();
        let void = (0..dex.type_count() as u32)
            .find(|&i| dex.type_descriptor(TypeIdx::new(i)) == Some("V"))
            .expect("the fixture uses void");
        (void, dex.header().class_defs_off as usize)
    };
    let before = build_report(&bundled(), &[bytes.clone()], None).unwrap();
    assert_eq!(before.unreadable_classes, 0);
    bytes[defs_off..defs_off + 4].copy_from_slice(&void.to_le_bytes()); // first class_def's class_idx
    let after = build_report(&bundled(), &[bytes], None).unwrap();
    assert_eq!(after.unreadable_classes, 1);
    assert_eq!(after.class_count, before.class_count - 1);
}
