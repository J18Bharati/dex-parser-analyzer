<!-- Modified by Zero (2026-11-01): `dexpa sdks` and `dexpa sdks diff` (fork M5); APK entries in load order (fork M1a); manifest_error, catalogue_fingerprint, diff refusal (2026-11-02, /review). See NOTICE. -->
# dexpa-cli

`dexpa`, the command-line front end of [dex-parser-analyzer](../README.md). It parses a `.dex` file, or every `classes*.dex` in an APK, with [`dexpa-core`](../dex-core/), runs every [`dexpa-analysis`](../dex-analysis/) pass with the default configuration, and prints the results.

```
dexpa [OPTIONS] <FILE>
dexpa sdks [OPTIONS] <FILE>       (fork; see "SDK detection" below)
dexpa sdks diff [OPTIONS] <OLD> <NEW>

Arguments:
  <FILE>  A `.dex` file, or an `.apk`/zip whose `classes*.dex` entries are analysed in load order

Options:
      --format <FORMAT>       Output format [default: text] [possible values: text, json]
      --max-findings <COUNT>  At most this many findings per DEX file
      --fail-on <SEVERITY>    Exit with status 2 when any finding is at or above this severity [possible values: info, low, medium, high, critical]
  -h, --help                  Print help
  -V, --version               Print version
```

Install from the workspace with `cargo install --path dex-cli`, or run it in place:

