// Modified by Jatin Bharati (2026-11-04): new module, moved from dex-cli's `scan` (fork-webapp). See NOTICE.
//! Reading a `.dex` or `.apk` into DEX buffers and a manifest, shared by the CLI and the GUI.
//!
//! An APK without `AndroidManifest.xml` is read as DEX only. A manifest that is present but does
//! not parse is not silently skipped: Android may still install the app, so the DEX evidence is
//! kept and the error is recorded in [`AppInput::manifest_error`] (and then in
//! [`SdkReport::manifest_error`]).

use dexpa_core::{
    DexError, DexResult,
    manifest::{Manifest, read_manifest_from_apk_bytes_with_limit},
    multidex::{MAX_DEX_ENTRY_BYTES, MAX_TOTAL_DEX_BYTES, read_dex_buffers_from_bytes_with_limits},
};

use crate::{
    catalogue::Catalogue,
    report::{SdkReport, build_report},
};

/// Size limits, in bytes, for reading an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanLimits {
    /// The largest inflated size of one DEX entry (and of the manifest).
    pub entry_bytes: u64,
    /// The largest sum of all DEX entries.
    pub total_bytes: u64,
}

impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            entry_bytes: MAX_DEX_ENTRY_BYTES,
            total_bytes: MAX_TOTAL_DEX_BYTES,
        }
    }
}

/// What was read from a file: its DEX buffers and, for an APK, the manifest.
#[derive(Debug)]
pub struct AppInput {
    /// One buffer per DEX file.
    pub buffers: Vec<Vec<u8>>,
    /// The parsed manifest; `None` for a bare DEX, an APK without one, or an unreadable one.
    pub manifest: Option<Manifest>,
    /// Why a manifest that is present could not be read.
    pub manifest_error: Option<String>,
}

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// A zip archive with no `classes*.dex` entry (for example a resource-only split).
    #[error("contains no classes*.dex entry")]
    NoDex,
    /// The archive could not be read.
    #[error("failed to read the APK: {0}")]
    Apk(#[source] DexError),
}

/// Reads `bytes` as an APK (a zip archive) or as a bare DEX file.
pub fn read_input(bytes: &[u8], limits: ScanLimits) -> Result<AppInput, ScanError> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return Ok(AppInput {
            buffers: vec![bytes.to_vec()],
            manifest: None,
            manifest_error: None,
        });
    }
    let buffers =
        read_dex_buffers_from_bytes_with_limits(bytes, limits.entry_bytes, limits.total_bytes)
            .map_err(ScanError::Apk)?;
    if buffers.is_empty() {
        return Err(ScanError::NoDex);
    }
    let (manifest, manifest_error) =
        match read_manifest_from_apk_bytes_with_limit(bytes, limits.entry_bytes) {
            Ok(m) => (Some(m), None),
            Err(DexError::Zip(zip::result::ZipError::FileNotFound)) => (None, None),
            Err(err) => (None, Some(err.to_string())),
        };
    Ok(AppInput {
        buffers,
        manifest,
        manifest_error,
    })
}

/// The SDK report for `input`, carrying over its manifest error.
pub fn report_for(catalogue: &Catalogue, input: &AppInput) -> DexResult<SdkReport> {
    let mut report = build_report(catalogue, &input.buffers, input.manifest.as_ref())?;
    report.manifest_error = input.manifest_error.clone();
    Ok(report)
}
