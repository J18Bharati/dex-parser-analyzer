// Modified by Jatin Bharati (2026-11-01): new test file (fork M5); review fixes (2026-11-02); sample app fixture (2026-11-04). See NOTICE.
//! `dexpa sdks` and `dexpa sdks diff`, driven through the built binary.
//!
//! The DEX fixtures (`dex-sdk/tests/data/sdk-{positive,negative}.dex`) are compiled from
//! `dex-sdk/tests/fixtures-src` by `just sdk-fixtures`, so which SDKs they contain is known from
//! their sources, not from this tool's output. APKs are assembled here around them, with a
//! manifest from the AXML test builder. Golden files: `UPDATE_GOLDEN=1 cargo test` rewrites them.

#[path = "../../dex-core/tests/common/axml_builder.rs"]
mod axml_builder;

use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use axml_builder::{El, Opts, V, aa, build, el, pa, s};
use serde_json::Value;
use zip::write::{SimpleFileOptions, ZipWriter};

const NAME: u32 = 0x0101_0003;
const VALUE: u32 = 0x0101_0024;
const VERSION_CODE: u32 = 0x0101_021b;
const VERSION_NAME: u32 = 0x0101_021c;

fn dexpa(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dexpa"))
        .args(args)
        .output()
        .expect("run dexpa")
}

