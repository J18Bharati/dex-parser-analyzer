// Modified by Jatin Bharati (2026-11-01): new test file (fork M1b); review fixes (2026-11-02). See NOTICE.
//! The AXML parser and the AndroidManifest model.

#[path = "common/axml_builder.rs"]
mod axml_builder;

use std::io::{Cursor, Write};

use axml_builder::{El, Opts, V, aa, build, el, pa, s};
use dexpa_core::axml::{XmlValue, parse_axml};
use dexpa_core::manifest::{
    ComponentKind, Manifest, parse_manifest, read_manifest_from_apk_bytes,
    read_manifest_from_apk_bytes_with_limit,
};
use zip::write::{SimpleFileOptions, ZipWriter};

const NAME: u32 = 0x0101_0003;
const EXPORTED: u32 = 0x0101_0010;
const AUTHORITIES: u32 = 0x0101_0018;
const VALUE: u32 = 0x0101_0024;
const RESOURCE: u32 = 0x0101_0025;
const VERSION_CODE: u32 = 0x0101_021b;
const VERSION_NAME: u32 = 0x0101_021c;
const MIN_SDK: u32 = 0x0101_020c;
const TARGET_SDK: u32 = 0x0101_0270;

fn sample() -> El {
    el(
        "manifest",
        vec![
            pa("package", s("org.example.app")),
            aa("versionCode", VERSION_CODE, V::Int(42)),
            aa("versionName", VERSION_NAME, s("1.4.2")),
        ],
        vec![
            el(
                "uses-sdk",
                vec![
                    aa("minSdkVersion", MIN_SDK, V::Int(21)),
                    aa("targetSdkVersion", TARGET_SDK, V::Int(34)),
                ],
                vec![],
            ),
            el(
                "uses-permission",
                vec![aa("name", NAME, s("android.permission.INTERNET"))],
                vec![],
            ),
            el(
                "uses-permission-sdk-23",
                vec![aa("name", NAME, s("android.permission.CAMERA"))],
                vec![],
            ),
            el(
                "application",
                vec![aa("name", NAME, s(".App"))],
                vec![
                    el(
                        "meta-data",
                        vec![
                            aa("name", NAME, s("io.sentry.dsn")),
                            aa("value", VALUE, s("https://k@o.example/1")),
                        ],
                        vec![],
                    ),
                    el(
                        "meta-data",
                        vec![
                            aa("name", NAME, s("com.google.android.gms.version")),
                            aa("value", VALUE, V::Int(12451000)),
                        ],
                        vec![],
                    ),
                    el(
                        "meta-data",
                        vec![
                            aa("name", NAME, s("theme.ref")),
                            aa("resource", RESOURCE, V::Ref(0x7f0f_0001)),
                        ],
                        vec![],
                    ),
                    el(
                        "activity",
                        vec![
                            aa("name", NAME, s(".MainActivity")),
                            aa("exported", EXPORTED, V::Bool(true)),
                        ],
                        vec![el(
                            "meta-data",
                            vec![
                                aa("name", NAME, s("inner.key")),
                                aa("value", VALUE, V::Bool(false)),
                            ],
                            vec![],
                        )],
                    ),
                    el(
                        "activity-alias",
                        vec![aa("name", NAME, s("Launcher"))],
                        vec![],
                    ),
                    el(
                        "service",
                        vec![
                            aa(
                                "name",
                                NAME,
                                s("com.google.firebase.messaging.FirebaseMessagingService"),
                            ),
                            aa("exported", EXPORTED, V::Bool(false)),
                        ],
                        vec![],
                    ),
                    el(
                        "receiver",
                        vec![aa("name", NAME, s("org.example.app.BootReceiver"))],
                        vec![],
                    ),
                    el(
                        "provider",
                        vec![
                            aa("name", NAME, s("androidx.startup.InitializationProvider")),
                            aa(
                                "authorities",
                                AUTHORITIES,
                                s("org.example.app.androidx-startup"),
                            ),
                        ],
                        vec![],
                    ),
                ],
            ),
        ],
    )
}

