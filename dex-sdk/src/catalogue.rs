// Modified by Zero (2026-11-01): new module (fork M3); review fixes (2026-11-02). See NOTICE.
//! SDK catalogues: this project's TOML list, and an importer for Exodus tracker files.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Errors loading a catalogue.
#[derive(Debug, thiserror::Error)]
pub enum CatalogueError {
    /// The file is not valid TOML or JSON of the expected shape.
    #[error("cannot parse catalogue: {0}")]
    Parse(String),
    /// An entry breaks a rule.
    #[error("catalogue entry {id:?}: {message}")]
    Invalid {
        /// The entry's id, if it has one.
        id: String,
        /// What is wrong.
        message: String,
    },
}

/// Where an SDK definition came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// This project's TOML catalogue.
    Catalogue,
    /// An Exodus tracker file (ODbL 1.0; never committed).
    Exodus,
}

/// How an SDK's code is recognised.
#[derive(Debug, Clone)]
pub enum Rule {
    /// Dotted package prefixes, matched on package boundaries.
    Packages(Vec<String>),
    /// An Exodus code signature, anchored at the start of the class name.
    Signature(Regex),
}

/// One SDK.
#[derive(Debug, Clone)]
pub struct Sdk {
    /// Stable id (`sentry`, `exodus:27`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Category (`analytics`, `ads`, ...).
    pub category: String,
    /// Where the definition came from.
    pub origin: Origin,
    /// Code rule.
    pub rule: Rule,
    /// Exact `<meta-data android:name>` keys that indicate the SDK.
    pub meta_data_keys: Vec<String>,
}

/// A set of SDK definitions.
#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    /// Definitions, sorted by id.
    pub sdks: Vec<Sdk>,
    /// Exodus signatures skipped because they were empty or not valid regexes.
    pub skipped: Vec<String>,
}

