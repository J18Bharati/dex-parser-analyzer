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

## 2026-11-01: the inventory (M2)

- **The fixtures are thin for this.** `multidex.apk` defines 2 classes, one per DEX file (`com.foobar.foo...` and `com.blafoo.bar...`), and most single-DEX fixtures use the default package. So the depth-4 prefix logic is covered by a unit test with synthetic names, not by a fixture. Real depth arrives with the M5 fixtures and the M6 APKs.
- **Mutation check:** 5 mutants, 3 killed (array descriptors, method counts, sort order).
  - `prefix_past_package` was equivalent, because the depth filter was redundant (`nth` already stops at the last dot). The filter is deleted.
  - `drop_unreadable_count` survived, because no fixture has a class whose descriptor fails to read. The invariant test would catch it on such an input.

## 2026-11-01: the SDK matcher (M3), and a fuzz run

- **Two semantics were decided by failing tests, and are now pinned:**
  - **`com.foo$C` is not under the package prefix `com.foo`.** It is an inner class of the class `com.foo`, in package `com`. My first `under()` accepted it; the test expected the package-boundary rule.
  - **Exodus signatures are regexes, so `.` matches any character.** `com.bar.` matches `com.barn.W` in Exodus too. My first test assumed a literal dot. The importer keeps Exodus's regex semantics and differs only in anchoring: a signature must match at the start of the class name, so `com.foo` does not match `x.com.foo.Bar`. M6 has to measure how many hits that difference costs or saves.
- **Mutation check:** 7 mutants, 7 killed:
  - no package boundary;
  - unanchored Exodus signatures;
  - no two-segment rule;
  - no duplicate check;
  - no manifest-component evidence;
  - component names not qualified;
  - a bad regex made fatal.
- **Fuzzing `parse_manifest` for 60 s (M1 follow-up):** 6.7 million inputs, 210 edges covered, no crash and no panic. The nightly fuzz shell took 2 GB of disk; 29 GB free after.
- **The bundled catalogue (25 SDKs) is written from memory of public package names and documented manifest keys.** Every entry is a hypothesis until M6 sees it in a real APK. The table there should report, per entry, whether it matched anything at all.

## 2026-11-01: the version diff (M4)

- Moved ahead of the F-Droid milestone, so the tool is complete offline even if M6's network budget fails.
- **Rule:** compare which SDKs are present and the kinds of evidence (`source:rule`), never counts or DEX placement. A rebuild moves classes between `classesN.dex` files and changes counts without changing what the app contains.
- **Mutation check:** 4 mutants, 4 killed. Two of them target the "no event" tests, which had passed trivially against the empty stub, so those tests do carry weight.

## 2026-11-01: the CLI (M5)

- **`dexpa sdks <FILE>` and `dexpa sdks diff OLD NEW`, with `dexpa <FILE>` unchanged.** clap's `args_conflicts_with_subcommands` and `subcommand_negates_reqs` make the positional `FILE` and the subcommand mutually exclusive. `dex-cli/tests/cli.rs` passes unmodified. A file named `sdks` needs a path (`dexpa ./sdks`).
- **Deviation from the plan: `dexpa-sdk` is now publishable.** The plan kept it `publish = false` so upstream's release rehearsal would not change. But the CLI depends on it, and `cargo publish` cannot package a crate that depends on an unpublished one. The choice was between publishing `dexpa-sdk`, unpublishing `dexpa-cli`, or shipping a second binary. I published `dexpa-sdk`: it joins `CRATES` in release.yml. `cargo publish --dry-run --workspace` packages and verifies all four crates; nothing was uploaded.
- **Fixtures with known positives**, compiled from Java sources by `just sdk-fixtures` (JDK 21, D8 9.5.28). A second build was byte-identical. Which SDKs they contain is fixed by the sources, not by this tool's output, so the golden files are checked against something independent:
  - **positive:** `FirebaseAnalytics`, `Sentry`, `SentryAndroid`, `MobileAds`, and an app class.
  - **negative:** near-misses on package boundaries (`com.google.firebase.analyticsx`, `io.sentryx`, `com.example.firebase_like`).
  - **Both** call a Facebook class that is only on the classpath (referenced, never defined). It never matches, which is the defined-versus-referenced rule end to end.
