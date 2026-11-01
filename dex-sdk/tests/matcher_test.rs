// Modified by Zero (2026-11-01): new test file (fork M3); review fixes (2026-11-02). See NOTICE.
use dexpa_core::axml::XmlValue;
use dexpa_core::manifest::{Component, ComponentKind, Manifest, MetaData};
use dexpa_sdk::catalogue::{Origin, bundled, load_exodus_json, load_toml};
use dexpa_sdk::inventory::{DefinedClass, Inventory};
use dexpa_sdk::matcher::{Source, match_sdks};

fn inv(names: &[(&str, usize)]) -> Inventory {
    let mut classes: Vec<DefinedClass> = names
        .iter()
        .map(|(n, d)| DefinedClass {
            name: n.to_string(),
            dex_index: *d,
            methods: 1,
        })
        .collect();
    classes.sort_by(|a, b| (&a.name, a.dex_index).cmp(&(&b.name, b.dex_index)));
    Inventory {
        dexes: vec![],
        classes,
        unreadable_classes: 0,
        prefixes: vec![],
    }
}

const TOML: &str = r#"
[[sdk]]
id = "foo"
name = "Foo SDK"
category = "analytics"
packages = ["com.foo"]
meta_data_keys = ["com.foo.API_KEY"]

[[sdk]]
id = "gms-ads"
name = "Google Mobile Ads"
category = "ads"
packages = ["com.google.android.gms.ads"]

[[sdk]]
id = "gms"
name = "Google Play Services"
category = "platform"
packages = ["com.google.android.gms"]
"#;

#[test]
fn package_prefixes_match_on_boundaries() {
    let cat = load_toml(TOML).unwrap();
    let found = match_sdks(
        &cat,
        &inv(&[
            ("com.foo.A", 0),
            ("com.foo.sub.B", 1),
            ("com.foo$C", 0),
            ("com.foobar.D", 0),
        ]),
        None,
    );
    assert_eq!(found.len(), 1);
    let e = &found[0].evidence[0];
    assert_eq!(
        (found[0].id.as_str(), e.source, e.rule.as_str(), e.count),
        ("foo", Source::DexClass, "com.foo", 2)
    );
    assert_eq!(e.examples, vec!["com.foo.A", "com.foo.sub.B"]);
    assert_eq!(e.dex_indexes, vec![0, 1]);
}

#[test]
fn overlapping_sdks_are_all_reported_sorted_by_id() {
    let cat = load_toml(TOML).unwrap();
    let found = match_sdks(
        &cat,
        &inv(&[("com.google.android.gms.ads.AdView", 0)]),
        None,
    );
    let ids: Vec<&str> = found.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["gms", "gms-ads"]);
}

#[test]
fn manifest_evidence_matches_when_the_classes_were_renamed() {
    // R8 renamed every class to a.b.c, but the manifest still names the SDK's components.
    let cat = load_toml(TOML).unwrap();
    let manifest = Manifest {
        package: Some("org.app".into()),
        components: vec![
            Component {
                kind: ComponentKind::Service,
                name: Some("com.foo.push.FooService".into()),
                exported: None,
                authorities: None,
            },
            Component {
                kind: ComponentKind::Activity,
                name: Some(".Main".into()),
                exported: None,
                authorities: None,
            },
        ],
        meta_data: vec![MetaData {
            owner: None,
            name: Some("com.foo.API_KEY".into()),
            value: Some(XmlValue::String("k".into())),
        }],
        ..Manifest::default()
    };
    let found = match_sdks(&cat, &inv(&[("a.b.c", 0), ("a.b.d", 0)]), Some(&manifest));
    assert_eq!(found.len(), 1);
    let sources: Vec<(Source, &str, usize)> = found[0]
        .evidence
        .iter()
        .map(|e| (e.source, e.rule.as_str(), e.count))
        .collect();
    assert_eq!(
        sources,
        vec![
            (Source::ManifestComponent, "com.foo", 1),
            (Source::ManifestMeta, "com.foo.API_KEY", 1)
        ]
    );
    assert_eq!(
        found[0].evidence[0].examples,
        vec!["com.foo.push.FooService"]
    );
}

#[test]
fn relative_component_names_are_qualified_before_matching() {
    let cat = load_toml(TOML).unwrap();
    let manifest = Manifest {
        package: Some("com.foo".into()),
        components: vec![Component {
            kind: ComponentKind::Activity,
            name: Some(".Main".into()),
            exported: None,
            authorities: None,
        }],
        ..Manifest::default()
    };
    let found = match_sdks(&cat, &inv(&[]), Some(&manifest));
    assert_eq!(found[0].evidence[0].examples, vec!["com.foo.Main"]);
}

#[test]
fn bad_catalogues_are_refused_never_panic() {
    let one = |body: &str| {
        load_toml(&format!(
            "[[sdk]]\nid = \"x\"\nname = \"X\"\ncategory = \"c\"\n{body}"
        ))
    };
    assert!(one("packages = [\"com.x\"]").is_ok());
    assert!(
        one("packages = [\"\"]").is_err(),
        "empty prefix would match everything"
    );
    assert!(one("packages = [\"comx\"]").is_err(), "one segment");
    assert!(one("packages = [\"com..x\"]").is_err(), "empty segment");
    assert!(one("").is_err(), "nothing to match");
    let dup = "[[sdk]]\nid = \"x\"\nname = \"X\"\ncategory = \"c\"\npackages = [\"com.x\"]\n";
    assert!(load_toml(&format!("{dup}{dup}")).is_err(), "duplicate id");
    for junk in ["", "[[sdk]]", "sdk = 3", "\u{0}\u{1}", "[[sdk]]\nid = 7"] {
        assert!(load_toml(junk).is_err() || load_toml(junk).unwrap().sdks.is_empty());
    }
}

