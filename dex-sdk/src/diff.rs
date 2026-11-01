// Modified by Zero (2026-11-01): new module (fork M4). See NOTICE.
//! SDK changes between two versions of an app.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::matcher::{SdkMatch, Source};
use crate::report::SdkReport;

/// How much an event can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// The side that has the SDK has manifest evidence, which minification does not rename.
    Strong,
    /// Class-name evidence only: R8 renaming can make it appear or vanish with no real change.
    Uncertain,
}

/// One change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DiffEvent {
    /// In the new version only.
    SdkAdded {
        /// SDK id.
        id: String,
        /// Display name.
        name: String,
        /// See [`Confidence`].
        confidence: Confidence,
    },
    /// In the old version only.
    SdkRemoved {
        /// SDK id.
        id: String,
        /// Display name.
        name: String,
        /// See [`Confidence`].
        confidence: Confidence,
    },
    /// In both, with different kinds of evidence.
    SdkEvidenceChanged {
        /// SDK id.
        id: String,
        /// `source:rule` pairs only the new version has.
        gained: Vec<String>,
        /// `source:rule` pairs only the old version has.
        lost: Vec<String>,
    },
}

fn confidence(m: &SdkMatch) -> Confidence {
    if m.evidence.iter().any(|e| e.source != Source::DexClass) {
        Confidence::Strong
    } else {
        Confidence::Uncertain
    }
}

fn kinds(m: &SdkMatch) -> BTreeSet<String> {
    m.evidence
        .iter()
        .map(|e| {
            let source = match e.source {
                Source::DexClass => "dex_class",
                Source::ManifestComponent => "manifest_component",
                Source::ManifestMeta => "manifest_meta",
            };
            format!("{source}:{}", e.rule)
        })
        .collect()
}

/// The SDK changes from `old` to `new`, sorted by SDK id. Only the presence of an SDK and the
/// kinds of evidence for it are compared; counts and DEX placement are not, since a rebuild
/// changes them without changing what the app contains.
pub fn diff(old: &SdkReport, new: &SdkReport) -> Vec<DiffEvent> {
    let before: BTreeMap<&str, &SdkMatch> =
        old.matches.iter().map(|m| (m.id.as_str(), m)).collect();
    let after: BTreeMap<&str, &SdkMatch> = new.matches.iter().map(|m| (m.id.as_str(), m)).collect();
    let ids: BTreeSet<&str> = before.keys().chain(after.keys()).copied().collect();
    let mut events = Vec::new();
    for id in ids {
        match (before.get(id), after.get(id)) {
            (None, Some(n)) => events.push(DiffEvent::SdkAdded {
                id: id.into(),
                name: n.name.clone(),
                confidence: confidence(n),
            }),
            (Some(o), None) => events.push(DiffEvent::SdkRemoved {
                id: id.into(),
                name: o.name.clone(),
                confidence: confidence(o),
            }),
            (Some(o), Some(n)) => {
                let (ko, kn) = (kinds(o), kinds(n));
                if ko != kn {
                    events.push(DiffEvent::SdkEvidenceChanged {
                        id: id.into(),
                        gained: kn.difference(&ko).cloned().collect(),
                        lost: ko.difference(&kn).cloned().collect(),
                    });
                }
            }
            (None, None) => {}
        }
    }
    events
}