/// This project's catalogue, compiled in.
pub const BUNDLED_TOML: &str = include_str!("../catalogue/sdks.toml");

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlFile {
    #[serde(default)]
    sdk: Vec<TomlSdk>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlSdk {
    id: String,
    name: String,
    category: String,
    #[serde(default)]
    packages: Vec<String>,
    #[serde(default)]
    meta_data_keys: Vec<String>,
}

fn invalid(id: &str, message: &str) -> CatalogueError {
    CatalogueError::Invalid {
        id: id.to_string(),
        message: message.to_string(),
    }
}

fn valid_prefix(p: &str) -> bool {
    let segments: Vec<&str> = p.split('.').collect();
    segments.len() >= 2 && segments.iter().all(|s| !s.is_empty())
}

/// Loads a catalogue in this project's TOML format and checks its rules.
pub fn load_toml(text: &str) -> Result<Catalogue, CatalogueError> {
    let file: TomlFile = toml::from_str(text).map_err(|e| CatalogueError::Parse(e.to_string()))?;
    let mut sdks = Vec::with_capacity(file.sdk.len());
    for entry in file.sdk {
        if entry.id.trim().is_empty() {
            return Err(invalid(&entry.id, "empty id"));
        }
        if entry.packages.is_empty() && entry.meta_data_keys.is_empty() {
            return Err(invalid(
                &entry.id,
                "no packages and no meta-data keys: nothing to match",
            ));
        }
        if let Some(p) = entry.packages.iter().find(|p| !valid_prefix(p)) {
            return Err(invalid(
                &entry.id,
                &format!("package prefix {p:?} needs at least two non-empty segments"),
            ));
        }
        if entry.meta_data_keys.iter().any(|k| k.is_empty()) {
            return Err(invalid(&entry.id, "empty meta-data key"));
        }
        sdks.push(Sdk {
            id: entry.id,
            name: entry.name,
            category: entry.category,
            origin: Origin::Catalogue,
            rule: Rule::Packages(entry.packages),
            meta_data_keys: entry.meta_data_keys,
        });
    }
    Catalogue {
        sdks,
        skipped: Vec::new(),
    }
    .sorted_unique()
}

#[derive(serde::Deserialize)]
struct ExodusFile {
    trackers: std::collections::BTreeMap<String, ExodusTracker>,
}

#[derive(serde::Deserialize)]
struct ExodusTracker {
    name: String,
    #[serde(default)]
    code_signature: String,
    #[serde(default)]
    categories: Vec<String>,
}

/// Loads an Exodus tracker file (`{"trackers": {"<id>": {name, code_signature, categories}}}`).
///
/// Each `code_signature` is a regex alternation. Exodus searches it anywhere in a class name;
/// here it is anchored at the start (`^(?:...)`), so `com.foo` does not match `x.com.foo.Bar`.
/// Empty signatures and invalid regexes are skipped and listed in [`Catalogue::skipped`].
pub fn load_exodus_json(text: &str) -> Result<Catalogue, CatalogueError> {
    let file: ExodusFile =
        serde_json::from_str(text).map_err(|e| CatalogueError::Parse(e.to_string()))?;
    let mut sdks = Vec::new();
    let mut skipped = Vec::new();
    for (key, t) in file.trackers {
        let id = format!("exodus:{key}");
        if t.code_signature.trim().is_empty() {
            skipped.push(format!("{id}: empty code signature"));
            continue;
        }
        // Check the signature on its own first: wrapped unchecked, an unbalanced `a)|(b`
        // would compile as `^(?:a)|(b)`, whose second branch escapes the anchor.
        let anchored = Regex::new(&t.code_signature)
            .and_then(|_| Regex::new(&format!("^(?:{})", t.code_signature)));
        match anchored {
            Ok(re) => sdks.push(Sdk {
                id,
                name: t.name,
                category: t.categories.join(", "),
                origin: Origin::Exodus,
                rule: Rule::Signature(re),
                meta_data_keys: Vec::new(),
            }),
            Err(_) => skipped.push(format!("{id}: invalid code signature")),
        }
    }
    Catalogue { sdks, skipped }.sorted_unique()
}

/// This project's catalogue ([`BUNDLED_TOML`]).
pub fn bundled() -> Catalogue {
    load_toml(BUNDLED_TOML).expect("the bundled catalogue is checked by its tests")
}

impl Catalogue {
    fn sorted_unique(mut self) -> Result<Catalogue, CatalogueError> {
        self.sdks.sort_by(|a, b| a.id.cmp(&b.id));
        if let Some(w) = self.sdks.windows(2).find(|w| w[0].id == w[1].id) {
            return Err(invalid(&w[0].id, "duplicate id"));
        }
        Ok(self)
    }

    /// Identifies what this catalogue matches: a hash (64-bit FNV-1a, stable across builds and
    /// platforms) of every SDK's id, package prefixes or signature, and meta-data keys, in id
    /// order. Names and categories are display text and are left out. `sdks diff` uses it to
    /// refuse reports made with different catalogues.
    pub fn fingerprint(&self) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |part: &str| {
            for b in part.bytes().chain([0]) {
                h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for sdk in &self.sdks {
            feed(&sdk.id);
            match &sdk.rule {
                Rule::Packages(prefixes) => {
                    feed("packages");
                    prefixes.iter().for_each(|p| feed(p));
                }
                Rule::Signature(re) => {
                    feed("signature");
                    feed(re.as_str());
                }
            }
            feed("meta");
            sdk.meta_data_keys.iter().for_each(|k| feed(k));
            feed("\n");
        }
        format!("fnv1a64:{h:016x}")
    }

    /// Combines two catalogues. Ids must not collide.
    pub fn merge(mut self, other: Catalogue) -> Result<Catalogue, CatalogueError> {
        self.sdks.extend(other.sdks);
        self.skipped.extend(other.skipped);
        self.sorted_unique()
    }
}
