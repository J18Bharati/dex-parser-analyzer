// Modified by Zero (2026-11-01): new module (fork M1b); review fixes (2026-11-02). See NOTICE.
//! The parts of `AndroidManifest.xml` that identify an app and the SDKs it declares.

use serde::Serialize;

use std::{
    fs::File,
    io::{Cursor, Read, Seek},
    path::Path,
};

use zip::{ZipArchive, ZipReadOptions};

use crate::axml::{XmlElement, XmlValue, parse_axml};
use crate::error::{DexError, DexResult};

/// The Android namespace URI.
pub const ANDROID_NS: &str = "http://schemas.android.com/apk/res/android";

/// Framework resource ids of the `android:` attributes the model reads (`android.R.attr`).
pub mod attr {
    /// `android:name`
    pub const NAME: u32 = 0x0101_0003;
    /// `android:exported`
    pub const EXPORTED: u32 = 0x0101_0010;
    /// `android:authorities`
    pub const AUTHORITIES: u32 = 0x0101_0018;
    /// `android:value`
    pub const VALUE: u32 = 0x0101_0024;
    /// `android:resource`
    pub const RESOURCE: u32 = 0x0101_0025;
    /// `android:minSdkVersion`
    pub const MIN_SDK_VERSION: u32 = 0x0101_020c;
    /// `android:versionCode`
    pub const VERSION_CODE: u32 = 0x0101_021b;
    /// `android:versionName`
    pub const VERSION_NAME: u32 = 0x0101_021c;
    /// `android:targetSdkVersion`
    pub const TARGET_SDK_VERSION: u32 = 0x0101_0270;
}

/// Default limit on the size of `AndroidManifest.xml` read from an archive: 16 MiB.
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// Kinds of manifest component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentKind {
    /// `<activity>`
    Activity,
    /// `<activity-alias>`
    ActivityAlias,
    /// `<service>`
    Service,
    /// `<receiver>`
    Receiver,
    /// `<provider>`
    Provider,
}

/// A declared component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Component {
    /// Which kind.
    pub kind: ComponentKind,
    /// `android:name` as written (may be relative, like `.Main`).
    pub name: Option<String>,
    /// `android:exported`, if set.
    pub exported: Option<bool>,
    /// `android:authorities` (providers).
    pub authorities: Option<String>,
}

impl Component {
    /// The fully qualified class name, resolved the way Android resolves it: a name starting
    /// with `.`, or with no `.` at all, is relative to the manifest's package.
    pub fn qualified_name(&self, package: Option<&str>) -> Option<String> {
        qualify(self.name.as_deref()?, package)
    }
}

/// A `<meta-data>` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetaData {
    /// The `android:name` of the enclosing component, or `None` for `<application>`.
    pub owner: Option<String>,
    /// `android:name`.
    pub name: Option<String>,
    /// `android:value`, or `android:resource` as a reference.
    pub value: Option<XmlValue>,
}

/// The manifest model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Manifest {
    /// `package`.
    pub package: Option<String>,
    /// `android:versionCode`.
    pub version_code: Option<XmlValue>,
    /// `android:versionName`.
    pub version_name: Option<XmlValue>,
    /// `<uses-sdk android:minSdkVersion>`.
    pub min_sdk: Option<XmlValue>,
    /// `<uses-sdk android:targetSdkVersion>`.
    pub target_sdk: Option<XmlValue>,
    /// `<uses-permission>` and `<uses-permission-sdk-23>` names, in order.
    pub uses_permissions: Vec<String>,
    /// Components under `<application>`, in order.
    pub components: Vec<Component>,
    /// `<meta-data>` under `<application>` and its components, in order.
    pub meta_data: Vec<MetaData>,
}

fn qualify(name: &str, package: Option<&str>) -> Option<String> {
    if let Some(rest) = name.strip_prefix('.') {
        return Some(format!("{}.{rest}", package?));
    }
    if !name.contains('.') {
        return Some(format!("{}.{name}", package?));
    }
    Some(name.to_string())
}

/// An `android:` attribute: by framework resource id where the attribute has one, else by name
/// in the Android namespace. A name emptied by resource shrinking still has its id.
fn android<'a>(e: &'a XmlElement, id: u32, name: &str) -> Option<&'a XmlValue> {
    e.attributes
        .iter()
        .find(|a| a.resource_id == Some(id))
        .or_else(|| {
            e.attributes.iter().find(|a| {
                a.resource_id.is_none()
                    && a.name == name
                    && a.namespace.as_deref() == Some(ANDROID_NS)
            })
        })
        .map(|a| &a.value)
}

