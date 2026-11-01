// Modified by Zero (2026-11-01): new module (fork M5); manifest_error, diff catalogue check and
// escaped text output (2026-11-02, /review). See NOTICE.
//! `dexpa sdks`: which third-party SDKs an app contains, and `dexpa sdks diff`: what changed.

use std::{fs, path::Path, path::PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use dexpa_core::{
    DexError,
    manifest::{Manifest, read_manifest_from_apk_bytes},
    multidex::read_dex_buffers_from_bytes,
};
use dexpa_sdk::{
    catalogue::{self, Catalogue},
    diff::{DiffEvent, diff},
    report::{SCHEMA_VERSION, SdkReport, build_report},
};
use serde::Serialize;

use crate::Format;

/// `dexpa sdks [OPTIONS] <FILE>`, or `dexpa sdks diff <OLD> <NEW>`.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct SdksArgs {
    #[command(subcommand)]
    command: Option<SdksCommand>,

    /// A `.dex` file, or an `.apk` (its `classes*.dex` entries and `AndroidManifest.xml`).
    #[arg(value_name = "FILE", required = true)]
    input: Option<PathBuf>,

    /// Use this TOML catalogue instead of the bundled one.
    #[arg(long, value_name = "TOML")]
    catalogue: Option<PathBuf>,

    /// Also match the trackers in this Exodus Privacy JSON file (ODbL 1.0; adds an attribution).
    #[arg(long, value_name = "JSON")]
    exodus: Option<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Debug, Subcommand)]
enum SdksCommand {
    /// Compare two `dexpa sdks --format json` reports: SDKs added, removed, or found differently.
    Diff {
        /// The older version's report.
        #[arg(value_name = "OLD")]
        old: PathBuf,
        /// The newer version's report.
        #[arg(value_name = "NEW")]
        new: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
}

/// `dexpa sdks diff --format json` (`schema_version` 1).
#[derive(Serialize)]
struct DiffOutput {
    schema_version: u32,
    old: Version,
    new: Version,
    events: Vec<DiffEvent>,
}

#[derive(Serialize)]
struct Version {
    package: Option<String>,
    version_code: Option<String>,
    version_name: Option<String>,
}

impl From<&SdkReport> for Version {
    fn from(r: &SdkReport) -> Self {
        Self {
            package: r.package.clone(),
            version_code: r.version_code.clone(),
            version_name: r.version_name.clone(),
        }
    }
}

/// Runs the subcommand and prints its output.
pub fn run(args: &SdksArgs) -> Result<()> {
    match &args.command {
        Some(SdksCommand::Diff { old, new, format }) => run_diff(old, new, *format),
        None => {
            let input = args.input.as_deref().context("an input file is required")?;
            let catalogue = load_catalogue(args.catalogue.as_deref(), args.exodus.as_deref())?;
            let report = scan(input, &catalogue)?;
            match args.format {
                Format::Json => print_json(&report),
                Format::Text => {
                    print_report(input, &report);
                    Ok(())
                }
            }
        }
    }
}

fn read_text(path: &Path, what: &str) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("failed to read the {what} {}", path.display()))
}

fn load_catalogue(toml: Option<&Path>, exodus: Option<&Path>) -> Result<Catalogue> {
    let base = match toml {
        Some(path) => catalogue::load_toml(&read_text(path, "catalogue")?)
            .with_context(|| format!("invalid catalogue {}", path.display()))?,
        None => catalogue::bundled(),
    };
    match exodus {
        Some(path) => {
            let trackers = catalogue::load_exodus_json(&read_text(path, "Exodus file")?)
                .with_context(|| format!("invalid Exodus file {}", path.display()))?;
            base.merge(trackers)
                .context("the Exodus file and the catalogue share an id")
        }
        None => Ok(base),
    }
}

/// The DEX buffers and, for an APK, the manifest. An APK without `AndroidManifest.xml` is read
/// as DEX only. A manifest that is present but does not parse is not silently skipped: Android
/// may still install the app, so the report keeps the DEX evidence, records the error in
/// `manifest_error`, and the error is printed on stderr.
fn scan(input: &Path, catalogue: &Catalogue) -> Result<SdkReport> {
    let bytes = fs::read(input).with_context(|| format!("failed to read {}", input.display()))?;
    let mut manifest_error = None;
    let (buffers, manifest): (Vec<Vec<u8>>, Option<Manifest>) = if bytes.starts_with(b"PK\x03\x04")
    {
        let buffers = read_dex_buffers_from_bytes(&bytes).context("failed to read the APK")?;
        if buffers.is_empty() {
            bail!("{} contains no classes*.dex entry", input.display());
        }
        let manifest = match read_manifest_from_apk_bytes(&bytes) {
            Ok(m) => Some(m),
            Err(DexError::Zip(zip::result::ZipError::FileNotFound)) => None,
            Err(err) => {
                eprintln!(
                    "warning: {}: AndroidManifest.xml not read ({err}); the report has DEX evidence only",
                    input.display()
                );
                manifest_error = Some(err.to_string());
                None
            }
        };
        (buffers, manifest)
    } else {
        (vec![bytes], None)
    };
    let mut report = build_report(catalogue, &buffers, manifest.as_ref())
        .with_context(|| format!("failed to read the DEX files of {}", input.display()))?;
    report.manifest_error = manifest_error;
    Ok(report)
}

