// Modified by Zero (2026-11-01): new test file (fork M4); new report fields; unread manifests (2026-11-02, /review). See NOTICE.
use dexpa_sdk::catalogue::Origin;
use dexpa_sdk::diff::{Confidence, DiffEvent, diff};
use dexpa_sdk::matcher::{Evidence, SdkMatch, Source};
use dexpa_sdk::report::{SCHEMA_VERSION, SdkReport};

fn ev(source: Source, rule: &str, count: usize, dex: &[usize]) -> Evidence {
    Evidence {
        source,
        rule: rule.into(),
        count,
        examples: vec![],
        dex_indexes: dex.to_vec(),
    }
}

fn m(id: &str, evidence: Vec<Evidence>) -> SdkMatch {
    SdkMatch {
        id: id.into(),
        name: id.to_uppercase(),
        category: "c".into(),
        origin: Origin::Catalogue,
        evidence,
    }
}

fn report(matches: Vec<SdkMatch>) -> SdkReport {
    SdkReport {
        schema_version: SCHEMA_VERSION,
        package: Some("p".into()),
        version_code: None,
        version_name: None,
        dex_count: 1,
        class_count: 10,
        catalogue_size: 25,
        skipped_signatures: 0,
        attribution: None,
        catalogue_fingerprint: None,
        manifest_error: None,
        matches,
    }
}

#[test]
fn identical_reports_have_no_events() {
    let r = report(vec![m("a", vec![ev(Source::DexClass, "com.a", 3, &[0])])]);
    assert!(diff(&r, &r).is_empty());
}

#[test]
fn added_and_removed_carry_confidence_from_the_side_that_has_the_sdk() {
    let old = report(vec![m(
        "gone",
        vec![ev(Source::ManifestComponent, "com.g", 1, &[])],
    )]);
    let new = report(vec![
        m("classes-only", vec![ev(Source::DexClass, "com.c", 5, &[0])]),
        m(
            "with-meta",
            vec![
                ev(Source::DexClass, "com.w", 2, &[0]),
                ev(Source::ManifestMeta, "com.w.KEY", 1, &[]),
            ],
        ),
    ]);
    assert_eq!(
        diff(&old, &new),
        vec![
            DiffEvent::SdkAdded {
                id: "classes-only".into(),
                name: "CLASSES-ONLY".into(),
                confidence: Confidence::Uncertain
            },
            DiffEvent::SdkRemoved {
                id: "gone".into(),
                name: "GONE".into(),
                confidence: Confidence::Strong
            },
            DiffEvent::SdkAdded {
                id: "with-meta".into(),
                name: "WITH-META".into(),
                confidence: Confidence::Strong
            },
        ],
        "sorted by id"
    );
}

#[test]
fn classes_moving_between_dex_files_or_counts_changing_is_not_an_event() {
    let old = report(vec![m("a", vec![ev(Source::DexClass, "com.a", 3, &[0])])]);
    let new = report(vec![m(
        "a",
        vec![ev(Source::DexClass, "com.a", 40, &[1, 2])],
    )]);
    assert!(diff(&old, &new).is_empty());
}

#[test]
fn a_change_in_kinds_of_evidence_is_reported() {
    // Minified in the new version: the class evidence is gone, the manifest evidence stays.
    let old = report(vec![m(
        "a",
        vec![
            ev(Source::DexClass, "com.a", 3, &[0]),
            ev(Source::ManifestComponent, "com.a", 1, &[]),
        ],
    )]);
    let new = report(vec![m(
        "a",
        vec![
            ev(Source::ManifestComponent, "com.a", 1, &[]),
            ev(Source::ManifestMeta, "com.a.KEY", 1, &[]),
        ],
    )]);
    assert_eq!(
        diff(&old, &new),
        vec![DiffEvent::SdkEvidenceChanged {
            id: "a".into(),
            gained: vec!["manifest_meta:com.a.KEY".into()],
            lost: vec!["dex_class:com.a".into()],
        }]
    );
}

#[test]
fn reports_round_trip_through_json() {
    let r = report(vec![m("a", vec![ev(Source::DexClass, "com.a", 3, &[0])])]);
    let back: SdkReport = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
    assert_eq!(back, r);
}

/// /review (2026-11-02): a side whose manifest was not read (DEX-only input, or `manifest_error`)
/// says nothing about manifest evidence, so manifest evidence on the other side cannot make an
/// added or removed SDK `strong`.
#[test]
fn an_unread_manifest_makes_added_and_removed_uncertain() {
    let with_manifest = report(vec![m(
        "tracker",
        vec![ev(Source::ManifestComponent, "com.t", 1, &[])],
    )]);
    let mut broken = report(vec![]);
    broken.manifest_error = Some("binary XML is malformed".into());
    let mut dex_only = report(vec![]);
    dex_only.package = None;
    for unread in [&broken, &dex_only] {
        assert_eq!(
            diff(&with_manifest, unread),
            vec![DiffEvent::SdkRemoved {
                id: "tracker".into(),
                name: "TRACKER".into(),
                confidence: Confidence::Uncertain
            }]
        );
        assert_eq!(
            diff(unread, &with_manifest),
            vec![DiffEvent::SdkAdded {
                id: "tracker".into(),
                name: "TRACKER".into(),
                confidence: Confidence::Uncertain
            }]
        );
    }
}