fn check_sample(m: &Manifest) {
    assert_eq!(m.package.as_deref(), Some("org.example.app"));
    assert_eq!(m.version_code, Some(XmlValue::Int(42)));
    assert_eq!(m.version_name, Some(XmlValue::String("1.4.2".into())));
    assert_eq!(m.min_sdk, Some(XmlValue::Int(21)));
    assert_eq!(m.target_sdk, Some(XmlValue::Int(34)));
    assert_eq!(
        m.uses_permissions,
        vec!["android.permission.INTERNET", "android.permission.CAMERA"]
    );

    let kinds: Vec<ComponentKind> = m.components.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        vec![
            ComponentKind::Activity,
            ComponentKind::ActivityAlias,
            ComponentKind::Service,
            ComponentKind::Receiver,
            ComponentKind::Provider
        ]
    );
    let main = &m.components[0];
    assert_eq!(main.name.as_deref(), Some(".MainActivity"));
    assert_eq!(
        main.qualified_name(m.package.as_deref()).as_deref(),
        Some("org.example.app.MainActivity")
    );
    assert_eq!(main.exported, Some(true));
    assert_eq!(
        m.components[1]
            .qualified_name(m.package.as_deref())
            .as_deref(),
        Some("org.example.app.Launcher")
    );
    assert_eq!(m.components[2].exported, Some(false));
    assert_eq!(
        m.components[4].authorities.as_deref(),
        Some("org.example.app.androidx-startup")
    );

    let md: Vec<(Option<&str>, Option<&str>, Option<&XmlValue>)> = m
        .meta_data
        .iter()
        .map(|d| (d.owner.as_deref(), d.name.as_deref(), d.value.as_ref()))
        .collect();
    assert_eq!(
        md,
        vec![
            (
                None,
                Some("io.sentry.dsn"),
                Some(&XmlValue::String("https://k@o.example/1".into()))
            ),
            (
                None,
                Some("com.google.android.gms.version"),
                Some(&XmlValue::Int(12451000))
            ),
            (
                None,
                Some("theme.ref"),
                Some(&XmlValue::Reference(0x7f0f_0001))
            ),
            (
                Some(".MainActivity"),
                Some("inner.key"),
                Some(&XmlValue::Bool(false))
            ),
        ]
    );
}

#[test]
fn utf16_pool_manifest() {
    check_sample(&parse_manifest(&build(&sample(), &Opts::default())).unwrap());
}

#[test]
fn utf8_pool_manifest() {
    check_sample(
        &parse_manifest(&build(
            &sample(),
            &Opts {
                utf8: true,
                ..Opts::default()
            },
        ))
        .unwrap(),
    );
}

#[test]
fn attribute_names_stripped_from_the_pool_still_resolve_by_resource_id() {
    for utf8 in [false, true] {
        let doc = build(
            &sample(),
            &Opts {
                utf8,
                strip_names: true,
                ..Opts::default()
            },
        );
        check_sample(&parse_manifest(&doc).unwrap());
    }
}

#[test]
fn unknown_chunks_are_skipped() {
    check_sample(
        &parse_manifest(&build(
            &sample(),
            &Opts {
                unknown_chunk: true,
                ..Opts::default()
            },
        ))
        .unwrap(),
    );
}

#[test]
fn version_name_reference_stays_a_reference() {
    let root = el(
        "manifest",
        vec![
            pa("package", s("p.q")),
            aa("versionName", VERSION_NAME, V::Ref(0x7f13_0042)),
        ],
        vec![],
    );
    let m = parse_manifest(&build(&root, &Opts::default())).unwrap();
    assert_eq!(m.version_name, Some(XmlValue::Reference(0x7f13_0042)));
}

#[test]
fn hex_and_long_strings_decode() {
    let long = "x".repeat(300); // two-byte length prefixes in both encodings
    for utf8 in [false, true] {
        let root = el(
            "manifest",
            vec![
                pa("package", V::Str(long.clone())),
                aa("versionCode", VERSION_CODE, V::Hex(0x10)),
            ],
            vec![],
        );
        let m = parse_manifest(&build(
            &root,
            &Opts {
                utf8,
                ..Opts::default()
            },
        ))
        .unwrap();
        assert_eq!(m.package.as_deref(), Some(long.as_str()));
        assert_eq!(m.version_code, Some(XmlValue::Hex(0x10)));
    }
}

