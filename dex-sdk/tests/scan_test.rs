// Modified by Jatin Bharati (2026-11-04): new test file (fork-webapp). See NOTICE.
//! Reading a `.dex` or `.apk` into DEX buffers and a manifest, shared by the CLI and the GUI.

use std::{
    fs,
    io::{Cursor, Write},
};

use dexpa_sdk::{
    catalogue::bundled,
    scan::{ScanError, ScanLimits, read_input, report_for},
};
use zip::write::{SimpleFileOptions, ZipWriter};

fn dex() -> Vec<u8> {
    fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/sdk-positive.dex"
    ))
    .unwrap()
}

fn zip_of(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        zip.start_file(*name, opts).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn bare_dex_has_no_manifest() {
    let input = read_input(&dex(), ScanLimits::default()).unwrap();
    assert_eq!(input.buffers.len(), 1);
    assert!(input.manifest.is_none());
    assert!(input.manifest_error.is_none());
}

#[test]
fn zip_without_dex_is_no_dex() {
    let bytes = zip_of(&[("AndroidManifest.xml", b"x".to_vec())]);
    assert!(matches!(
        read_input(&bytes, ScanLimits::default()),
        Err(ScanError::NoDex)
    ));
}

#[test]
fn zip_without_manifest_is_dex_only() {
    let bytes = zip_of(&[("classes.dex", dex())]);
    let input = read_input(&bytes, ScanLimits::default()).unwrap();
    assert_eq!(input.buffers.len(), 1);
    assert!(input.manifest.is_none());
    assert!(input.manifest_error.is_none());
}

#[test]
fn broken_manifest_keeps_the_dex() {
    let bytes = zip_of(&[
        ("classes.dex", dex()),
        ("AndroidManifest.xml", b"not binary xml".to_vec()),
    ]);
    let input = read_input(&bytes, ScanLimits::default()).unwrap();
    assert_eq!(input.buffers.len(), 1);
    assert!(input.manifest.is_none());
    assert!(input.manifest_error.is_some());
    let report = report_for(&bundled(), &input).unwrap();
    assert_eq!(report.manifest_error, input.manifest_error);
    assert_eq!(report.dex_count, 1);
}

#[test]
fn truncated_zip_is_an_error() {
    let bytes = zip_of(&[("classes.dex", dex())]);
    assert!(matches!(
        read_input(&bytes[..40], ScanLimits::default()),
        Err(ScanError::Apk(_))
    ));
}

#[test]
fn limits_are_applied() {
    let bytes = zip_of(&[("classes.dex", dex())]);
    let limits = ScanLimits {
        entry_bytes: 10,
        ..ScanLimits::default()
    };
    assert!(matches!(read_input(&bytes, limits), Err(ScanError::Apk(_))));
}
