// Modified by Zero (2026-11-01): new test file (fork M1); review fixes (2026-11-02). See NOTICE.
//! Which zip entries count as the app's DEX files, in what order, and how much is read.
//!
//! Android loads `classes.dex`, then `classes2.dex`, `classes3.dex`, ... from the archive root.
//! Upstream sorted entry names lexically (so `classes10.dex` came before `classes2.dex`) and
//! accepted any `*.dex` anywhere in the archive (so `assets/plugin.dex` counted as app code).

use std::io::{Cursor, Write};

use dexpa_core::multidex::{
    MAX_TOTAL_DEX_BYTES, read_dex_buffers_from_bytes, read_dex_buffers_from_bytes_with_limit,
    read_dex_buffers_from_bytes_with_limits,
};
use zip::write::{SimpleFileOptions, ZipWriter};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("tests/data/{name}")).expect("fixture missing")
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn dex_entries_are_in_numeric_order() {
    let (one, two, ten) = (
        fixture("classes.dex"),
        fixture("Test.dex"),
        fixture("StringTests.dex"),
    );
    // classes.dex .. classes10.dex, written out of order on purpose: the archive's order must
    // not matter, and a text sort would put classes10.dex before classes2.dex.
    let names: Vec<String> = (3..=9).map(|n| format!("classes{n}.dex")).collect();
    let mut entries: Vec<(&str, &[u8])> = vec![("classes10.dex", &ten), ("classes.dex", &one)];
    entries.extend(names.iter().map(|n| (n.as_str(), one.as_slice())));
    entries.push(("classes2.dex", &two));
    let got = read_dex_buffers_from_bytes(&zip_of(&entries)).unwrap();
    assert_eq!(got.len(), 10);
    assert!(got[0] == one, "classes.dex first");
    assert!(got[1] == two, "classes2.dex second");
    assert!(got[9] == ten, "classes10.dex last, not before classes2.dex");
}

/// /review (2026-11-02): ART loads classes.dex, classes2.dex, ... and stops at the first missing
/// number. A classesN.dex after a gap is never loaded, so it is not app code: it must neither
/// add classes nor, when it does not parse, block the analysis of the real ones.
#[test]
fn dex_entries_stop_at_the_first_gap() {
    let one = fixture("classes.dex");
    // Other entries make the archive long enough that a loop which skipped the gap would
    // reach classes3.dex.
    let apk = zip_of(&[
        ("AndroidManifest.xml", b"not read here"),
        ("resources.arsc", b"not read here"),
        ("classes.dex", &one),
        ("classes3.dex", b"dex\n035\0not really a dex file"),
    ]);
    let got = read_dex_buffers_from_bytes(&apk).unwrap();
    assert_eq!(
        got.len(),
        1,
        "classes3.dex follows a gap: Android never loads it"
    );
    assert!(got[0] == one);
}

/// Re-review (2026-11-02): ART refuses an app whose numbered DEX entry is not a DEX file. One
/// was skipped silently, so the report undercounted and its classes were missing.
#[test]
fn a_numbered_entry_that_is_not_dex_is_an_error() {
    let one = fixture("classes.dex");
    let apk = zip_of(&[
        ("classes.dex", &one),
        ("classes2.dex", b"PK\x03\x04 not a dex file"),
        ("classes3.dex", &one),
    ]);
    assert!(read_dex_buffers_from_bytes(&apk).is_err());
}

/// /review (2026-11-02): the zip crate keeps one of two entries with the same name, and other
/// readers may keep the other one. Two `classes.dex` entries are refused, not silently merged.
#[test]
fn duplicate_entry_names_are_refused() {
    let (one, other) = (fixture("classes.dex"), fixture("Test.dex"));
    let mut apk = zip_of(&[("classes.dex", &one), ("classes.dey", &other)]);
    replace_all(&mut apk, b"classes.dey", b"classes.dex");
    assert!(read_dex_buffers_from_bytes(&apk).is_err());
}

/// Renames entries in a built archive (local headers and central directory alike), to make
/// archives that `ZipWriter` refuses to write.
fn replace_all(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut i = 0;
    while i + from.len() <= bytes.len() {
        if &bytes[i..i + from.len()] == from {
            bytes[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
}

#[test]
fn only_top_level_classes_dex_entries_count() {
    let (one, other) = (fixture("classes.dex"), fixture("FieldsTest.dex"));
    let apk = zip_of(&[
        ("classes.dex", &one),
        ("assets/plugin.dex", &other),
        ("lib/classes2.dex", &other),
        ("secondary.dex", &other),
        ("classes1.dex", &other),
        ("classes02.dex", &other),
    ]);
    let got = read_dex_buffers_from_bytes(&apk).unwrap();
    assert_eq!(got.len(), 1, "only classes.dex is app code here");
    assert!(got[0] == one);
}

#[test]
fn an_entry_over_the_limit_is_refused() {
    let one = fixture("classes.dex");
    let apk = zip_of(&[("classes.dex", &one)]);
    assert!(
        read_dex_buffers_from_bytes_with_limit(&apk, one.len() as u64).is_ok(),
        "exactly at the limit is fine"
    );
    let err = read_dex_buffers_from_bytes_with_limit(&apk, one.len() as u64 - 1);
    assert!(
        err.is_err(),
        "one byte over the limit must fail, not truncate"
    );
}

/// /cso CSO-1 (2026-11-02): each entry was capped, but not their sum, so many small deflated
/// entries could inflate to many GiB. The total is capped too.
#[test]
fn the_sum_of_entries_is_capped_as_well() {
    let one = fixture("classes.dex");
    let n = one.len() as u64;
    let apk = zip_of(&[
        ("classes.dex", &one),
        ("classes2.dex", &one),
        ("classes3.dex", &one),
    ]);
    assert_eq!(
        read_dex_buffers_from_bytes_with_limits(&apk, n, 3 * n)
            .unwrap()
            .len(),
        3,
        "exactly at the total is fine"
    );
    assert!(
        read_dex_buffers_from_bytes_with_limits(&apk, n, 3 * n - 1).is_err(),
        "one byte over the total must fail"
    );
    assert!(
        read_dex_buffers_from_bytes_with_limits(&apk, n, 2 * n).is_err(),
        "the third entry passes the per-entry limit but not the total"
    );
}

// Room for several full-size DEX files: checked at compile time.
const _: () = assert!(MAX_TOTAL_DEX_BYTES >= 4 * (1 << 28));

/// /review (2026-11-02): `u64::MAX` as a limit means "no limit". `limit + 1` overflowed on it:
/// a panic in debug builds, and in release `take(0)`, which silently dropped every entry.
#[test]
fn u64_max_limits_mean_no_limit() {
    let one = fixture("classes.dex");
    let apk = zip_of(&[("classes.dex", &one)]);
    let got = read_dex_buffers_from_bytes_with_limits(&apk, u64::MAX, u64::MAX).unwrap();
    assert_eq!(got.len(), 1);
    assert!(got[0] == one);
}
