<!-- Written by `eval/fdroid.py run`. -->
# F-Droid evaluation of `dexpa sdks`

Index `214b029d8b4b` (entry.json timestamp 1793418664799), seed `dexpa-m6-2026-11-01`. Per app, the newest version is used for presence counts; diffs are between consecutive selected versions.

| | Tracking | NonFreeNet | control |
|---|---:|---:|---:|
| apps | 10 | 10 | 10 |
| APKs analysed / selected | 30 / 30 | 30 / 30 | 30 / 30 |
| APKs failed | 0 | 0 | 0 |
| apps with any SDK | 4 | 3 | 2 |
| apps with `dex_class` evidence | 3 | 3 | 1 |
| apps with `manifest_component` evidence | 2 | 2 | 2 |
| apps with `manifest_meta` evidence | 1 | 0 | 0 |
| median defined classes | 6571 | 9958 | 2642 |
| diff `sdk_evidence_changed:-` | 0 | 0 | 1 |

## SDKs by stratum (apps, newest version)

| SDK | Tracking | NonFreeNet | control |
|---|---:|---:|---:|
| `acra` | 1 | 0 | 1 |
| `countly` | 1 | 0 | 0 |
| `google-mobile-ads` | 1 | 0 | 0 |
| `google-play-services` | 1 | 0 | 1 |
| `microg` | 0 | 2 | 0 |
| `sentry` | 1 | 1 | 0 |

## Manifest parser against index-v2

81 of 90 APKs agree on every field (package, versionCode, versionName, min/target SDK, permissions). On 0 the parser disagrees with the index for a reason other than the index's own additions (`index_*` below).

- `index_implied_read_permission`: 9
- `index_version_name_not_in_apk`: 1
