<!-- Modified by Zero (2026-11-01): axml.rs and manifest.rs rows, multidex.rs entry rules, dex-sdk, `dexpa sdks` (fork). See NOTICE. -->
# Architecture

What exists in the workspace as of 2026-10-21. The documents in `docs/research/` are design surveys written before the code; where they describe something not listed here (for example a full OWASP static-analysis engine, CHA call graphs, or disassembly and hex views in the GUI), it is not built.

## Data flow

```
bytes (.dex, or an APK through multidex)
  -> dexpa_core::parse_dex           header, section tables, lazy string resolution
  -> dexpa_core::DexFile             classes, methods, code items, decoded instructions
  -> dexpa_core::graphs              CFG per method, call graph, cross-references
  -> dexpa_analysis::engine          pattern, structural and taint passes
  -> AnalysisReport                findings and counts (serde)
  -> dex-cli (text) / dex-gui (browser) / your code
```

Parsing borrows from the input buffer (`DexFile<'a>`), so nothing is copied until a caller asks for an owned value. No unsafe code is used.

## dex-core

| Module | Lines | Role |
|---|---:|---|
| `parser.rs` | 1,048 | `parse_dex`: magic and version check (versions 035, 037-041), the header, the id tables, `map_list`, class data and code items, optional map sections. Bounds errors become `DexError::SectionOutOfBounds`. |
| `format.rs` | 416 | Plain structs mirroring the on-disk layout, plus index newtypes (`StringIdx`, `TypeIdx`, `MethodIdx`, ...). |
| `model.rs` | 615 | `DexFile`, `ClassHandle` and `MethodHandle`: the consumer API (iterate classes, resolve strings, reach code items). |
| `bytecode.rs` | 2,925 | The Dalvik opcode table and decoder: registers, literals, references, switch and fill-array payloads, and the ART/odex quickened forms. |
| `graphs.rs` | 449 | Basic-block CFGs with fallthrough, branch, switch and exception edges; a direct-edge call graph (one edge per resolved `invoke-*` target, without class-hierarchy resolution); cross-references (strings, fields, types, protos, call sites, method handles). Built on `petgraph`. |
| `dto.rs` | 248 | Serializable overviews (`DexOverviewDto`, ...) that the CLI and GUI print. |
| `multidex.rs` | 307 | Reading an APK's app DEX entries (`zip`): archive-root `classes.dex`, `classes2.dex`, ... in numeric load order, each read capped (`MAX_DEX_ENTRY_BYTES`) and their sum capped (`MAX_TOTAL_DEX_BYTES`); and `MultiDex`, which builds shared class, method, string and type indexes across files. |
| `axml.rs` | 297 | (fork) Android binary XML: string pools (UTF-8 and UTF-16, decoded lazily under a byte budget), the resource map, and the element tree; attribute values stay typed, references unresolved. Depth-limited, never panics. |
| `manifest.rs` | 251 | (fork) The `AndroidManifest.xml` model: package, version, SDK levels, permissions, components and meta-data. `android:` attributes resolve by framework resource id first, so names removed by resource shrinking still resolve. |
| `semantics.rs` | 67 | Small helpers layered on `DexFile`. |
| `error.rs` | 84 | `DexError`, the crate's single error type. |

Known gaps (PROGRESS.md "Next Steps" 1-2): the checksum, signature, `file_size` and `endian_tag` are not verified; `debug_info_item` and hidden-API metadata are not decoded. Malformed input is reported as a `DexError`, never a panic: `dex-core/tests/hostile_input_test.rs` covers the known cases, and the cargo-fuzz targets in `fuzz/` (run weekly by `.github/workflows/fuzz.yml`) look for new ones.

## dex-analysis