- **The test APKs are assembled in the test** from a fixture DEX and a manifest built with M1's AXML builder. That builder is shared through `#[path]`, not copied. The v2 manifest declares Firebase Messaging's service with no Firebase Messaging classes in the DEX: the minified-app case. The diff reports it `strong`, while Mobile Ads, which has class names only, is `uncertain`.
- **A manifest that is absent is fine; a manifest that is present and broken is exit 1.** An APK without `AndroidManifest.xml` (upstream's `multidex.apk`) is read as DEX only. The alternative, skipping the manifest whenever it fails to parse, would silently turn every manifest-only SDK into "not found", which is exactly the evidence M3 added.
- **Mutation check:** 10 mutants, all killed in the end, but 2 only after I added a test:
  - **"Skip every manifest error" survived at first.** No test had a broken manifest. Added `malformed_manifest_exits_1_instead_of_being_skipped`.
  - **Turning off `args_conflicts_with_subcommands` survived at first.** `dexpa --fail-on high sdks app.apk` then parsed, and `--fail-on` was silently ignored. Added `analysis_options_are_rejected_before_the_sdks_command`, which expects a usage error (exit 2).
  - **The other 8 were killed by the first version of the tests:** the manifest dropped, the schema check removed, `--catalogue` or `--exodus` ignored, no attribution, the skipped count lost, hex versionCodes printed in hex, and the diff's `+` marker removed.
- `just ci` green: 158 tests (137 before; 16 CLI and 5 report tests added).

## 2026-11-01: F-Droid evaluation tooling (M6a)

- **Checked before writing any code:**
  - **The signing key.** F-Droid's signing-keys page lists the index signer as `43:23:8D:51:…:C9:CC:AB`, the same value the eng review recalled. `jarsigner -verify` passes on `entry.jar`, and `keytool` reports exactly that SHA-256. The warnings about a self-signed certificate and SHA1withRSA are expected for F-Droid's long-lived key, so trust rests on the pinned fingerprint, not on a CA chain.
  - **The index pin.** `entry.json` pins `index-v2.json` at 70,359,050 bytes, sha256 `214b029d…`, 4,923 packages. The download matched both.
  - **The schema.** Per-version `antiFeatures`, `file.{name,sha256,size}`, and `manifest.{versionCode,versionName,usesSdk,usesPermission,usesPermissionSdk23}` exist as the scripts assume.
- **The trust chain:** pinned key → `entry.jar` → `entry.json` → index sha256 → per-APK sha256 in the committed lockfile. `fetch` trusts only the lockfile, so a re-run downloads the same bytes or fails.
- **Deviation: the archive repo is not used.** Plan hypothesis 6 expected fewer than 3 versions per app. In fact the main repo alone has 47 Tracking, 434 NonFreeNet and 2,628 control apps with 3 or more versions of 60 MB or less. The archive index (119 MB) would add only older versions. The sample is therefore biased toward apps that are still updated, and the README says so.
- **Deviation: scripts.** The plan named `scripts/fdroid-select` and `scripts/fdroid-fetch`. Instead there is one module, `eval/fdroid.py`, with `select`, `fetch` and `run` subcommands, behind `just` recipes. The selection needs a JDK and Python with `ssl`, which upstream's default shell (`python3Minimal`) doesn't provide, so there is a separate `.#eval` shell.
- **The selection** has 90 APKs (1,369 MB): 10 apps per stratum, 3 versions each. Apps are taken in seeded-hash order, and each versionCode uses its smallest APK, because ABI splits share a code. Stratum comes from the newest version's anti-features. Apps with any other anti-feature are left out of the control group.
- **Tests:** 25 offline tests, run against a `file://` repo in a temp directory, so there's no network.
  - **They were written before the module but had only failed at import.** So the weight check is the mutation pass.
  - **Mutation check: 21 mutants, 21 killed.** They removed each fetch guard (sha256, size, read cut-off, partial cleanup, resume, total, per-file, disk and time budgets, file-name and URL-scheme checks); broke selection (anti-feature strata, smallest APK per code, newest versions, seed); weakened the signer checks (two signers, fingerprint); and broke the cross-check and summary (permissions, reference category, SDK-23 permissions, newest version).
  - My script mislabelled the test that killed one mutant ("reference not separated"). Rerun by hand, the expected test, `test_reference_version_name_is_its_own_category`, fails.

## 2026-11-02: the F-Droid run (M6b)

- **Run:** 90 of 90 APKs analysed, with 0 failures. 1,368,909,611 bytes were downloaded, exactly the lockfile's total, and every APK was deleted after analysis. The full results, the "looked wrong" list and the hypothesis verdicts are in `eval/README.md`.
- **Two runs; the first was discarded.** The first run matched `facebook` in one Tracking app through Fresco (`com.facebook.animated.gif.*`). The catalogue entry was narrowed, with a failing test first, and the whole run was repeated from the same lockfile. Only that one count changed (Tracking apps with any SDK, 5 → 4).
- **Three cross-check differences, all checked on the APK bytes before being classified:**
  - **READ_EXTERNAL_STORAGE (9 APKs, 3 apps).** I re-fetched two of the APKs against the lockfile. Neither manifest contains the string `READ_EXTERNAL_STORAGE`, in UTF-16 or UTF-8; both contain `WRITE_EXTERNAL_STORAGE`. The index adds the implied READ.
  - **Navit's version name.** Its manifest contains neither `versionName` nor `0.5.7`.
  - **How they are counted now.** Both are tallied as `index_*` additions, and the table counts parser disagreements separately: 0.
  - **An aside:** that probe first hit the free-disk guard, because the scratchpad is a 7.5 GB tmpfs. The guard behaved as designed.
- **Not fixed: manifest evidence can be a leftover.** One app declares `com.google.android.gms.ads.APPLICATION_ID`, and two declare the `ModuleDependencies` component, while defining no `com.google.android.gms` class. M4's `strong` rests on the assumption that manifest evidence is reliable, which these cases contradict. The next step is a `manifest_only` flag, which would change the report schema; it is left for a decision.
- **Exodus unreachable.** `reports.exodus-privacy.eu.org` and `exodus-privacy.eu.org` timed out over IPv4 and IPv6, so hypothesis 1 is untested rather than answered from memory.
- **Mutation check, classifier:** 4 mutants, 4 killed (WRITE not required; any permission difference treated as implied; any version-name difference treated as metadata; index additions counted as parser errors).
- **Two tooling bugs found on the way:**
  - The recipe's `cargo build --example manifest_json` built only the example, never `dexpa`. Fixed, and a preflight now checks for both tools before any download.
  - The `.#eval` shell lacked Rust, so cargo fell back to the host's rustup. The shell now carries the pinned toolchain.