const EXODUS: &str = r#"{"trackers": {
  "1": {"id": 1, "name": "Bar Analytics", "code_signature": "com.bar.|org.bar.sdk", "categories": ["Analytics"]},
  "2": {"id": 2, "name": "Broken", "code_signature": "com.(unclosed", "categories": []},
  "3": {"id": 3, "name": "Empty", "code_signature": "", "categories": []}
}}"#;

#[test]
fn exodus_signatures_are_anchored_and_bad_ones_skipped() {
    let cat = load_exodus_json(EXODUS).unwrap();
    assert_eq!(cat.sdks.len(), 1);
    assert_eq!(
        cat.skipped.len(),
        2,
        "invalid regex and empty signature are counted, not fatal"
    );
    assert_eq!(cat.sdks[0].origin, Origin::Exodus);
    assert_eq!(cat.sdks[0].id, "exodus:1");
    let found = match_sdks(
        &cat,
        &inv(&[
            ("com.bar.X", 0),
            ("org.bar.sdk.Y", 1),
            ("x.com.bar.Z", 0),
            ("com.barn.W", 0),
        ]),
        None,
    );
    assert_eq!(found.len(), 1);
    let e = &found[0].evidence[0];
    // Exodus signatures are regexes, so the `.` in `com.bar.` matches any character and
    // `com.barn.W` matches, as it would in Exodus. Only the anchoring differs from Exodus.
    assert_eq!(
        (e.source, e.rule.as_str(), e.count),
        (Source::DexClass, "signature", 3)
    );
    assert_eq!(
        e.examples,
        vec!["com.bar.X", "com.barn.W", "org.bar.sdk.Y"],
        "anchored: x.com.bar.Z does not match"
    );
    for junk in ["", "{}", "[]", "{\"trackers\": 3}", "\u{0}"] {
        let _ = load_exodus_json(junk);
    }
}

#[test]
fn catalogues_merge_and_reject_id_collisions() {
    let a = load_toml(TOML).unwrap();
    let b = load_exodus_json(EXODUS).unwrap();
    let merged = a.clone().merge(b).unwrap();
    assert_eq!(merged.sdks.len(), 4);
    assert!(a.clone().merge(a).is_err());
}

#[test]
fn bundled_catalogue_loads_and_follows_its_rules() {
    let cat = bundled();
    assert!(cat.sdks.len() >= 20, "{} entries", cat.sdks.len());
    assert!(cat.sdks.iter().all(|s| s.origin == Origin::Catalogue));
}

/// Found in the F-Droid evaluation (NOTES.md, M6): an app with Fresco (React Native's image
/// library) matched `facebook` through the old `com.facebook` prefix. Fresco, React Native and
/// Yoga live under `com.facebook` but are open-source libraries that send nothing to Facebook.
#[test]
fn facebook_open_source_libraries_are_not_the_facebook_sdk() {
    let cat = bundled();
    let ids = |names: &[(&str, usize)]| -> Vec<String> {
        match_sdks(&cat, &inv(names), None)
            .into_iter()
            .map(|m| m.id)
            .collect()
    };
    for library in [
        "com.facebook.animated.gif.GifImage",
        "com.facebook.drawee.view.SimpleDraweeView",
        "com.facebook.imagepipeline.core.ImagePipeline",
        "com.facebook.react.ReactActivity",
        "com.facebook.soloader.SoLoader",
        "com.facebook.yoga.YogaNode",
    ] {
        assert!(
            !ids(&[(library, 0)]).contains(&"facebook".to_string()),
            "{library} is not the Facebook SDK"
        );
    }
    for sdk in [
        "com.facebook.appevents.AppEventsLogger",
        "com.facebook.login.LoginManager",
        "com.facebook.share.widget.ShareButton",
        "com.facebook.ads.AdView",
    ] {
        assert!(ids(&[(sdk, 0)]).contains(&"facebook".to_string()), "{sdk}");
    }
}

/// /review (2026-11-02): a signature was wrapped as `^(?:SIG)` before it was checked. An
/// unbalanced `a)|(b` then compiled as `^(?:a)|(b)`, whose second branch is not anchored.
#[test]
fn an_unbalanced_exodus_signature_cannot_escape_the_anchor() {
    let cat = load_exodus_json(
        r#"{"trackers": {"1": {"name": "Esc", "code_signature": "com.bar)|(evil", "categories": []}}}"#,
    )
    .unwrap();
    assert!(
        cat.sdks.is_empty(),
        "the raw signature is not a valid regex"
    );
    assert_eq!(cat.skipped.len(), 1);
    assert!(match_sdks(&cat, &inv(&[("x.evil.Y", 0)]), None).is_empty());
}

/// /review (2026-11-02): signature rules are matched in one `RegexSet` pass, whose hits are
/// indices into the rule list. A hit must land on the SDK whose rule matched, not on another.
#[test]
fn each_signature_hit_belongs_to_the_rule_that_matched() {
    let cat = load_exodus_json(
        r#"{"trackers": {
  "1": {"name": "A", "code_signature": "com.aaa.", "categories": []},
  "2": {"name": "B", "code_signature": "org.bbb.", "categories": []}
}}"#,
    )
    .unwrap();
    let found = match_sdks(&cat, &inv(&[("org.bbb.Y", 0)]), None);
    let ids: Vec<&str> = found.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["exodus:2"]);
}
