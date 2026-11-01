// Modified by Zero (2026-11-01): new fuzz target (fork M1b). See NOTICE.
//! Parse arbitrary bytes as a binary AndroidManifest.xml, then resolve every component name.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(manifest) = dexpa_core::manifest::parse_manifest(data) {
        for component in &manifest.components {
            let _ = component.qualified_name(manifest.package.as_deref());
        }
    }
});
