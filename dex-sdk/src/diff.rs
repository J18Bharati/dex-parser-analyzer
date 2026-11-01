// Modified by Zero (2026-11-01): new module (fork M4); uses Source::as_str (M5); unread manifests
// are uncertain, and only class evidence is compared (2026-11-02, /review). See NOTICE.
//! SDK changes between two versions of an app.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::matcher::{SdkMatch, Source};
use crate::report::SdkReport;

/// How much an event can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// The side that has the SDK has manifest evidence, which minification does not rename, and
    /// the other side's manifest was read.
    Strong,
    /// Class-name evidence only: R8 renaming can make it appear or vanish with no real change.
    /// Also when the other side's manifest was not read, so its absence there proves nothing.
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

/// `m` is on one side; `other` is the report it is missing from.
fn confidence(m: &SdkMatch, other: &SdkReport) -> Confidence {
    if manifest_read(other) && m.evidence.iter().any(|e| e.source != Source::DexClass) {
        Confidence::Strong
    } else {
        Confidence::Uncertain
    }
}

/// Whether a report's manifest evidence can be trusted to be complete: a DEX-only input has no
/// package, and a manifest that did not parse has `manifest_error`.
pub fn manifest_read(r: &SdkReport) -> bool {
    r.package.is_some() && r.manifest_error.is_none()
}

/// The `source:rule` pairs of `m`; class evidence only when `classes_only`.
fn kinds(m: &SdkMatch, classes_only: bool) -> BTreeSet<String> {
    m.evidence
        .iter()
        .filter(|e| !classes_only || e.source == Source::DexClass)
        .map(|e| format!("{}:{}", e.source.as_str(), e.rule))
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
    // With a manifest unread on either side, its evidence is missing there, not gone.
    let classes_only = !(manifest_read(old) && manifest_read(new));
    let mut events = Vec::new();
    for id in ids {
        match (before.get(id), after.get(id)) {
            (None, Some(n)) => events.push(DiffEvent::SdkAdded {
                id: id.into(),
                name: n.name.clone(),
                confidence: confidence(n, old),
            }),
            (Some(o), None) => events.push(DiffEvent::SdkRemoved {
                id: id.into(),
                name: o.name.clone(),
                confidence: confidence(o, new),
            }),
            (Some(o), Some(n)) => {
                let (ko, kn) = (kinds(o, classes_only), kinds(n, classes_only));
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
