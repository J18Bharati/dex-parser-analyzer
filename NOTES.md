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