#[test]
fn element_tree_is_returned() {
    let root = parse_axml(&build(&sample(), &Opts::default())).unwrap();
    assert_eq!(root.name, "manifest");
    let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "uses-sdk",
            "uses-permission",
            "uses-permission-sdk-23",
            "application"
        ]
    );
}

#[test]
fn qualified_names_follow_android_rules() {
    let root = el(
        "manifest",
        vec![pa("package", s("a.b"))],
        vec![el(
            "application",
            vec![],
            vec![
                el("service", vec![aa("name", NAME, s(".Dot"))], vec![]),
                el("service", vec![aa("name", NAME, s("Bare"))], vec![]),
                el("service", vec![aa("name", NAME, s("x.y.Full"))], vec![]),
            ],
        )],
    );
    let m = parse_manifest(&build(&root, &Opts::default())).unwrap();
    let q: Vec<String> = m
        .components
        .iter()
        .filter_map(|c| c.qualified_name(m.package.as_deref()))
        .collect();
    assert_eq!(q, vec!["a.b.Dot", "a.b.Bare", "x.y.Full"]);
}

/// Every truncation of a valid document parses or fails, and never panics.
#[test]
fn every_truncation_is_an_error_or_ok_never_a_panic() {
    for utf8 in [false, true] {
        let doc = build(
            &sample(),
            &Opts {
                utf8,
                ..Opts::default()
            },
        );
        for n in 0..doc.len() {
            let _ = parse_manifest(&doc[..n]);
        }
        assert!(parse_manifest(&doc[..doc.len() / 2]).is_err());
    }
}

/// Every single-byte corruption of a valid document parses or fails, and never panics.
#[test]
fn every_byte_flip_is_an_error_or_ok_never_a_panic() {
    for utf8 in [false, true] {
        let doc = build(
            &sample(),
            &Opts {
                utf8,
                ..Opts::default()
            },
        );
        for i in 0..doc.len() {
            for mask in [0xff, 0x80, 0x01] {
                let mut d = doc.clone();
                d[i] ^= mask;
                let _ = parse_manifest(&d);
            }
        }
    }
}

#[test]
fn deep_nesting_is_refused_without_overflowing_the_stack() {
    let mut e = el("leaf", vec![], vec![]);
    for _ in 0..1_000 {
        e = el("n", vec![], vec![e]);
    }
    let root = el("manifest", vec![], vec![e]);
    assert!(parse_axml(&build(&root, &Opts::default())).is_err());
}

#[test]
fn a_string_pool_that_points_every_index_at_one_string_cannot_blow_up_memory() {
    // One 30,000-byte value referenced by 5,000 attributes would decode to 150 MB of strings.
    // (UTF-8 pool lengths top out at 0x7fff, so the string stays under that.)
    let big = "y".repeat(30_000);
    let attrs: Vec<_> = (0..5_000).map(|_| pa("k", V::Str(big.clone()))).collect();
    let root = el("manifest", attrs, vec![]);
    let doc = build(
        &root,
        &Opts {
            utf8: true,
            ..Opts::default()
        },
    );
    assert!(doc.len() < 1_000_000, "the input itself is small");
    assert!(parse_axml(&doc).is_err(), "decoded output must be bounded");
}

fn apk_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn manifest_is_read_from_an_apk() {
    let doc = build(&sample(), &Opts::default());
    let apk = apk_with(&[
        ("AndroidManifest.xml", &doc),
        ("classes.dex", b"dex\n035\0"),
    ]);
    check_sample(&read_manifest_from_apk_bytes(&apk).unwrap());
    assert!(
        read_manifest_from_apk_bytes(&apk_with(&[("classes.dex", b"x")])).is_err(),
        "no manifest"
    );
    assert!(
        read_manifest_from_apk_bytes_with_limit(&apk, doc.len() as u64 - 1).is_err(),
        "over the cap"
    );
    assert!(read_manifest_from_apk_bytes_with_limit(&apk, doc.len() as u64).is_ok());
}