```bash
cargo run -p dexpa-cli -- dex-analysis/tests/data/rules-positive.dex --max-findings 25
cargo run -p dexpa-cli -- app.apk --format json --fail-on high
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | The input was analysed, and no finding met `--fail-on` (or `--fail-on` was not given). |
| 1 | The input could not be read, is neither a DEX file nor a zip with `classes*.dex` entries, or failed to parse. The error is on stderr. |
| 2 | The input was analysed, and at least one finding is at or above the `--fail-on` severity. The report is still printed. |

So `dexpa app.apk --fail-on high` in CI fails the job on any High or Critical finding.

## Text output

For each DEX file in the input:
1. the header summary: version, checksum, file size, and the string, type, field, method and class counts;
2. the ten classes with the most methods;
3. the analysis counts and the findings, most severe first (severity, rule id, location, message).

## JSON output (`--format json`)

One document. `schema_version` changes only with a breaking change to the shape below; adding a field is not breaking.

```json
{
  "schema_version": 1,
  "input": "app.apk",
  "dex_files": [
    {
      "index": 0,
      "summary": {
        "version": "35",
        "checksum": 3692947289,
        "file_size": 614592,
        "string_count": 4329,
        "type_count": 596,
        "field_count": 865,
        "method_count": 3602,
        "class_count": 340
      },
      "top_classes": [
        { "descriptor": "Landroid/support/v4/app/Fragment;", "method_count": 95 }
      ],
      "report": {
        "findings": [
          {
            "id": "M1_HARDCODED_SECRET",
            "kind": "HardcodedSecret",
            "severity": "Medium",
            "location": {
              "class_descriptor": "Lcom/example/Foo;",
              "method_name": "bar",
              "method_signature": "(Ljava/lang/String;)V",
              "pc": null
            },
            "message": "Literal `...` matched entropy pattern",
            "cwe": 798,
            "remediation": "Remove the credential from the app; ...",
            "extra": { "pattern": "entropy", "literal": "..." }
          }
        ],
        "stats": { "method_count": 3602, "string_count": 4329, "instruction_count": 26192, "elapsed_ms": 66 }
      }
    }
  ]
}
```

- `dex_files` holds one entry for a `.dex` input, and one per app DEX file for an APK (the archive-root `classes.dex`, `classes2.dex`, ... in numeric load order); `index` is the position.
- `top_classes` lists at most ten classes, most methods first.
- `report` is the `dexpa-analysis` `AnalysisReport` as its serde types serialise it. Findings are listed most severe first. The rule ids, their severities, CWEs and remediation texts are in [docs/rules.md](../docs/rules.md); `cwe` and `remediation` are `null` for an id outside that catalogue.

## SDK detection: `dexpa sdks` (fork)

```
dexpa sdks [--catalogue T.toml] [--exodus F.json] [--format text|json] <FILE>
dexpa sdks diff [--format text|json] <OLD.json> <NEW.json>
```

`dexpa sdks` lists the third-party SDKs an app contains, each with the evidence that found it ([`dexpa-sdk`](../dex-sdk/)):

- `dex_class`: classes the app *defines* (from `class_defs`) under one of the SDK's package prefixes. Classes the app only calls into are not counted. Prefixes match on package boundaries, so `io.sentry` does not match `io.sentryx`.
- `manifest_component`: an activity, service, receiver or provider in the SDK's packages. Android instantiates components by name, so R8 cannot rename them; this evidence survives minification when the class names do not.
- `manifest_meta`: an exact `<meta-data>` key, such as `io.sentry.dsn`.

The bundled catalogue lists 25 SDKs (`dex-sdk/catalogue/sdks.toml`). `--catalogue` replaces it with your own TOML in the same shape. `--exodus` adds the trackers from an [Exodus Privacy](https://exodus-privacy.eu.org/) JSON file you supply; that data is ODbL 1.0, is never bundled, and when it is used the report carries an `attribution`.

An APK without `AndroidManifest.xml` is read as DEX only (`package` is `null`). A manifest that is present but malformed is reported on stderr and in `manifest_error`, and the report has DEX evidence only. The app's DEX files are `classes.dex`, `classes2.dex`, ... up to the first missing number, as Android loads them; an APK with two entries of the same name is refused. Text output escapes control and bidirectional-override characters.

`dexpa sdks diff` compares two reports from `dexpa sdks --format json`. It refuses reports whose `catalogue_fingerprint`s differ, since a different catalogue would show up as SDKs added and removed, and warns when a report has no fingerprint (one made before the field existed) or the packages differ.

- `sdk_added` and `sdk_removed` have a `confidence`. It is `strong` when the side that has the SDK has manifest evidence, and `uncertain` when that side has only class names, which a change in minification can make appear or vanish.
- `sdk_evidence_changed` lists the `source:rule` pairs gained and lost.

Counts and DEX placement are not compared, because a rebuild changes them without changing what the app contains.

Exit codes: 0 when the report or diff is printed, including a DEX-only report after a malformed manifest; 1 when an input, the catalogue or the Exodus file cannot be read or parsed, when an APK has no `classes.dex`, when a report given to `diff` does not have `schema_version` 1, or when the two reports were made with different catalogues.

### JSON (`schema_version` 1, independent of the analysis report's)

```json
{
  "schema_version": 1,
  "package": "app.fixture",
  "version_code": "2",
  "version_name": "1.2",
  "dex_count": 1,
  "class_count": 5,
  "catalogue_size": 25,
  "catalogue_fingerprint": "fnv1a64:608ed0e5cc895aec",
  "skipped_signatures": 0,
  "manifest_error": null,
  "attribution": null,
  "matches": [
    {
      "id": "sentry",
      "name": "Sentry",
      "category": "crash-reporting",
      "origin": "catalogue",
      "evidence": [
        { "source": "dex_class", "rule": "io.sentry", "count": 2,
          "examples": ["io.sentry.Sentry", "io.sentry.android.core.SentryAndroid"], "dex_indexes": [0] },
        { "source": "manifest_meta", "rule": "io.sentry.dsn", "count": 1,
          "examples": ["io.sentry.dsn"], "dex_indexes": [] }
      ]
    }
  ]
}
```

- `version_code` and `version_name` are text. A resource reference is printed as `@0x7f...` because `resources.arsc` is not read.
- `matches` is sorted by id. `evidence` is sorted by source, then rule, and keeps at most 3 `examples`. `skipped_signatures` counts Exodus signatures that were empty or were not valid regexes. `catalogue_fingerprint` identifies the catalogue (ids, rules and meta-data keys; names and categories are left out). `manifest_error` is the reason a present manifest was not read, else `null`. Both are absent from reports made before they existed and then load as `null`.
- The diff document is `{"schema_version": 1, "old": {package, version_code, version_name}, "new": {...}, "events": [...]}`; each event has an `event` tag (`sdk_added`, `sdk_removed`, `sdk_evidence_changed`). The full examples are the golden files in `tests/golden/`.

Licensed under MIT OR Apache-2.0.