fn load_report(path: &Path) -> Result<SdkReport> {
    let text = read_text(path, "report")?;
    let version: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))?;
    match version.get("schema_version").and_then(|v| v.as_u64()) {
        Some(v) if v == u64::from(SCHEMA_VERSION) => {}
        Some(v) => bail!(
            "{} has schema_version {v}; this dexpa reads {SCHEMA_VERSION}",
            path.display()
        ),
        None => bail!(
            "{} has no schema_version: not a `dexpa sdks --format json` report",
            path.display()
        ),
    }
    serde_json::from_value(version)
        .with_context(|| format!("{} is not a `dexpa sdks` report", path.display()))
}

fn run_diff(old: &Path, new: &Path, format: Format) -> Result<()> {
    let (a, b) = (load_report(old)?, load_report(new)?);
    // Another catalogue would show its differences as SDKs added or removed.
    match (&a.catalogue_fingerprint, &b.catalogue_fingerprint) {
        (Some(x), Some(y)) if x != y => bail!(
            "{} and {} were made with different catalogues ({x}, {y}); rerun both with the same \
             --catalogue and --exodus options",
            old.display(),
            new.display()
        ),
        (Some(_), Some(_)) => {}
        _ => eprintln!(
            "warning: a report has no catalogue_fingerprint, so whether both used the same \
             catalogue cannot be checked"
        ),
    }
    if a.package != b.package {
        eprintln!(
            "warning: comparing different packages: {} and {}",
            shown(a.package.as_deref().unwrap_or("(no manifest)")),
            shown(b.package.as_deref().unwrap_or("(no manifest)"))
        );
    }
    let output = DiffOutput {
        schema_version: SCHEMA_VERSION,
        old: (&a).into(),
        new: (&b).into(),
        events: diff(&a, &b),
    };
    match format {
        Format::Json => print_json(&output),
        Format::Text => {
            print_diff(&output);
            Ok(())
        }
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).context("failed to serialise the report")?;
    println!("{json}");
    Ok(())
}

/// `s` with control characters and bidirectional overrides escaped (`\u{1b}`): text from an
/// APK must not be able to move the cursor, recolour or reorder the analyst's terminal.
fn shown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

fn label(v: &Version) -> String {
    format!(
        "{} {} ({})",
        shown(v.package.as_deref().unwrap_or("(no manifest)")),
        shown(v.version_name.as_deref().unwrap_or("?")),
        shown(v.version_code.as_deref().unwrap_or("?")),
    )
}

fn print_report(input: &Path, r: &SdkReport) {
    println!("SDKs in {}", shown(&input.display().to_string()));
    if let Some(package) = &r.package {
        println!(
            "Package:        {} {} ({})",
            shown(package),
            shown(r.version_name.as_deref().unwrap_or("?")),
            shown(r.version_code.as_deref().unwrap_or("?"))
        );
    }
    if let Some(err) = &r.manifest_error {
        println!(
            "Manifest:       AndroidManifest.xml not read ({}); DEX evidence only",
            shown(err)
        );
    }
    println!(
        "Read:           {} DEX file(s), {} defined classes",
        r.dex_count, r.class_count
    );
    println!(
        "Catalogue:      {} SDKs ({} Exodus signatures skipped)",
        r.catalogue_size, r.skipped_signatures
    );
    println!("\nFound {}:", r.matches.len());
    for m in &r.matches {
        println!(
            "  {}  {} [{}]",
            shown(&m.id),
            shown(&m.name),
            shown(&m.category)
        );
        for e in &m.evidence {
            let examples: Vec<String> = e.examples.iter().map(|x| shown(x)).collect();
            println!(
                "      {}:{}  x{}  e.g. {}",
                e.source.as_str(),
                shown(&e.rule),
                e.count,
                examples.join(", ")
            );
        }
    }
    if r.matches.is_empty() {
        println!("  (none)");
    }
    if let Some(attribution) = &r.attribution {
        println!("\n{attribution}");
    }
}

fn print_diff(d: &DiffOutput) {
    println!("{}  ->  {}", label(&d.old), label(&d.new));
    let list = |v: &[String]| v.iter().map(|x| shown(x)).collect::<Vec<_>>().join(", ");
    for event in &d.events {
        match event {
            DiffEvent::SdkAdded {
                id,
                name,
                confidence,
            } => println!("+ {}  {}  [{confidence:?}]", shown(id), shown(name)),
            DiffEvent::SdkRemoved {
                id,
                name,
                confidence,
            } => println!("- {}  {}  [{confidence:?}]", shown(id), shown(name)),
            DiffEvent::SdkEvidenceChanged { id, gained, lost } => {
                println!(
                    "~ {}  gained [{}]  lost [{}]",
                    shown(id),
                    list(gained),
                    list(lost)
                );
            }
        }
    }
    if d.events.is_empty() {
        println!("  no SDK changes");
    }
}
