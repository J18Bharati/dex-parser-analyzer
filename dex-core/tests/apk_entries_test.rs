// Modified by Zero (2026-11-01): new test file (fork M1). See NOTICE.
//! Which zip entries count as the app's DEX files, in what order, and how much is read.
//!
//! Android loads `classes.dex`, then `classes2.dex`, `classes3.dex`, ... from the archive root.
//! Upstream sorted entry names lexically (so `classes10.dex` came before `classes2.dex`) and
//! accepted any `*.dex` anywhere in the archive (so `assets/plugin.dex` counted as app code).

use std::io::{Cursor, Write};

use dexpa_core::multidex::{read_dex_buffers_from_bytes, read_dex_buffers_from_bytes_with_limit};
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
    // Written out of order on purpose: the archive's order must not matter either.
    let apk = zip_of(&[
        ("classes10.dex", &ten),
        ("classes.dex", &one),
        ("classes2.dex", &two),
    ]);
    let got = read_dex_buffers_from_bytes(&apk).unwrap();
    assert_eq!(got.len(), 3);
    assert!(got[0] == one, "classes.dex first");
    assert!(got[1] == two, "classes2.dex second");
    assert!(got[2] == ten, "classes10.dex last, not before classes2.dex");
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
