// Modified by Zero (2026-11-01): new module (fork M3); Source::as_str (M5). See NOTICE.
//! Matching a catalogue against an inventory and a manifest, with the evidence for each match.

use dexpa_core::manifest::Manifest;
use serde::{Deserialize, Serialize};

use crate::catalogue::{Catalogue, Origin, Rule};
use crate::inventory::Inventory;

/// What kind of evidence supports a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Classes the app defines.
    DexClass,
    /// A component declared in the manifest. R8 keeps these names, since Android instantiates
    /// them by name, so this evidence survives minification.
    ManifestComponent,
    /// A `<meta-data>` key in the manifest.
    ManifestMeta,
}

impl Source {
    /// The name used in JSON and in `source:rule` labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Source::DexClass => "dex_class",
            Source::ManifestComponent => "manifest_component",
            Source::ManifestMeta => "manifest_meta",
        }
    }
}

/// One piece of evidence: everything one rule matched from one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// The source.
    pub source: Source,
    /// The rule that matched: a package prefix, `signature`, or the meta-data key.
    pub rule: String,
    /// How many items matched.
    pub count: usize,
    /// Up to [`MAX_EXAMPLES`] matched names, sorted.
    pub examples: Vec<String>,
    /// DEX files the matched classes are in (empty for manifest evidence).
    pub dex_indexes: Vec<usize>,
}

/// How many example names an [`Evidence`] keeps.
pub const MAX_EXAMPLES: usize = 3;

/// One SDK found, with its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkMatch {
    /// Catalogue id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Category.
    pub category: String,
    /// Which catalogue.
    pub origin: Origin,
    /// Sorted by source, then rule.
    pub evidence: Vec<Evidence>,
}

/// Does the class `name` lie in package `prefix` or a sub-package? `com.foo` covers
/// `com.foo.A` and `com.foo.sub.B`, but not `com.foobar.C`, and not `com.foo$D` (an inner class
/// of the class `com.foo`, which is in package `com`).
fn under(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('.'))
}

struct Collect {
    count: usize,
    examples: Vec<String>,
    dex_indexes: Vec<usize>,
}

impl Collect {
    fn new() -> Self {
        Self {
            count: 0,
            examples: Vec::new(),
            dex_indexes: Vec::new(),
        }
    }

    fn add(&mut self, name: &str, dex_index: Option<usize>) {
        self.count += 1;
        if self.examples.len() < MAX_EXAMPLES && !self.examples.iter().any(|e| e == name) {
            self.examples.push(name.to_string());
        }
        if let Some(d) = dex_index
            && !self.dex_indexes.contains(&d)
        {
            self.dex_indexes.push(d);
        }
    }

    fn into_evidence(mut self, source: Source, rule: &str) -> Option<Evidence> {
        if self.count == 0 {
            return None;
        }
        self.examples.sort();
        self.dex_indexes.sort_unstable();
        Some(Evidence {
            source,
            rule: rule.to_string(),
            count: self.count,
            examples: self.examples,
            dex_indexes: self.dex_indexes,
        })
    }
}

/// Matches every SDK in `catalogue` against the classes in `inventory` and, if given, the
/// components and meta-data of `manifest`. Matches are sorted by id; overlapping SDKs (a
/// library inside a larger one) are all reported.
pub fn match_sdks(
    catalogue: &Catalogue,
    inventory: &Inventory,
    manifest: Option<&Manifest>,
) -> Vec<SdkMatch> {
    let classes = &inventory.classes; // sorted by name
    let components: Vec<String> = manifest
        .map(|m| {
            m.components
                .iter()
                .filter_map(|c| c.qualified_name(m.package.as_deref()))
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for sdk in &catalogue.sdks {
        let mut evidence = Vec::new();
        match &sdk.rule {
            Rule::Packages(prefixes) => {
                for prefix in prefixes {
                    let mut dex = Collect::new();
                    // Every class under `prefix` sorts at or after `prefix` itself.
                    let start = classes.partition_point(|c| c.name.as_str() < prefix.as_str());
                    for c in classes[start..]
                        .iter()
                        .take_while(|c| c.name.starts_with(prefix.as_str()))
                    {
                        if under(&c.name, prefix) {
                            dex.add(&c.name, Some(c.dex_index));
                        }
                    }
                    evidence.extend(dex.into_evidence(Source::DexClass, prefix));
                    let mut man = Collect::new();
                    for name in components.iter().filter(|n| under(n, prefix)) {
                        man.add(name, None);
                    }
                    evidence.extend(man.into_evidence(Source::ManifestComponent, prefix));
                }
            }
            Rule::Signature(re) => {
                let mut dex = Collect::new();
                for c in classes.iter().filter(|c| re.is_match(&c.name)) {
                    dex.add(&c.name, Some(c.dex_index));
                }
                evidence.extend(dex.into_evidence(Source::DexClass, "signature"));
                let mut man = Collect::new();
                for name in components.iter().filter(|n| re.is_match(n)) {
                    man.add(name, None);
                }
                evidence.extend(man.into_evidence(Source::ManifestComponent, "signature"));
            }
        }
        if let Some(m) = manifest {
            for key in &sdk.meta_data_keys {
                let mut meta = Collect::new();
                for _ in m
                    .meta_data
                    .iter()
                    .filter(|d| d.name.as_deref() == Some(key.as_str()))
                {
                    meta.add(key, None);
                }
                evidence.extend(meta.into_evidence(Source::ManifestMeta, key));
            }
        }
        if !evidence.is_empty() {
            evidence.sort_by(|a, b| (a.source, &a.rule).cmp(&(b.source, &b.rule)));
            out.push(SdkMatch {
                id: sdk.id.clone(),
                name: sdk.name.clone(),
                category: sdk.category.clone(),
                origin: sdk.origin,
                evidence,
            });
        }
    }
    out
}
