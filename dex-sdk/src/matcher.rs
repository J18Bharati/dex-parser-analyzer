// Modified by Zero (2026-11-01): new module (fork M3); Source::as_str (M5); one RegexSet pass
// for Exodus signatures (2026-11-02, /review). See NOTICE.
//! Matching a catalogue against an inventory and a manifest, with the evidence for each match.

use dexpa_core::manifest::Manifest;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use regex::{Regex, RegexSet};

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

#[derive(Default)]
struct Collect {
    count: usize,
    examples: Vec<String>,
    dex_indexes: Vec<usize>,
}

impl Collect {
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
    let mut signatures = signature_hits(catalogue, classes, &components);
    let mut out = Vec::new();
    for (sdk_index, sdk) in catalogue.sdks.iter().enumerate() {
        let mut evidence = Vec::new();
        match &sdk.rule {
            Rule::Packages(prefixes) => {
                for prefix in prefixes {
                    let mut dex = Collect::default();
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
                    let mut man = Collect::default();
                    for name in components.iter().filter(|n| under(n, prefix)) {
                        man.add(name, None);
                    }
                    evidence.extend(man.into_evidence(Source::ManifestComponent, prefix));
                }
            }
            Rule::Signature(_) => {
                let (dex, man) = signatures.remove(&sdk_index).unwrap_or_default();
                evidence.extend(dex.into_evidence(Source::DexClass, "signature"));
                evidence.extend(man.into_evidence(Source::ManifestComponent, "signature"));
            }
        }
        if let Some(m) = manifest {
            for key in &sdk.meta_data_keys {
                let mut meta = Collect::default();
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

/// Class and component hits for every signature rule, keyed by SDK index. One `RegexSet` pass
/// per name instead of one regex per signature per name: with about 430 Exodus signatures and
/// 100k classes that is 100k passes, not 43 million `is_match` calls. If the set is too large
/// to compile, each regex runs on its own, with the same result.
fn signature_hits(
    catalogue: &Catalogue,
    classes: &[crate::inventory::DefinedClass],
    components: &[String],
) -> HashMap<usize, (Collect, Collect)> {
    let rules: Vec<(usize, &Regex)> = catalogue
        .sdks
        .iter()
        .enumerate()
        .filter_map(|(i, sdk)| match &sdk.rule {
            Rule::Signature(re) => Some((i, re)),
            Rule::Packages(_) => None,
        })
        .collect();
    let mut hits: HashMap<usize, (Collect, Collect)> = HashMap::new();
    if rules.is_empty() {
        return hits;
    }
    let set = RegexSet::new(rules.iter().map(|(_, re)| re.as_str())).ok();
    let matched = |name: &str| -> Vec<usize> {
        match &set {
            Some(set) => set.matches(name).into_iter().map(|k| rules[k].0).collect(),
            None => rules
                .iter()
                .filter(|(_, re)| re.is_match(name))
                .map(|(i, _)| *i)
                .collect(),
        }
    };
    for c in classes {
        for i in matched(&c.name) {
            hits.entry(i).or_default().0.add(&c.name, Some(c.dex_index));
        }
    }
    for name in components {
        for i in matched(name) {
            hits.entry(i).or_default().1.add(name, None);
        }
    }
    hits
}