fn sdk_fixture(kind: &str) -> String {
    format!(
        "{}/../dex-sdk/tests/data/sdk-{kind}.dex",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn core_fixture(name: &str) -> String {
    format!(
        "{}/../dex-core/tests/data/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sdks-cli");
    fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 stdout")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8 stderr")
}

fn json(output: &Output) -> Value {
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn ids(report: &Value) -> Vec<String> {
    report["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

fn meta(name: &str, value: V) -> El {
    el(
        "meta-data",
        vec![aa("name", NAME, s(name)), aa("value", VALUE, value)],
        vec![],
    )
}

/// v1: Play Services' version key only. v2 adds the Firebase messaging service (a component whose
/// classes are not in the DEX: the minified-app case) and Sentry's DSN key.
fn manifest(version: i32) -> Vec<u8> {
    let mut app = vec![meta("com.google.android.gms.version", V::Int(12451000))];
    if version >= 2 {
        app.push(el(
            "service",
            vec![aa(
                "name",
                NAME,
                s("com.google.firebase.messaging.FirebaseMessagingService"),
            )],
            vec![],
        ));
        app.push(meta("io.sentry.dsn", s("https://key@o.example/1")));
    }
    let root = el(
        "manifest",
        vec![
            pa("package", s("app.fixture")),
            aa("versionCode", VERSION_CODE, V::Int(version)),
            aa("versionName", VERSION_NAME, s(&format!("1.{version}"))),
        ],
        vec![el("application", vec![], app)],
    );
    build(&root, &Opts::default())
}

fn write_apk(name: &str, version: i32, dex_kind: &str) -> String {
    write_apk_with_manifest(name, manifest(version), dex_kind)
}

fn write_apk_with_manifest(name: &str, manifest: Vec<u8>, dex_kind: &str) -> String {
    let path = scratch(name);
    fs::write(&path, apk_bytes(manifest, dex_kind)).unwrap();
    path.display().to_string()
}

/// A two-entry APK around a fixture DEX. The entry timestamp is fixed (the zip epoch, 1980), so
/// the same inputs always give the same bytes: the committed sample app depends on it.
fn apk_bytes(manifest: Vec<u8>, dex_kind: &str) -> Vec<u8> {
    let dex = fs::read(sdk_fixture(dex_kind)).unwrap();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    for (entry, data) in [("AndroidManifest.xml", manifest), ("classes.dex", dex)] {
        zip.start_file(entry, opts).unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// The sample app the GUI offers to visitors without an APK (`dex-sdk/tests/data/sample-app.apk`):
/// the v2 manifest around the positive DEX.
fn sample_app_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../dex-sdk/tests/data/sample-app.apk")
}

/// Compares `actual` with `tests/golden/<name>`; `UPDATE_GOLDEN=1` rewrites the file instead.
fn golden(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing {}; run with UPDATE_GOLDEN=1", path.display()));
    assert_eq!(
        expected, actual,
        "{name} differs from the golden file; rerun with UPDATE_GOLDEN=1 if the change is intended"
    );
}

#[test]
fn top_level_help_lists_the_sdks_command() {
    let output = dexpa(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("sdks"));
}

#[test]
fn analysis_options_are_rejected_before_the_sdks_command() {
    // `--fail-on` would otherwise be accepted and silently ignored: `sdks` never fails on findings.
    let output = dexpa(&["--fail-on", "high", "sdks", &sdk_fixture("positive")]);
    assert_eq!(output.status.code(), Some(2), "a usage error, not a report");
    assert!(stdout(&output).is_empty());
}

#[test]
fn sdks_help_lists_its_options() {
    let output = dexpa(&["sdks", "--help"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let help = stdout(&output);
    for option in ["--catalogue", "--exodus", "--format", "diff"] {
        assert!(help.contains(option), "sdks --help lacks {option}");
    }
}

#[test]
fn plain_file_argument_still_runs_the_analysis() {
    // `dexpa <FILE>` must not be taken for a subcommand.
    let output = dexpa(&[&sdk_fixture("positive"), "--format", "json"]);
    let report = json(&output);
    assert_eq!(report["schema_version"], 1);
    assert!(report["dex_files"].is_array());
}

#[test]
fn positive_dex_finds_exactly_the_defined_sdks() {
    let report = json(&dexpa(&[
        "sdks",
        &sdk_fixture("positive"),
        "--format",
        "json",
    ]));
    // From fixtures-src: FirebaseAnalytics, Sentry (two classes), MobileAds. Facebook is only
    // referenced (a classpath stub), so it must not appear.
    assert_eq!(
        ids(&report),
        [
            "firebase-analytics",
            "google-mobile-ads",
            "google-play-services",
            "sentry"
        ]
    );
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["class_count"], 5);
    assert_eq!(report["package"], Value::Null, "a bare DEX has no manifest");
    golden(
        "sdks-positive-dex.json",
        &serde_json::to_string_pretty(&report).unwrap(),
    );
}

#[test]
fn negative_dex_finds_nothing() {
    let report = json(&dexpa(&[
        "sdks",
        &sdk_fixture("negative"),
        "--format",
        "json",
    ]));
    assert_eq!(ids(&report), Vec::<String>::new());
    assert_eq!(report["class_count"], 4);
}

#[test]
fn apk_adds_manifest_evidence() {
    let apk = write_apk("v2.apk", 2, "positive");
    let report = json(&dexpa(&["sdks", &apk, "--format", "json"]));
    assert_eq!(report["package"], "app.fixture");
    assert_eq!(report["version_code"], "2");
    assert_eq!(report["version_name"], "1.2");
    // Firebase messaging has no classes in the DEX: the manifest component alone finds it.
    assert!(ids(&report).contains(&"firebase-messaging".to_string()));
    golden(
        "sdks-positive-apk.json",
        &serde_json::to_string_pretty(&report).unwrap(),
    );
}

#[test]
fn apk_without_a_manifest_is_read_as_dex_only() {
    let report = json(&dexpa(&[
        "sdks",
        &core_fixture("multidex.apk"),
        "--format",
        "json",
    ]));
    assert_eq!(report["package"], Value::Null);
    assert_eq!(report["dex_count"], 2);
}

/// /review (2026-11-02): a manifest this parser rejects may still install on Android, so failing
/// the whole scan would hide the DEX evidence. The report keeps the DEX matches and says, in
/// `manifest_error` and on stderr, that the manifest was not read.
#[test]
fn malformed_manifest_still_reports_the_dex_and_names_the_error() {
    let apk = write_apk_with_manifest("broken.apk", b"\x03\x00\x08\x00junk".to_vec(), "positive");
    let output = dexpa(&["sdks", &apk, "--format", "json"]);
    let report = json(&output);
    assert!(
        ids(&report).contains(&"sentry".to_string()),
        "DEX evidence kept"
    );
    assert_eq!(report["package"], Value::Null);
    assert!(
        report["manifest_error"]
            .as_str()
            .is_some_and(|e| !e.is_empty()),
        "{report}"
    );
    assert!(
        stderr(&output).contains("AndroidManifest.xml"),
        "{}",
        stderr(&output)
    );
    let text = dexpa(&["sdks", &apk]);
    assert_eq!(text.status.code(), Some(0));
    assert!(
        stdout(&text).contains("AndroidManifest.xml"),
        "{}",
        stdout(&text)
    );
}

/// No app DEX entry at all (only a DEX outside the archive root) is an error, not an empty report.
#[test]
fn apk_without_an_app_dex_exits_1() {
    let dex = fs::read(sdk_fixture("positive")).unwrap();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default();
    for (entry, data) in [
        ("AndroidManifest.xml", manifest(1)),
        ("assets/plugin.dex", dex),
    ] {
        zip.start_file(entry, opts).unwrap();
        zip.write_all(&data).unwrap();
    }
    let path = scratch("nodex.apk");
    fs::write(&path, zip.finish().unwrap().into_inner()).unwrap();
    let output = dexpa(&["sdks", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("classes*.dex"),
        "{}",
        stderr(&output)
    );
}

/// /review (2026-11-02): APK strings go to the analyst's terminal. Control characters (escape
/// sequences that could rewrite or hide lines) are printed escaped in text mode. Re-review: so
/// are invisible format characters (marks, zero-width characters), which make one name look
/// like another.
#[test]
fn text_output_escapes_control_characters() {
    let root = el(
        "manifest",
        vec![
            pa(
                "package",
                s("app.fix\u{1b}[2Kture\u{202e}\u{200f}\u{200b}\u{feff}\u{61c}\u{2028}"),
            ),
            aa("versionCode", VERSION_CODE, V::Int(1)),
        ],
        vec![el("application", vec![], vec![])],
    );
    let apk = write_apk_with_manifest("escape.apk", build(&root, &Opts::default()), "positive");
    let output = dexpa(&["sdks", &apk]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(
        !text.contains('\u{1b}') && !text.contains('\u{202e}'),
        "{text:?}"
    );
    assert!(
        text.contains("app.fix\\u{1b}[2Kture\\u{202e}\\u{200f}\\u{200b}\\u{feff}\\u{61c}\\u{2028}"),
        "{text}"
    );
}

/// Re-review (2026-11-02): report JSON can come from anyone. Its text reaches stderr through
/// errors and warnings too, and is escaped there as on stdout.
#[test]
fn error_output_escapes_report_text() {
    let apk = write_apk("errtext.apk", 2, "positive");
    let mut a = json(&dexpa(&["sdks", &apk, "--format", "json"]));
    let mut b = a.clone();
    a["catalogue_fingerprint"] = Value::from("x\u{1b}]0;title\u{7}");
    b["catalogue_fingerprint"] = Value::from("y\u{202e}");
    let (pa_, pb) = (scratch("errtext-a.json"), scratch("errtext-b.json"));
    fs::write(&pa_, a.to_string()).unwrap();
    fs::write(&pb, b.to_string()).unwrap();
    let output = dexpa(&["sdks", "diff", pa_.to_str().unwrap(), pb.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(
        !err.contains('\u{1b}') && !err.contains('\u{7}') && !err.contains('\u{202e}'),
        "{err:?}"
    );
    assert!(err.contains("x\\u{1b}]0;title"), "{err}");
}

/// /review (2026-11-02): reports made with different catalogues cannot be diffed: every SDK the
/// other catalogue lacks would show up as added or removed. Different packages only warn.
#[test]
fn diff_refuses_reports_from_different_catalogues() {
    let toml = scratch("only-sentry.toml");
    fs::write(
        &toml,
        "[[sdk]]\nid = \"sentry\"\nname = \"Sentry\"\ncategory = \"crash\"\npackages = [\"io.sentry\"]\n",
    )
    .unwrap();
    let apk = write_apk("cat.apk", 2, "positive");
    let bundled = json(&dexpa(&["sdks", &apk, "--format", "json"]));
    let custom = json(&dexpa(&[
        "sdks",
        &apk,
        "--catalogue",
        toml.to_str().unwrap(),
        "--format",
        "json",
    ]));
    let (a, b) = (scratch("cat-bundled.json"), scratch("cat-custom.json"));
    fs::write(&a, bundled.to_string()).unwrap();
    fs::write(&b, custom.to_string()).unwrap();
    let output = dexpa(&["sdks", "diff", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1), "stdout: {}", stdout(&output));
    assert!(stderr(&output).contains("catalogue"), "{}", stderr(&output));

    // Another package: the diff runs, with a warning.
    let mut other = bundled.clone();
    other["package"] = Value::from("other.app");
    fs::write(&b, other.to_string()).unwrap();
    let output = dexpa(&["sdks", "diff", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("package"), "{}", stderr(&output));

    // Reports without a fingerprint (made before it existed): the diff runs, with a warning.
    let mut old = bundled.clone();
    old.as_object_mut().unwrap().remove("catalogue_fingerprint");
    fs::write(&b, old.to_string()).unwrap();
    let output = dexpa(&["sdks", "diff", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("catalogue"), "{}", stderr(&output));
}

/// /review (2026-11-02): an SDK seen only in the manifest disappears from a report whose
/// manifest did not parse. The diff says so on stderr, and the removal is `uncertain`.
#[test]
fn diff_warns_when_a_manifest_was_not_read() {
    let good = json(&dexpa(&[
        "sdks",
        &write_apk("read.apk", 2, "positive"),
        "--format",
        "json",
    ]));
    let broken_apk =
        write_apk_with_manifest("unread.apk", b"\x03\x00\x08\x00junk".to_vec(), "positive");
    let broken = json(&dexpa(&["sdks", &broken_apk, "--format", "json"]));
    let (a, b) = (scratch("read.json"), scratch("unread.json"));
    fs::write(&a, good.to_string()).unwrap();
    fs::write(&b, broken.to_string()).unwrap();
    let output = dexpa(&[
        "sdks",
        "diff",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("manifest was not read"),
        "{}",
        stderr(&output)
    );
    let events = json(&output)["events"].as_array().unwrap().clone();
    assert!(
        events.iter().all(|e| e["confidence"] != "strong"),
        "{events:?}"
    );
}

#[test]
fn text_format_names_each_sdk_and_its_evidence() {
    let apk = write_apk("text.apk", 2, "positive");
    let output = dexpa(&["sdks", &apk]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("app.fixture"));
    for line in [
        "firebase-messaging",
        "manifest_component:com.google.firebase.messaging",
        "sentry",
        "manifest_meta:io.sentry.dsn",
    ] {
        assert!(text.contains(line), "text output lacks {line}:\n{text}");
    }
}

#[test]
fn diff_reports_added_sdks_with_confidence() {
    let v1 = write_apk("diff-v1.apk", 1, "negative");
    let v2 = write_apk("diff-v2.apk", 2, "positive");
    let (r1, r2) = (scratch("diff-v1.json"), scratch("diff-v2.json"));
    for (apk, out) in [(&v1, &r1), (&v2, &r2)] {
        let output = dexpa(&["sdks", apk, "--format", "json"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        fs::write(out, &output.stdout).unwrap();
    }
    let diff = json(&dexpa(&[
        "sdks",
        "diff",
        r1.to_str().unwrap(),
        r2.to_str().unwrap(),
        "--format",
        "json",
    ]));
    assert_eq!(diff["schema_version"], 1);
    assert_eq!(diff["old"]["version_code"], "1");
    assert_eq!(diff["new"]["version_code"], "2");
    let events = diff["events"].as_array().unwrap();
    let find = |id: &str| {
        events
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("no event for {id}: {events:?}"))
    };
    // Manifest evidence on the new side: strong. Class names only: uncertain.
    assert_eq!(find("firebase-messaging")["event"], "sdk_added");
    assert_eq!(find("firebase-messaging")["confidence"], "strong");
    assert_eq!(find("sentry")["confidence"], "strong");
    assert_eq!(find("google-mobile-ads")["confidence"], "uncertain");
    // Play Services was in v1 through the manifest; v2 adds class evidence.
    assert_eq!(
        find("google-play-services")["event"],
        "sdk_evidence_changed"
    );
    golden(
        "sdks-diff.json",
        &serde_json::to_string_pretty(&diff).unwrap(),
    );

    let text = dexpa(&["sdks", "diff", r1.to_str().unwrap(), r2.to_str().unwrap()]);
    assert_eq!(text.status.code(), Some(0));
    assert!(stdout(&text).contains("+ firebase-messaging"));
}

#[test]
fn custom_catalogue_replaces_the_bundled_one() {
    let toml = scratch("one.toml");
    fs::write(
        &toml,
        "[[sdk]]\nid = \"only-sentry\"\nname = \"Sentry\"\ncategory = \"crash\"\npackages = [\"io.sentry\"]\n",
    )
    .unwrap();
    let report = json(&dexpa(&[
        "sdks",
        &sdk_fixture("positive"),
        "--catalogue",
        toml.to_str().unwrap(),
        "--format",
        "json",
    ]));
    assert_eq!(report["catalogue_size"], 1);
    assert_eq!(ids(&report), ["only-sentry"]);
    assert_eq!(report["attribution"], Value::Null);
}

#[test]
fn exodus_file_adds_trackers_and_attribution() {
    // A hand-written file in the Exodus shape; no Exodus data is committed (ODbL).
    let exodus = scratch("exodus.json");
    fs::write(
        &exodus,
        r#"{"trackers": {
            "1": {"name": "Sentry", "code_signature": "io.sentry.", "categories": ["Crash reporting"]},
            "2": {"name": "Broken", "code_signature": "(", "categories": []}
        }}"#,
    )
    .unwrap();
    let report = json(&dexpa(&[
        "sdks",
        &sdk_fixture("positive"),
        "--exodus",
        exodus.to_str().unwrap(),
        "--format",
        "json",
    ]));
    assert!(ids(&report).contains(&"exodus:1".to_string()));
    assert!(ids(&report).contains(&"sentry".to_string()), "bundled kept");
    assert_eq!(report["skipped_signatures"], 1);
    assert!(
        report["attribution"]
            .as_str()
            .is_some_and(|a| a.contains("ODbL"))
    );
}

#[test]
fn missing_input_exits_1() {
    let output = dexpa(&["sdks", "/nonexistent/app.apk"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("/nonexistent/app.apk"));
}

#[test]
fn bad_catalogue_exits_1() {
    let toml = scratch("bad.toml");
    fs::write(&toml, "[[sdk]]\nid = \"x\"\n").unwrap();
    let output = dexpa(&[
        "sdks",
        &sdk_fixture("positive"),
        "--catalogue",
        toml.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("catalogue"), "{}", stderr(&output));
}

#[test]
fn diff_rejects_other_json_and_other_schema_versions() {
    let other = scratch("not-a-report.json");
    fs::write(&other, "{\"hello\": 1}").unwrap();
    let output = dexpa(&[
        "sdks",
        "diff",
        other.to_str().unwrap(),
        other.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(1));

    let report = json(&dexpa(&[
        "sdks",
        &sdk_fixture("negative"),
        "--format",
        "json",
    ]));
    let mut future = report.clone();
    future["schema_version"] = Value::from(2);
    let (a, b) = (scratch("v1-report.json"), scratch("v2-schema.json"));
    fs::write(&a, report.to_string()).unwrap();
    fs::write(&b, future.to_string()).unwrap();
    let output = dexpa(&["sdks", "diff", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("schema_version"),
        "{}",
        stderr(&output)
    );
}

/// The committed sample APK is exactly what these fixtures build. `UPDATE_GOLDEN=1` rewrites it.
#[test]
fn sample_app_is_current() {
    let built = apk_bytes(manifest(2), "positive");
    let path = sample_app_path();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&path, &built).unwrap();
        return;
    }
    let committed = fs::read(&path)
        .unwrap_or_else(|_| panic!("missing {}; run with UPDATE_GOLDEN=1", path.display()));
    assert!(
        committed == built,
        "sample-app.apk is out of date; rerun with UPDATE_GOLDEN=1 if the change is intended"
    );
}

#[test]
fn sample_app_reports_the_golden() {
    let report = json(&dexpa(&[
        "sdks",
        sample_app_path().to_str().unwrap(),
        "--format",
        "json",
    ]));
    golden(
        "sdks-positive-apk.json",
        &serde_json::to_string_pretty(&report).unwrap(),
    );
}
