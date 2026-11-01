// Modified by Zero (2026-11-01): new module (fork M4); build_report and value_text (M5);
// catalogue_fingerprint and manifest_error (2026-11-02, /review). See NOTICE.
//! The SDK report: what `dexpa sdks` prints as JSON, and what `diff` reads back.

use dexpa_core::{DexResult, axml::XmlValue, manifest::Manifest};
use serde::{Deserialize, Serialize};

use crate::catalogue::{Catalogue, Origin};
use crate::inventory::inventory_from_buffers;
use crate::matcher::{SdkMatch, match_sdks};

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
    /// [`Catalogue::fingerprint`] of the catalogue that made this report. Absent in reports
    /// made before it existed.
    #[serde(default)]
    pub catalogue_fingerprint: Option<String>,
    /// Why `AndroidManifest.xml` could not be read, when an APK has one that does not parse.
    /// The report then holds the DEX evidence only.
    #[serde(default)]
    pub manifest_error: Option<String>,
    /// SDKs found, sorted by id.
    pub matches: Vec<SdkMatch>,
}

/// Builds the report for one app: the DEX files in load order and, for an APK, its manifest.
/// Fails only when a DEX file does not parse.
pub fn build_report(
    catalogue: &Catalogue,
    dex_buffers: &[Vec<u8>],
    manifest: Option<&Manifest>,
) -> DexResult<SdkReport> {
    let inventory = inventory_from_buffers(dex_buffers)?;
    let exodus = catalogue.sdks.iter().any(|s| s.origin == Origin::Exodus);
    Ok(SdkReport {
        schema_version: SCHEMA_VERSION,
        package: manifest.and_then(|m| m.package.clone()),
        version_code: manifest.and_then(|m| m.version_code.as_ref().and_then(value_text)),
        version_name: manifest.and_then(|m| m.version_name.as_ref().and_then(value_text)),
        dex_count: dex_buffers.len(),
        class_count: inventory.classes.len(),
        catalogue_size: catalogue.sdks.len(),
        skipped_signatures: catalogue.skipped.len(),
        attribution: exodus.then(|| EXODUS_ATTRIBUTION.to_string()),
        catalogue_fingerprint: Some(catalogue.fingerprint()),
        manifest_error: None,
        matches: match_sdks(catalogue, &inventory, manifest),
    })
}

/// A manifest value as text: numbers in decimal, references as `@0x...` (resolving them needs
/// `resources.arsc`, which is not read), other typed values with their type. `Null` gives `None`.
pub fn value_text(value: &XmlValue) -> Option<String> {
    Some(match value {
        XmlValue::Null => return None,
        XmlValue::String(s) => s.clone(),
        XmlValue::Int(n) => n.to_string(),
        XmlValue::Hex(n) => n.to_string(),
        XmlValue::Bool(b) => b.to_string(),
        XmlValue::Reference(id) => format!("@0x{id:08x}"),
        XmlValue::AttributeRef(id) => format!("?0x{id:08x}"),
        XmlValue::Other { data_type, data } => format!("type 0x{data_type:02x}: 0x{data:08x}"),
    })
}
