<!-- Modified by Zero (2026-11-02): new file (fork M6); review note on report fields; parser changes (re-review). See NOTICE. -->
# F-Droid evaluation of `dexpa sdks`

**Claim.** On 90 F-Droid APKs (30 apps × 3 versions), the manifest parser agreed with F-Droid's index on every field it could have known. 81 APKs match exactly, and the other 9 differ only by values the index adds itself. The SDK matcher found SDKs in 9 of the 30 apps. Finding them took manifest evidence as well as class names. The run also turned up two weaknesses in the matcher, described below.

Run it (network, about 1.4 GB, sequential; every APK is deleted after analysis):

```bash
nix develop .#eval -c just eval-test       # offline tests of the scripts
nix develop .#eval -c just fdroid-select   # rewrite the lockfile from the current signed index
nix develop .#eval -c just fdroid-eval     # fetch, analyse, delete; writes results.json and TABLE.md
```

`fdroid-eval` downloads exactly what [`fdroid.lock.toml`](fdroid.lock.toml) pins. The trust chain runs from F-Droid's published signing key (`43:23:8D:51:…:C9:CC:AB`) through `entry.jar` to the `index-v2.json` sha256, and from there to each APK's sha256 and size. Re-running against the committed lockfile fetches the same bytes or fails.

## Sample

- **Source:** F-Droid index `214b029d` (4,923 packages), fetched 2026-11-01.
- **Strata:** 10 apps in each of three groups, set by the newest version's anti-features: `Tracking`, `NonFreeNet`, and none (`control`). Apps with any other anti-feature are left out.
- **Selection:** apps are taken in seeded-hash order (seed `dexpa-m6-2026-11-01`). Each app contributes its 3 newest versionCodes, with the smallest APK per code, all of 60 MB or less.
- **Bias:** every app has 3 such versions in the main repo, so the sample leans toward apps that are still updated. The archive repo was not needed.
- **Download:** 1,368,909,611 bytes, equal to the lockfile's total, on both runs. 0 sha256 or size failures, 0 analysis failures.

## Results

The full table is in [TABLE.md](TABLE.md); per-APK reports are in [reports/](reports/) and aggregates in [results.json](results.json).

The committed reports were made before `catalogue_fingerprint` and `manifest_error` existed, so they lack both, and `dexpa sdks diff` warns that it cannot check their catalogues. The run also predates three parser changes from review: DEX files now stop at the first missing number, an APK with duplicate entry names is refused, and a numbered DEX entry that is not a DEX file is an error. Re-running would show whether any of them changes the 90 results; it has not been re-run.

| | Tracking | NonFreeNet | control |
|---|---:|---:|---:|
| apps with any SDK (newest version) | 4 | 3 | 2 |
| … found by defined classes | 3 | 3 | 1 |
| … found by manifest components | 2 | 2 | 2 |
| … found by manifest meta-data | 1 | 0 | 0 |
| median defined classes | 6,571 | 9,958 | 2,642 |
| diff events over 20 version pairs | 0 | 0 | 1 |

The matches were Sentry (2 apps), ACRA (2), Play Services (2), microG (2), Countly (1) and Google Mobile Ads (1).

**R8 kept the packages.** In one Tracking app, the 1,554 classes under `io.sentry` include minified names such as `io.sentry.A` and `io.sentry.A0`, but they still sit under `io.sentry`. Package-prefix matching survives obfuscation that renames classes but doesn't repackage them.

**Manifest parser cross-check:** 81 of 90 APKs agree with the index on package, versionCode, versionName, min and target SDK, and permissions. The other 9 were checked against the APK bytes, and in every case the difference comes from the index:

- **9 APKs (3 apps): `READ_EXTERNAL_STORAGE`.** The string does not occur in the manifest at all; only `WRITE_EXTERNAL_STORAGE` is declared. The index adds READ, which Android grants implicitly with WRITE.
- **1 APK (Navit 2026020887): the version name.** Its manifest has no `versionName` attribute, so the index's `v0.5.7` comes from F-Droid's build metadata.

## Numbers that looked wrong

1. **`facebook` matched a Tracking app through Fresco**, React Native's image library (`com.facebook.animated.gif.GifImage`, 1,669 classes), not the Facebook SDK. The catalogue's `com.facebook` prefix covered every open-source Facebook library. It is now narrowed to the SDK's own packages (`appevents`, `login`, `share`, `ads`, …), with a test, and the run was repeated. That moved Tracking "apps with any SDK" from 5 to 4 and changed nothing else.
2. **Manifest evidence without the code.** Two cases:
   - One Tracking app declares `com.google.android.gms.ads.APPLICATION_ID` but defines no class under `com.google.android.gms`.
   - Two apps declare the component `com.google.android.gms.metadata.ModuleDependencies` and also define no Play Services classes.

   M4's diff labels manifest evidence `strong` because R8 can't rename it. These cases show it can be a leftover. Not fixed yet: a `manifest_only` flag on the match is the obvious next step.
3. **9 permission mismatches** looked like a parser bug. They are the implied-permission rule above, and are now counted separately (`index_implied_read_permission`), so a real parser disagreement would still show.

## The hypotheses written before the run

These were recorded in [NOTES.md](../NOTES.md) before any APK was fetched.

| # | Prediction | Result |
|---|---|---|
| 1 | Exodus finds almost no trackers | **Not tested.** `reports.exodus-privacy.eu.org` timed out from this machine over IPv4 and IPv6, so only the bundled 25-SDK catalogue ran. `--exodus` works (tested); rerun with a trackers file to answer it. |
| 2 | Tracking apps still show zero SDK hits | **Confirmed:** 6 of 10, against the bundled catalogue only. |
| 3 | Play Services stubs and microG match a loose `com.google.android.gms` prefix | **Not as predicted.** No app defines classes under `com.google.android.gms`. microG matched 2 apps through its own libraries (`org.microg.safeparcel`, `org.microg.nlp.api`), which is correct. The Play Services hits come from leftover manifest entries (see above). |
| 4 | Class counts differ from dexdump or jadx | **Not tested:** neither tool was run. |
| 5 | The diff reports SDKs added or removed that didn't change | **Not observed:** 0 added or removed over 60 consecutive version pairs. The one event, Navit gaining an ACRA manifest component, is between its two oldest selected versions (versionCodes 6382 and 2021030660), which are far apart. Consecutive versions rarely change SDKs, so this is weak evidence either way. |
| 6 | Fewer than 3 versions per app | **Refuted:** the main repo had 3 for every selected app (and for 3,109 eligible apps). |
| 7 | versionName mismatches because it is a resource reference | **Refuted as stated:** 0 of 90 were references. The one mismatch is a versionName missing from the APK. |
| 8 | Downloaded bytes differ from the lockfile | **Refuted:** equal to the byte, on both runs. |

## Data and licences

- **Committed:** only F-Droid index metadata (package names, versions, file names, hashes and sizes, in the lockfile) and this tool's own output.
- **Not committed:** APKs, which are downloaded to `.cache/` and deleted. No Exodus data is bundled or committed; it is ODbL 1.0, and a report made with `--exodus` carries the attribution.
