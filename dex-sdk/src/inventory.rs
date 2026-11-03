// Modified by Jatin Bharati (2026-11-01): new module (fork M2); DEX v041 containers refused (2026-11-02,
// re-review). See NOTICE.
//! Defined classes and package-prefix counts across an app's DEX files.

use std::collections::BTreeMap;

use dexpa_core::{DexError, DexFile, DexResult, parse_dex};
use serde::Serialize;

/// Package prefixes are counted at these depths (`com.google` is depth 2).
pub const PREFIX_DEPTHS: std::ops::RangeInclusive<usize> = 2..=4;

/// One class definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefinedClass {
    /// Dotted name, `com.example.Foo$Inner`.
    pub name: String,
    /// Which DEX file (0 is `classes.dex`).
    pub dex_index: usize,
    /// Direct plus virtual methods with a definition in this class.
    pub methods: usize,
}

/// Counts for one DEX file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DexCounts {
    /// Position in load order.
    pub dex_index: usize,
    /// `class_defs` entries.
    pub classes: usize,
    /// Methods defined by those classes.
    pub methods: usize,
}

/// Counts for one package prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrefixCount {
    /// `com.google.firebase`
    pub prefix: String,
    /// Number of package segments in `prefix`.
    pub depth: usize,
    /// Classes defined under it (in any sub-package).
    pub classes: usize,
    /// Methods defined by those classes.
    pub methods: usize,
}

/// What an app defines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inventory {
    /// Per DEX file, in load order.
    pub dexes: Vec<DexCounts>,
    /// Every defined class, sorted by name then DEX index. A class defined in two DEX files is
    /// listed twice.
    pub classes: Vec<DefinedClass>,
    /// Class definitions whose descriptor could not be read.
    pub unreadable_classes: usize,
    /// Prefix counts at each depth in [`PREFIX_DEPTHS`], sorted by depth then prefix.
    pub prefixes: Vec<PrefixCount>,
}

/// `Lcom/foo/Bar$Inner;` to `com.foo.Bar$Inner`. Array descriptors give their element class;
/// primitives and malformed descriptors give `None`.
pub fn descriptor_to_name(descriptor: &str) -> Option<String> {
    let body = descriptor
        .trim_start_matches('[')
        .strip_prefix('L')?
        .strip_suffix(';')?;
    if body.is_empty() {
        return None;
    }
    Some(body.replace('/', "."))
}

/// The package prefixes of a dotted class name at each depth in [`PREFIX_DEPTHS`] that its
/// package reaches. `com.google.firebase.analytics.Foo` gives `com.google`,
/// `com.google.firebase` and `com.google.firebase.analytics`.
fn prefixes(name: &str) -> impl Iterator<Item = (usize, &str)> {
    // The n-th dot ends the depth-n prefix; past the last dot there is none.
    PREFIX_DEPTHS.filter_map(move |depth| {
        let end = name.match_indices('.').nth(depth - 1)?.0;
        Some((depth, &name[..end]))
    })
}

/// Builds the inventory from parsed DEX files in load order. One pass over each file's
/// `class_defs`; no cross-file index is built.
pub fn inventory(dexes: &[DexFile<'_>]) -> Inventory {
    let mut dex_counts = Vec::with_capacity(dexes.len());
    let mut classes = Vec::new();
    let mut unreadable_classes = 0;
    for (dex_index, dex) in dexes.iter().enumerate() {
        let mut counts = DexCounts {
            dex_index,
            classes: 0,
            methods: 0,
        };
        for class in dex.classes() {
            counts.classes += 1;
            let methods = class.methods().map_or(0, |m| m.len());
            counts.methods += methods;
            match class.descriptor().ok().and_then(descriptor_to_name) {
                Some(name) => classes.push(DefinedClass {
                    name,
                    dex_index,
                    methods,
                }),
                None => unreadable_classes += 1,
            }
        }
        dex_counts.push(counts);
    }
    classes.sort_by(|a, b| (&a.name, a.dex_index).cmp(&(&b.name, b.dex_index)));

    let mut by_prefix: BTreeMap<(usize, &str), (usize, usize)> = BTreeMap::new();
    for class in &classes {
        for key in prefixes(&class.name) {
            let entry = by_prefix.entry(key).or_default();
            entry.0 += 1;
            entry.1 += class.methods;
        }
    }
    let prefixes = by_prefix
        .into_iter()
        .map(|((depth, prefix), (classes, methods))| PrefixCount {
            prefix: prefix.to_string(),
            depth,
            classes,
            methods,
        })
        .collect();
    Inventory {
        dexes: dex_counts,
        classes,
        unreadable_classes,
        prefixes,
    }
}

/// Parses each buffer and builds the inventory.
pub fn inventory_from_buffers(buffers: &[Vec<u8>]) -> DexResult<Inventory> {
    let dexes = buffers
        .iter()
        .map(|b| parse_dex(b))
        .collect::<DexResult<Vec<_>>>()?;
    // A v041 file can hold several DEX files back to back; only the first is parsed, so the
    // rest of the app's classes would be missing without a word.
    for (dex, buffer) in dexes.iter().zip(buffers) {
        let header = dex.header();
        if header.version >= 41 && (header.file_size as usize) < buffer.len() {
            return Err(DexError::Message(
                "a DEX v041 container holding more than one DEX file is not supported",
            ));
        }
    }
    Ok(inventory(&dexes))
}

#[cfg(test)]
mod tests {
    use super::prefixes;

    fn all(name: &str) -> Vec<(usize, &str)> {
        prefixes(name).collect()
    }

    #[test]
    fn prefixes_stop_at_the_package_and_at_depth_four() {
        assert_eq!(
            all("com.google.firebase.analytics.internal.Foo"),
            vec![
                (2, "com.google"),
                (3, "com.google.firebase"),
                (4, "com.google.firebase.analytics")
            ]
        );
        assert_eq!(
            all("com.google.firebase.Foo"),
            vec![(2, "com.google"), (3, "com.google.firebase")]
        );
        assert_eq!(all("com.foo.Bar$Inner"), vec![(2, "com.foo")]);
        assert_eq!(
            all("a.B"),
            vec![],
            "depth-1 packages are below the first counted depth"
        );
        assert_eq!(all("Top"), vec![]);
    }
}