fn string(v: Option<&XmlValue>) -> Option<String> {
    match v? {
        XmlValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn boolean(v: Option<&XmlValue>) -> Option<bool> {
    match v? {
        XmlValue::Bool(b) => Some(*b),
        XmlValue::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn component_kind(name: &str) -> Option<ComponentKind> {
    Some(match name {
        "activity" => ComponentKind::Activity,
        "activity-alias" => ComponentKind::ActivityAlias,
        "service" => ComponentKind::Service,
        "receiver" => ComponentKind::Receiver,
        "provider" => ComponentKind::Provider,
        _ => return None,
    })
}

fn meta_data(e: &XmlElement, owner: Option<String>) -> MetaData {
    MetaData {
        owner,
        name: string(android(e, attr::NAME, "name")),
        value: android(e, attr::VALUE, "value")
            .or_else(|| android(e, attr::RESOURCE, "resource"))
            .cloned(),
    }
}

/// Builds the model from a parsed `<manifest>` element.
pub fn manifest_from_element(root: &XmlElement) -> DexResult<Manifest> {
    if root.name != "manifest" {
        return Err(DexError::Malformed {
            context: "AndroidManifest.xml",
            message: "root element is not <manifest>",
        });
    }
    let mut m = Manifest {
        package: root
            .attributes
            .iter()
            .find(|a| a.name == "package" && a.namespace.is_none())
            .and_then(|a| string(Some(&a.value))),
        version_code: android(root, attr::VERSION_CODE, "versionCode").cloned(),
        version_name: android(root, attr::VERSION_NAME, "versionName").cloned(),
        ..Manifest::default()
    };
    for child in &root.children {
        match child.name.as_str() {
            "uses-sdk" => {
                m.min_sdk = android(child, attr::MIN_SDK_VERSION, "minSdkVersion").cloned();
                m.target_sdk =
                    android(child, attr::TARGET_SDK_VERSION, "targetSdkVersion").cloned();
            }
            "uses-permission" | "uses-permission-sdk-23" => {
                if let Some(name) = string(android(child, attr::NAME, "name")) {
                    m.uses_permissions.push(name);
                }
            }
            "application" => {
                for item in &child.children {
                    if item.name == "meta-data" {
                        m.meta_data.push(meta_data(item, None));
                    } else if let Some(kind) = component_kind(&item.name) {
                        let name = string(android(item, attr::NAME, "name"));
                        for inner in item.children.iter().filter(|c| c.name == "meta-data") {
                            m.meta_data.push(meta_data(inner, name.clone()));
                        }
                        m.components.push(Component {
                            kind,
                            name,
                            exported: boolean(android(item, attr::EXPORTED, "exported")),
                            authorities: string(android(item, attr::AUTHORITIES, "authorities")),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(m)
}

/// Parses a binary `AndroidManifest.xml`.
pub fn parse_manifest(bytes: &[u8]) -> DexResult<Manifest> {
    manifest_from_element(&parse_axml(bytes)?)
}

/// Reads and parses `AndroidManifest.xml` from an APK on disk.
pub fn read_manifest_from_apk<P: AsRef<Path>>(path: P) -> DexResult<Manifest> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    read_manifest_entry(&mut archive, MAX_MANIFEST_BYTES)
}

/// Reads and parses `AndroidManifest.xml` from an in-memory APK.
pub fn read_manifest_from_apk_bytes(bytes: &[u8]) -> DexResult<Manifest> {
    read_manifest_from_apk_bytes_with_limit(bytes, MAX_MANIFEST_BYTES)
}

/// Like [`read_manifest_from_apk_bytes`], with a size limit in bytes for the manifest entry.
pub fn read_manifest_from_apk_bytes_with_limit(bytes: &[u8], limit: u64) -> DexResult<Manifest> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    read_manifest_entry(&mut archive, limit)
}

fn read_manifest_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limit: u64,
) -> DexResult<Manifest> {
    // Header consistency also refuses duplicate names: the zip crate keeps one copy, and
    // Android or another tool might read the other.
    let file = archive.by_name_with_options(
        "AndroidManifest.xml",
        ZipReadOptions::new().enforce_header_consistency(true),
    )?;
    if file.size() > limit {
        return Err(DexError::Message(
            "AndroidManifest.xml exceeds the size limit",
        ));
    }
    let mut buf = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(DexError::Message(
            "AndroidManifest.xml exceeds the size limit",
        ));
    }
    parse_manifest(&buf)
}
