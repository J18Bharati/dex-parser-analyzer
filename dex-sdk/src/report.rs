// Modified by Zero (2026-11-01): new module (fork M4). See NOTICE.
//! The SDK report: what `dexpa sdks` prints as JSON, and what `diff` reads back.

use serde::{Deserialize, Serialize};

use crate::matcher::SdkMatch;

/// Version of the report's JSON shape. Independent of the main `dexpa` report's version.
pub const SCHEMA_VERSION: u32 = 1;

/// Attribution required when Exodus data contributed to a report (ODbL 1.0).
pub const EXODUS_ATTRIBUTION: &str = "Contains information from the Exodus Privacy tracker database, which is made available under the ODbL 1.0 (https://opendatacommons.org/licenses/odbl/1-0/).";

/// One app's SDK report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkReport {
    /// [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Manifest `package`, when the input was an APK.
    pub package: Option<String>,
    /// Manifest `versionCode`, as text.
    pub version_code: Option<String>,
    /// Manifest `versionName`, as text (`@0x7f...` when it is a reference).
    pub version_name: Option<String>,
    /// DEX files read.
    pub dex_count: usize,
    /// Classes the app defines.
    pub class_count: usize,
    /// SDK definitions consulted.
    pub catalogue_size: usize,
    /// Exodus signatures that could not be used.
    pub skipped_signatures: usize,
    /// Present when Exodus data was loaded.
    pub attribution: Option<String>,
    /// SDKs found, sorted by id.
    pub matches: Vec<SdkMatch>,
}
