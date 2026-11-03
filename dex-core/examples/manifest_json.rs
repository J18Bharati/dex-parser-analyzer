// Modified by Jatin Bharati (2026-11-01): new example (fork M6). See NOTICE.
//! Prints an APK's parsed `AndroidManifest.xml` as JSON. The F-Droid evaluation
//! (`eval/fdroid.py run`) compares it with the manifest fields in F-Droid's index.
//!
//! `cargo run -p dexpa-core --example manifest_json -- app.apk`

use std::process::ExitCode;

use dexpa_core::manifest::read_manifest_from_apk;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: manifest_json <app.apk>");
        return ExitCode::from(2);
    };
    match read_manifest_from_apk(&path).map(|m| serde_json::to_string_pretty(&m)) {
        Ok(Ok(json)) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Ok(Err(err)) => {
            eprintln!("error: {err}");
            ExitCode::from(1)
        }
        Err(err) => {
            eprintln!("error: {}: {err}", path.to_string_lossy());
            ExitCode::from(1)
        }
    }
}