| Module | Role |
|---|---|
| `config.rs` | `AnalysisConfig`: per-pass switches, the secret-entropy threshold and the finding cap. |
| `engine.rs` | `analyze_dex` and `analyze_dex_bytes`: run the enabled passes, attach each finding's CWE and remediation, sort and de-duplicate, apply `max_findings`, fill `AnalysisStats`. |
| `pattern.rs`, `structural.rs`, `taint.rs` | The three passes; every rule they emit is in [rules.md](rules.md). |
| `rules.rs` | The rule catalogue: each finding id's CWE and remediation text, the code-side twin of [rules.md](rules.md). |
| `data_flow.rs` | A worklist forward data-flow framework over `dexpa-core` CFGs, used by the taint pass. |
| `model.rs` | `Finding` (id, kind, severity, location, message, CWE, remediation, JSON `extra`), `Severity`, `VulnerabilityKind`, `AnalysisReport`. |
| `ffi.rs` | wasm32 only: `wasm-bindgen` exports for JavaScript hosts. |

The engine sorts the findings by severity, most severe first (a stable sort, so pass order breaks ties), drops exact duplicates, and only then applies `max_findings`, so a cap keeps the most severe findings. `tests/rules_test.rs` holds a positive and a negative test per rule id, run against DEX fixtures compiled from `tests/fixtures-src`.

## dex-sdk (fork)

Published with the other crates since M5, because `dexpa-cli` depends on it. It depends on `dex-core` only; `dex-gui` does not depend on it. Its DEX fixtures (`tests/data/sdk-{positive,negative}.dex`) are compiled from `tests/fixtures-src` by `just sdk-fixtures`.

| Module | Role |
|---|---|
| `catalogue.rs` | SDK definitions: this project's TOML list (`dex-sdk/catalogue/sdks.toml`, compiled in; package prefixes of at least two segments and exact meta-data keys) and an importer for Exodus tracker files supplied at run time (signatures anchored at the class name's start; invalid ones skipped and counted). |
| `matcher.rs` | Matches a catalogue against an inventory and a manifest. Evidence per SDK: defined classes (binary search in the sorted class list), manifest components (qualified the way Android does), and meta-data keys, each with counts, examples and DEX indexes. |
| `report.rs` | `SdkReport`, the JSON `dexpa sdks` prints (`schema_version` 1, ODbL attribution when Exodus data was used), and `build_report`, which runs the inventory and the matcher over an app's DEX buffers and manifest. |
| `diff.rs` | SDK changes between two reports: added, removed (with `strong` or `uncertain` confidence) and evidence changed. Counts and DEX placement are deliberately ignored. |
| `inventory.rs` | The classes an app defines, one pass over each DEX file's `class_defs` (no `MultiDex` index, and no merely referenced types), with per-DEX totals and per-prefix class and method counts at depths 2 to 4. |

## dex-cli

`main.rs`: clap arguments (a `.dex` or APK path, `--format`, `--max-findings`, `--fail-on`), then `read_dex_buffers_from_bytes` for APKs, and `parse_dex`, `dex_to_overview` and `analyze_dex` per DEX file, printed as text or as one JSON document. (fork) `sdks.rs`: the `dexpa sdks` subcommand (`args_conflicts_with_subcommands`, so `dexpa <FILE>` is unchanged), which reads the DEX files and the manifest, calls `dexpa_sdk::report::build_report`, and diffs two saved reports. [dex-cli/README.md](../dex-cli/README.md) documents the JSON schema and the exit codes.

## dex-gui

A Leptos 0.8 client-side app compiled to wasm32 with trunk. The user picks a file, and `gloo-file` reads it in the browser; `parse_dex` and `analyze_dex` run in the page, and the overview and findings render as tables. Nothing is uploaded anywhere. `.github/workflows/deploy-dex-gui.yml` builds the bundle with `nix build .#dex-gui` (self-hosted CSS, a Content-Security-Policy; see [dex-gui/README.md](../dex-gui/README.md)) and publishes it to GitHub Pages.

## Toolchain and gates

`rust-toolchain.toml` pins Rust 1.99.1 with rustfmt, clippy and the wasm32 target. `flake.nix` builds its dev shell from that file and adds trunk, just and cargo-deny. `just ci` runs the gate (fmt, clippy native and wasm32, tests, docs, cargo deny), and `.github/workflows/ci.yml` runs the same checks on every push. The flake also builds `packages.dex-cli` and `packages.dex-gui` with the same pinned toolchain. `.github/workflows/release.yml` calls `ci.yml` as its gate, then builds the binaries, drafts the GitHub Release, publishes the crates and redeploys the GUI ([RELEASING.md](../RELEASING.md)).
