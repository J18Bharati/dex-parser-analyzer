# Notes: dated findings

Anomalies, numbers that looked wrong, and their causes. These are the raw material for the README.

## 2026-11-01: baseline at the fork point (M0)

- `nix develop -c just ci` (lint, clippy for wasm, test, doc, deny) passes on `3f5a124` unmodified: **101 tests in 17 suites, 0 failed, 0 ignored**.
- The first run downloaded the rust-overlay 1.99.1 toolchain. Together with `target/` (2.9 GB), it took 5 GB of `/home`, which also holds `/nix`: 37 GB free before, 32 GB after.
- `cargo deny` ran clean on advisories, bans, licences and sources. Its 5 warnings are duplicate crate versions (convert_case, hashbrown, syn, thiserror, thiserror-impl), all from upstream's dependency tree.
- The upstream-tests rule from here on: `git diff --stat upstream-base -- '*/tests/*'` must show no pre-existing test file modified, and the count of 101 can only grow.

## Hypotheses written before the F-Droid evaluation (M6)

From the 2026-11-01 eng review of the plan. They are recorded here before any APK is fetched, so the results can confirm or refute each one rather than be explained after the fact.

1. **Exodus finds almost no trackers across the ~30 apps.** There are three possible causes, and the table must separate them with per-evidence-source hit counts:
   - the apps really have no trackers;
   - R8 renamed the classes (class-name evidence is gone, but manifest evidence remains);
   - our anchoring is stricter than Exodus's.
2. **Apps with the `Tracking` anti-feature still show zero SDK hits.** F-Droid's flag describes behaviour, not an embedded SDK.
3. **Google Play Services stubs and microG client libraries match a loose `com.google.android.gms` prefix.**
4. **Class counts differ from dexdump or jadx.** We count classes an app defines; those tools may also count classes it only references. Dex index order also matters.
5. **The version diff reports SDKs added or removed that didn't change.** R8 renames, and classes moving between `classesN.dex` files.
6. **Fewer than 3 versions are found per app.** The main repo keeps few; the archive repo is the fallback.
7. **The manifest version doesn't match the index**, because `versionName` is a resource reference.
8. **Downloaded bytes differ from the lockfile sizes.** This should be impossible; the sha256 check would catch it.

## 2026-11-01: the manifest parser (M1)

- **Two upstream behaviours changed (M1a).** Each was first shown by a test failing on upstream's own code:
  - `classes10.dex` was read before `classes2.dex`, because entry names were sorted as text;
  - any `*.dex` at any path counted as app code: 6 entries were read where Android loads 1.
- **No AXML fixture exists, and no third-party APK is committed.** So the tests build their documents with `tests/common/axml_builder.rs`, which is written from the chunk layout in AOSP's `ResourceTypes.h`. The builder and the parser share one author, so they could share a misreading. M6 checks the parser against an independent source: F-Droid's index-v2 `manifest` fields, over every fetched APK.
- **Mutation check, run serially:** 6 mutants, 5 killed:
  - no depth limit;
  - no string budget;
  - attributes looked up by name only;
  - relative component names not resolved;
  - DEX entries not sorted numerically.
- **The survivor is the post-read size check in `read_manifest_entry`.** The declared-size check before it refuses the same oversized input. The second check only matters for a header that understates the entry's size, and the test-side zip writer can't produce one. It stays as defence in depth, without a test.
- **A number that looked wrong, in my own test.** The string-budget test first "passed" against the real parser by accident. Its 65,536-byte string can't be encoded in a UTF-8 pool, where length prefixes stop at 0x7fff, so the builder wrote a garbled length and the parser read a short string. The builder now asserts the limit, and the test uses 30,000 bytes × 5,000 references, which would be 150 MB if decoded.
