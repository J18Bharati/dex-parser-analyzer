// Modified by Jatin Bharati (2026-11-04): new test file for `dex_gui::load` (fork-webapp). See NOTICE.
use std::borrow::Cow;

use dex_gui::load::*;
use dexpa_analysis::model::{
    AnalysisReport, AnalysisStats, Finding, Location, Severity, VulnerabilityKind,
};
use dexpa_core::parse_dex;
use dexpa_sdk::{
    catalogue,
    scan::{ScanError, read_input, report_for},
};

fn finding(severity: Severity) -> Finding {
    Finding {
        id: Cow::Borrowed("T-1"),
        kind: VulnerabilityKind::WeakCrypto,
        severity,
        location: Location::unknown(),
        message: String::new(),
        extra: serde_json::Value::Null,
        cwe: None,
        remediation: None,
    }
}

fn part(severities: &[Severity], stats: AnalysisStats) -> AnalysisReport {
    AnalysisReport {
        findings: severities.iter().copied().map(finding).collect(),
        stats,
    }
}

fn stats(methods: u32, strings: u32, instructions: u64, ms: Option<u64>) -> AnalysisStats {
    AnalysisStats {
        method_count: methods,
        string_count: strings,
        instruction_count: instructions,
        elapsed_ms: ms,
    }
}

#[test]
fn oversized_files_are_refused() {
    let err = check_size(MAX_FILE_BYTES + 1).unwrap_err();
    assert!(err.contains("200 MiB"), "{err}");
    assert!(check_size(MAX_FILE_BYTES).is_ok());
}

#[test]
fn merge_puts_a_later_critical_first() {
    let parts = vec![
        part(&[Severity::Info], AnalysisStats::default()),
        part(&[Severity::Critical], AnalysisStats::default()),
    ];
    let merged = merge_reports(parts, None);
    assert_eq!(merged.report.findings[0].severity, Severity::Critical);
    assert_eq!(merged.report.findings[1].severity, Severity::Info);
}

#[test]
fn merge_flags_the_cap() {
    let full = vec![part(&[Severity::Low; 500], AnalysisStats::default())];
    assert!(merge_reports(full, Some(500)).capped);
    let short = vec![part(&[Severity::Low; 499], AnalysisStats::default())];
    assert!(!merge_reports(short, Some(500)).capped);
    let uncapped = vec![part(&[Severity::Low; 500], AnalysisStats::default())];
    assert!(!merge_reports(uncapped, None).capped);
}

#[test]
fn merge_sums_stats() {
    let parts = vec![
        part(&[], stats(1, 2, 3, Some(10))),
        part(&[], stats(10, 20, 30, None)),
        part(&[], stats(100, 200, 300, Some(5))),
    ];
    let s = merge_reports(parts, None).report.stats;
    assert_eq!(
        (s.method_count, s.string_count, s.instruction_count),
        (111, 222, 333)
    );
    assert_eq!(s.elapsed_ms, Some(15));
    let none = merge_reports(vec![part(&[], stats(0, 0, 0, None))], None);
    assert_eq!(none.report.stats.elapsed_ms, None);
}

#[test]
fn tracking_categories() {
    for yes in [
        "ads",
        "analytics",
        "attribution",
        "social, analytics, ads",
        "push, analytics",
    ] {
        assert!(is_tracking_category(yes), "{yes}");
    }
    for no in ["push", "maps", "platform", "crash-reporting", ""] {
        assert!(!is_tracking_category(no), "{no}");
    }
}

#[test]
fn sample_apk_reads_and_reports() {
    let input = read_input(SAMPLE_APK, GUI_LIMITS).expect("sample reads");
    let report = report_for(&catalogue::bundled(), &input).expect("sample reports");
    assert_eq!(report.package.as_deref(), Some("app.fixture"));
    assert_eq!(report.version_name.as_deref(), Some("1.2"));
    assert_eq!(report.matches.len(), 5);
}

#[test]
fn class_rows_cover_every_dex() {
    let input = read_input(SAMPLE_APK, GUI_LIMITS).unwrap();
    let report = report_for(&catalogue::bundled(), &input).unwrap();
    let (mut total, mut unreadable) = (0, 0);
    for (index, buffer) in input.buffers.iter().enumerate() {
        let dex = parse_dex(buffer).unwrap();
        let rows = class_rows(&dex, index);
        assert!(rows.rows.iter().all(|r| r.dex_index == index));
        total += rows.rows.len();
        unreadable += rows.unreadable;
    }
    assert!(total > 0);
    assert_eq!(total, report.class_count);
    assert_eq!(unreadable, report.unreadable_classes);
}

#[test]
fn not_a_zip_or_dex_is_an_error_not_a_panic() {
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(b"junk junk junk");
    let err = read_input(&bytes, GUI_LIMITS).unwrap_err();
    assert!(!scan_error_text(&err).is_empty());
    assert!(!scan_error_text(&err).starts_with("failed to read the APK: failed to read"));
}

#[test]
fn no_dex_message_says_so() {
    assert!(scan_error_text(&ScanError::NoDex).contains("no DEX"));
}
