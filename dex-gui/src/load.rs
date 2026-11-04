// Modified by Jatin Bharati (2026-11-04): new module, the GUI's pure loading logic (fork-webapp). See NOTICE.
//! The GUI's loading logic, kept free of the browser so it can be tested natively.

use dexpa_analysis::model::{AnalysisReport, Finding};
use dexpa_core::DexFile;
use dexpa_sdk::scan::{ScanError, ScanLimits};

/// The largest file the page accepts.
pub const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;
/// The most rows any table renders.
pub const MAX_ROWS: usize = 500;
/// Archive limits for the browser: 256 MiB per DEX entry, 512 MiB of DEX in total.
pub const GUI_LIMITS: ScanLimits = ScanLimits {
    entry_bytes: 256 * 1024 * 1024,
    total_bytes: 512 * 1024 * 1024,
};

/// A small APK the page can load without a file picker.
pub const SAMPLE_APK: &[u8] = include_bytes!("../../dex-sdk/tests/data/sample-app.apk");

/// One part of a result, which loads independently of the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Section<T> {
    /// Not computed yet.
    Pending,
    /// Computed.
    Ready(T),
    /// Could not be computed; the text says why.
    Unavailable(String),
}

/// One class definition in the class list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassRow {
    /// The class descriptor, such as `Lcom/example/Foo;`.
    pub descriptor: String,
    /// Methods with a definition in the class.
    pub method_count: u32,
    /// Which DEX file defines it (0 is `classes.dex`).
    pub dex_index: usize,
}

/// The class rows of one DEX file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassRows {
    /// Readable classes.
    pub rows: Vec<ClassRow>,
    /// Class definitions whose descriptor could not be read.
    pub unreadable: usize,
}

/// `Err` with a message when a file of `len` bytes is too large to load.
pub fn check_size(len: u64) -> Result<(), String> {
    if len > MAX_FILE_BYTES {
        Err(format!(
            "this file is {} MiB; the limit is {} MiB",
            len.div_ceil(1024 * 1024),
            MAX_FILE_BYTES / (1024 * 1024)
        ))
    } else {
        Ok(())
    }
}

/// The class rows of `dex`, tagged with `dex_index`.
pub fn class_rows(dex: &DexFile<'_>, dex_index: usize) -> ClassRows {
    let mut rows = Vec::with_capacity(dex.class_defs().len());
    let mut unreadable = 0;
    for class in dex.classes() {
        match class.descriptor() {
            Ok(descriptor) => rows.push(ClassRow {
                descriptor: descriptor.to_owned(),
                method_count: class
                    .methods()
                    .map_or(0, |m| u32::try_from(m.len()).unwrap_or(u32::MAX)),
                dex_index,
            }),
            Err(_) => unreadable += 1,
        }
    }
    ClassRows { rows, unreadable }
}

/// Findings from every DEX file of an app, most severe first.
#[derive(Debug, Clone)]
pub struct MergedFindings {
    /// The combined report.
    pub report: AnalysisReport,
    /// A part held `max_per_dex` findings, so it may have been cut short.
    pub capped: bool,
}

/// Combines per-DEX reports: findings sorted by severity (stable, most severe first), counts
/// summed, and `elapsed_ms` summed over the parts that have one.
pub fn merge_reports(parts: Vec<AnalysisReport>, max_per_dex: Option<usize>) -> MergedFindings {
    let capped = max_per_dex.is_some_and(|max| parts.iter().any(|p| p.findings.len() >= max));
    let mut findings: Vec<Finding> = Vec::new();
    let mut stats = dexpa_analysis::model::AnalysisStats::default();
    for part in parts {
        findings.extend(part.findings);
        stats.method_count = stats.method_count.saturating_add(part.stats.method_count);
        stats.string_count = stats.string_count.saturating_add(part.stats.string_count);
        stats.instruction_count = stats
            .instruction_count
            .saturating_add(part.stats.instruction_count);
        if let Some(ms) = part.stats.elapsed_ms {
            stats.elapsed_ms = Some(stats.elapsed_ms.unwrap_or(0).saturating_add(ms));
        }
    }
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
    MergedFindings {
        report: AnalysisReport { findings, stats },
        capped,
    }
}

/// Whether a catalogue category (possibly several, comma separated) is advertising, analytics
/// or attribution.
pub fn is_tracking_category(category: &str) -> bool {
    category
        .split(',')
        .map(str::trim)
        .any(|c| matches!(c, "ads" | "analytics" | "attribution"))
}

/// The message shown when a file cannot be read.
pub fn scan_error_text(err: &ScanError) -> String {
    match err {
        ScanError::NoDex => {
            "this archive has no DEX code (no classes.dex), for example a resource-only split"
                .to_owned()
        }
        // Its Display already starts "failed to read the APK".
        ScanError::Apk(_) => err.to_string(),
    }
}