/// /review (2026-11-02): `u64::MAX` as a limit means "no limit"; `limit + 1` overflowed on it.
#[test]
fn a_u64_max_manifest_limit_means_no_limit() {
    let doc = build(&sample(), &Opts::default());
    let apk = apk_with(&[("AndroidManifest.xml", &doc)]);
    check_sample(&read_manifest_from_apk_bytes_with_limit(&apk, u64::MAX).unwrap());
}

/// /review (2026-11-02): an attribute without a resource id, such as `package`, is read the way
/// `XmlPullParser.getAttributeValue(null, "package")` reads it: through its raw string index
/// (AOSP `ResXMLParser::getAttributeStringValue`), not the typed value. A manifest whose two
/// indexes disagree must show what Android sees.
#[test]
fn a_string_attribute_reads_its_raw_value_like_android() {
    let mut root = sample();
    root.attrs[0] = pa(
        "package",
        V::StrSplit("org.example.real".into(), "org.example.decoy".into()),
    );
    let m = parse_manifest(&build(&root, &Opts::default())).unwrap();
    assert_eq!(m.package.as_deref(), Some("org.example.real"));
}

/// Re-review (2026-11-02): `android:` attributes are read through `TypedArray`
/// (`obtainAttributes`, matched by resource id), which uses the typed value's string index. A
/// decoy raw value must not hide the name or key Android actually sees.
#[test]
fn a_framework_attribute_reads_its_typed_value_like_android() {
    let mut root = sample();
    root.attrs[2] = aa(
        "versionName",
        VERSION_NAME,
        V::StrSplit("0.0-decoy".into(), "1.4.2".into()),
    );
    root.children.push(el(
        "uses-permission",
        vec![aa(
            "name",
            NAME,
            V::StrSplit(
                "android.permission.VIBRATE".into(),
                "android.permission.READ_CONTACTS".into(),
            ),
        )],
        vec![],
    ));
    let m = parse_manifest(&build(&root, &Opts::default())).unwrap();
    assert_eq!(m.version_name, Some(XmlValue::String("1.4.2".into())));
    assert!(
        m.uses_permissions
            .contains(&"android.permission.READ_CONTACTS".to_string())
    );
    assert!(
        !m.uses_permissions
            .contains(&"android.permission.VIBRATE".to_string())
    );
}

/// Re-review (2026-11-02): `<uses-permission-sdk-m>` is Android's other name for
/// `<uses-permission-sdk-23>`.
#[test]
fn uses_permission_sdk_m_is_a_permission() {
    let mut root = sample();
    root.children.push(el(
        "uses-permission-sdk-m",
        vec![aa("name", NAME, s("android.permission.BODY_SENSORS"))],
        vec![],
    ));
    let m = parse_manifest(&build(&root, &Opts::default())).unwrap();
    assert!(
        m.uses_permissions
            .contains(&"android.permission.BODY_SENSORS".to_string())
    );
}

/// /review (2026-11-02): one root element only; a second one is an error, not a replacement.
#[test]
fn a_second_root_element_is_an_error() {
    let opts = Opts {
        second_root: true,
        ..Opts::default()
    };
    assert!(parse_axml(&build(&sample(), &opts)).is_err());
    assert!(parse_axml(&build(&sample(), &Opts::default())).is_ok());
}

/// /review (2026-11-02): elements name their strings by pool index, so a pool that comes after
/// them is an error.
#[test]
fn an_element_before_the_string_pool_is_an_error() {
    let opts = Opts {
        pool_after_body: true,
        ..Opts::default()
    };
    assert!(parse_axml(&build(&sample(), &opts)).is_err());
}

/// /review (2026-11-02): two `AndroidManifest.xml` entries are refused: the zip crate keeps one
/// of them, and Android or another tool may read the other.
#[test]
fn duplicate_manifest_entries_are_refused() {
    let doc = build(&sample(), &Opts::default());
    let mut apk = apk_with(&[("AndroidManifest.xml", &doc), ("AndroidManifest.xmm", &doc)]);
    let (from, to) = (b"AndroidManifest.xmm", b"AndroidManifest.xml");
    let mut i = 0;
    while i + from.len() <= apk.len() {
        if &apk[i..i + from.len()] == from {
            apk[i..i + from.len()].copy_from_slice(to);
        }
        i += 1;
    }
    assert!(read_manifest_from_apk_bytes(&apk).is_err());
}
