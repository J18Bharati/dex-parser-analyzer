#!/usr/bin/env python3
# Modified by Zero (2026-11-01): new file (fork M6); run-loop fixes (2026-11-02, /review); reports
# cleared before a run, tool errors per APK, UTF-8 lockfile, HTTP errors per APK (re-review).
# See NOTICE.
"""The F-Droid evaluation of `dexpa sdks`: select apps, fetch them under a budget, analyse them.

  select  Verify F-Droid's signed entry.jar against the pinned key, fetch index-v2.json (its
          sha256 is pinned by entry.json), pick apps per stratum, write eval/fdroid.lock.toml.
          Needs `jarsigner` and `keytool` (run inside `nix develop .#eval`).
  fetch   Download exactly the lockfile's APKs: one at a time, each to a .part file, checked
          against the pinned sha256 and size, under per-file, total, time and free-disk budgets.
  run     fetch, then `dexpa sdks` and the manifest cross-check per APK, `dexpa sdks diff` between
          versions of an app; APKs are deleted after analysis unless --keep. Writes
          eval/results.json and eval/TABLE.md. An APK that cannot be fetched or verified, or whose
          analysis fails or passes --tool-timeout, is recorded in `failures` and the run goes on.

Standard library only. Exit codes: 0 ok, 1 verification or analysis setup failure, 3 a budget ran out.
"""

from __future__ import annotations

import argparse
import hashlib
import http.client
import io
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.request
import zipfile
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path

REPO = "https://f-droid.org/repo"
# SHA-256 of the certificate that signs f-droid.org's index, as published at
# https://f-droid.org/docs/Release_Channels_and_Signing_Keys/ (checked 2026-11-01).
FDROID_SIGNER_SHA256 = "43238d512c1e5eb2d6569f4a3afbf5523418b82e0a3ed1552770abb9a9c9ccab"
STRATA = ("Tracking", "NonFreeNet", "control")
NAME_RE = re.compile(r"^/[A-Za-z0-9][A-Za-z0-9._-]*\.apk$")
CHUNK = 1 << 16

# Budgets: the defaults of both the Budget dataclass and the command-line options.
MAX_FILE_BYTES = 60_000_000
MAX_TOTAL_BYTES = 3_000_000_000
MAX_MINUTES = 45
MIN_FREE_BYTES = 15_000_000_000
PAUSE_SECONDS = 1.0
TOOL_TIMEOUT_SECONDS = 600

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / ".cache" / "fdroid"


class VerifyError(Exception):
    """Something did not match what was pinned. Exit 1."""


class BudgetError(Exception):
    """A byte, time or disk budget ran out. Exit 3."""


# ---------------------------------------------------------------- budgets and fetching


@dataclass
class Budget:
    max_file: int = MAX_FILE_BYTES
    max_total: int = MAX_TOTAL_BYTES
    seconds: float = MAX_MINUTES * 60
    min_free: int = MIN_FREE_BYTES
    pause: float = PAUSE_SECONDS


@dataclass
class FetchState:
    budget: Budget
    downloaded: int = 0
    files: int = 0
    started: float = field(default_factory=time.monotonic)

    def check_time(self):
        if time.monotonic() - self.started >= self.budget.seconds:
            raise BudgetError(f"time budget of {self.budget.seconds:.0f} s used up")


def _sha256_file(path: Path) -> str:
    with open(path, "rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def _matches(path: Path, entry: dict) -> bool:
    return path.stat().st_size == entry["size"] and _sha256_file(path) == entry["sha256"]


def check_base_url(base_url: str) -> None:
    if not (base_url.startswith("https://") or base_url.startswith("file://")):
        raise VerifyError(f"refusing base URL {base_url!r}: https or file only")


def ensure(entry: dict, dest: Path, base_url: str, state: FetchState) -> Path:
    """The verified local copy of one lockfile APK, downloading it only if needed."""
    name = entry["name"]
    if not NAME_RE.match(name):
        raise VerifyError(f"refusing file name {name!r}")
    check_base_url(base_url)
    state.check_time()
    dest.mkdir(parents=True, exist_ok=True)
    final = dest / name[1:]
    if final.exists():
        if _matches(final, entry):
            return final
        final.unlink()

    size = entry["size"]
    b = state.budget
    if size > b.max_file:
        raise BudgetError(f"{name}: {size} bytes is over the per-file cap of {b.max_file}")
    if state.downloaded + size > b.max_total:
        raise BudgetError(f"{name}: would pass the total budget of {b.max_total} bytes")
    if shutil.disk_usage(dest).free - size < b.min_free:
        raise BudgetError(f"{name}: would leave less than {b.min_free} bytes free")
    if state.files and b.pause:
        time.sleep(b.pause)

    part = final.with_name(final.name + ".part")
    h = hashlib.sha256()
    got = 0
    try:
        with urllib.request.urlopen(base_url.rstrip("/") + name, timeout=60) as resp, open(part, "wb") as out:
            while got <= size:
                chunk = resp.read(min(CHUNK, size + 1 - got))
                if not chunk:
                    break
                got += len(chunk)
                state.downloaded += len(chunk)
                h.update(chunk)
                out.write(chunk)
                state.check_time()
        state.files += 1
        if got != size:
            raise VerifyError(f"{name}: got {got if got <= size else 'more than ' + str(size)} bytes, lock says {size}")
        if h.hexdigest() != entry["sha256"]:
            raise VerifyError(f"{name}: sha256 {h.hexdigest()} does not match the lock")
        os.replace(part, final)
        return final
    finally:
        if part.exists():
            part.unlink()


# ---------------------------------------------------------------- selection


def _runner(cmd: list[str]) -> str:
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout + r.stderr


def check_entry_jar(path, expected_sha256: str, runner=_runner) -> None:
    """jarsigner must verify the jar, and its single signer must be the pinned certificate."""
    out = runner(["jarsigner", "-verify", str(path)])
    if "jar verified." not in out or "unsigned" in out:
        raise VerifyError(f"jarsigner did not verify {path}: {out.strip()[:200]}")
    fps = re.findall(r"SHA256:\s*([0-9A-Fa-f:]{95})", runner(["keytool", "-printcert", "-jarfile", str(path)]))
    if len(fps) != 1:
        raise VerifyError(f"expected one signer certificate, found {len(fps)}")
    if fps[0].replace(":", "").lower() != expected_sha256:
        raise VerifyError(f"signer {fps[0]} is not the pinned F-Droid key")


def read_entry_json(jar) -> dict:
    """entry.json from a jar that check_entry_jar verified. A duplicated name is refused: Java
    and Python zip readers may pick different copies, so the verified one might not be read."""
    with zipfile.ZipFile(jar) as z:
        names = z.namelist()
        if len(names) != len(set(names)):
            raise VerifyError(f"{jar} has duplicate entry names")
        return json.loads(z.read("entry.json"))


def _order(seed: str, package: str) -> str:
    return hashlib.sha256(f"{seed}\0{package}".encode()).hexdigest()


def _stratum(version: dict) -> str | None:
    af = set(version.get("antiFeatures") or {})
    if "Tracking" in af:
        return "Tracking"
    if "NonFreeNet" in af:
        return "NonFreeNet"
    return "control" if not af else None


def _entry(package: str, stratum: str, v: dict) -> dict:
    m = v["manifest"]
    sdk = m.get("usesSdk") or {}
    min_sdk = sdk.get("minSdkVersion", 1)
    perms = {p["name"] for p in m.get("usesPermission") or []} | {p["name"] for p in m.get("usesPermissionSdk23") or []}
    return {
        "package": package,
        "stratum": stratum,
        "version_code": m["versionCode"],
        "version_name": m.get("versionName", ""),
        "name": v["file"]["name"],
        "sha256": v["file"]["sha256"],
        "size": v["file"]["size"],
        "anti_features": sorted(v.get("antiFeatures") or {}),
        "min_sdk": min_sdk,
        "target_sdk": sdk.get("targetSdkVersion", min_sdk),
        "permissions": sorted(perms),
    }


def select(index: dict, per_stratum: int, versions: int, max_file: int, seed: str) -> list[dict]:
    """Per stratum (by the newest version's anti-features), the first `per_stratum` packages in
    seeded-hash order that have `versions` distinct versionCodes with an APK under `max_file`.
    Per versionCode, the smallest APK (ABI splits share a code). Newest versions first."""
    candidates: dict[str, list] = {s: [] for s in STRATA}
    for package, p in index["packages"].items():
        vs = list(p.get("versions", {}).values())
        if not vs:
            continue
        newest = max(vs, key=lambda v: v["manifest"]["versionCode"])
        stratum = _stratum(newest)
        if stratum is None:
            continue
        by_code: dict[int, dict] = {}
        for v in vs:
            if v["file"]["size"] > max_file:
                continue
            code = v["manifest"]["versionCode"]
            best = by_code.get(code)
            if best is None or (v["file"]["size"], v["file"]["name"]) < (best["file"]["size"], best["file"]["name"]):
                by_code[code] = v
        picked = [by_code[c] for c in sorted(by_code, reverse=True)[:versions]]
        if len(picked) == versions:
            candidates[stratum].append((_order(seed, package), package, picked))
    out = []
    for s in STRATA:
        for _, package, picked in sorted(candidates[s])[:per_stratum]:
            out.extend(_entry(package, s, v) for v in picked)
    return out


def _toml_value(v) -> str:
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        # JSON's escapes are valid TOML except surrogate pairs, which ensure_ascii=False avoids.
        return json.dumps(v, ensure_ascii=False)
    if isinstance(v, list):
        return "[" + ", ".join(_toml_value(x) for x in v) + "]"
    raise TypeError(type(v))


def lock_toml(source: dict, entries: list[dict]) -> str:
    lines = ["# Written by `eval/fdroid.py select`. Do not edit: `fetch` downloads exactly this.", "version = 1", "", "[source]"]
    lines += [f"{k} = {_toml_value(v)}" for k, v in source.items()]
    for e in entries:
        lines += ["", "[[apk]]"] + [f"{k} = {_toml_value(v)}" for k, v in e.items()]
    return "\n".join(lines) + "\n"


def _get(url: str, limit: int) -> bytes:
    with urllib.request.urlopen(url, timeout=120) as resp:
        data = resp.read(limit + 1)
    if len(data) > limit:
        raise BudgetError(f"{url} is over {limit} bytes")
    return data


def cmd_select(args) -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    jar = CACHE / "entry.jar"
    jar.write_bytes(_get(f"{REPO}/entry.jar", 1_000_000))
    check_entry_jar(jar, FDROID_SIGNER_SHA256)
    entry = read_entry_json(jar)
    pin = entry["index"]
    raw = _get(f"{REPO}{pin['name']}", 200_000_000)
    if len(raw) != pin["size"] or hashlib.sha256(raw).hexdigest() != pin["sha256"]:
        raise VerifyError("index-v2.json does not match the size and sha256 in the signed entry.json")
    index = json.load(io.BytesIO(raw))
    entries = select(index, args.per_stratum, args.versions, args.max_file, args.seed)
    source = {
        "repo": REPO,
        "entry_signer_sha256": FDROID_SIGNER_SHA256,
        "entry_timestamp": entry["timestamp"],
        "index_sha256": pin["sha256"],
        "index_packages": pin.get("numPackages", len(index["packages"])),
        "seed": args.seed,
        "per_stratum": args.per_stratum,
        "versions_per_app": args.versions,
        "max_apk_bytes": args.max_file,
    }
    Path(args.lock).write_text(lock_toml(source, entries), encoding="utf-8")
    counts = Counter(e["stratum"] for e in entries)
    print(f"wrote {args.lock}: {len(entries)} APKs, {sum(e['size'] for e in entries) / 1e6:.0f} MB, {dict(counts)}")


# ---------------------------------------------------------------- analysis


def _xml_value(v):
    if not v or v.get("type") == "null":
        return None
    return v.get("value")


def _as_int(v):
    if isinstance(v, int):
        return v
    if isinstance(v, str) and v.strip().lstrip("-").isdigit():
        return int(v)
    return v


def cross_check(manifest: dict, entry: dict) -> dict:
    """Fields where our parsed manifest disagrees with F-Droid's index: {field: [ours, index]}.
    A versionName that is a resource reference is reported as `version_name_reference`: it
    cannot match without resources.arsc, which is not read."""
    diffs = {}
    if manifest.get("package") != entry["package"]:
        diffs["package"] = [manifest.get("package"), entry["package"]]
    code = _as_int(_xml_value(manifest.get("version_code")))
    if code != entry["version_code"]:
        diffs["version_code"] = [code, entry["version_code"]]
    vn = manifest.get("version_name") or {}
    if vn.get("type") == "reference":
        diffs["version_name_reference"] = [vn.get("value"), entry["version_name"]]
    elif _xml_value(vn) != entry["version_name"] and not (_xml_value(vn) is None and entry["version_name"] == ""):
        diffs["version_name"] = [_xml_value(vn), entry["version_name"]]
    min_sdk = _as_int(_xml_value(manifest.get("min_sdk")))
    min_sdk = 1 if min_sdk is None else min_sdk
    if min_sdk != entry["min_sdk"]:
        diffs["min_sdk"] = [min_sdk, entry["min_sdk"]]
    target = _as_int(_xml_value(manifest.get("target_sdk")))
    target = min_sdk if target is None else target
    if target != entry["target_sdk"]:
        diffs["target_sdk"] = [target, entry["target_sdk"]]
    perms = sorted(set(manifest.get("uses_permissions") or []))
    if perms != entry["permissions"]:
        ours, theirs = set(perms), set(entry["permissions"])
        diffs["permissions"] = [sorted(ours - theirs), sorted(theirs - ours)]
    return diffs


READ = "android.permission.READ_EXTERNAL_STORAGE"
WRITE = "android.permission.WRITE_EXTERNAL_STORAGE"


def classify(diff: dict, entry: dict) -> list[str]:
    """Names the kind of each cross_check difference. Two are additions the index makes, not
    parser errors (both checked on the APK bytes, NOTES.md 2026-11-01):
    - index_implied_read_permission: the APK declares only WRITE_EXTERNAL_STORAGE, and the index
      adds READ_EXTERNAL_STORAGE, which Android grants implicitly with it;
    - index_version_name_not_in_apk: the manifest has no versionName; the index's comes from
      F-Droid's build metadata."""
    out = []
    for field_name, (ours, theirs) in sorted(diff.items()):
        if field_name == "permissions" and ours == [] and theirs == [READ] and WRITE in entry["permissions"]:
            out.append("index_implied_read_permission")
        elif field_name == "version_name" and ours is None and entry["version_name"]:
            out.append("index_version_name_not_in_apk")
        else:
            out.append(field_name)
    return out


INDEX_ADDITIONS = {"index_implied_read_permission", "index_version_name_not_in_apk"}


def _key(e: dict) -> str:
    return f"{e['package']}_{e['version_code']}"


def summarize(entries: list[dict], reports: dict, diffs: dict, checks: dict, failures: dict) -> dict:
    """Aggregates per stratum. `reports` maps `_key` to a `dexpa sdks` report, `diffs` maps a
    package to its diff documents (oldest pair first), `checks` maps `_key` to cross_check output."""
    strata = {}
    for s in STRATA:
        es = [e for e in entries if e["stratum"] == s]
        apps = sorted({e["package"] for e in es})
        newest = {}
        for e in es:
            if _key(e) in reports and (e["package"] not in newest or e["version_code"] > newest[e["package"]]["version_code"]):
                newest[e["package"]] = e
        latest = [reports[_key(e)] for e in newest.values()]
        by_source = Counter()
        sdk_apps = Counter()
        for r in latest:
            sources = {ev["source"] for m in r["matches"] for ev in m["evidence"]}
            by_source.update(sources)
            sdk_apps.update({m["id"] for m in r["matches"]})
        events = Counter()
        for p in apps:
            for d in diffs.get(p, []):
                for ev in d["events"]:
                    events[f"{ev['event']}:{ev.get('confidence', '-')}"] += 1
        strata[s] = {
            "apps": len(apps),
            "apks": len(es),
            "analysed": sum(1 for e in es if _key(e) in reports),
            "failed": sum(1 for e in es if _key(e) in failures),
            "apps_with_any_sdk": sum(1 for r in latest if r["matches"]),
            "apps_by_evidence_source": dict(sorted(by_source.items())),
            "apps_by_sdk": dict(sorted(sdk_apps.items(), key=lambda kv: (-kv[1], kv[0]))),
            "median_classes": sorted(r["class_count"] for r in latest)[len(latest) // 2] if latest else None,
            "diff_events": dict(sorted(events.items())),
            "versions_found": dict(sorted(Counter(Counter(e["package"] for e in es).values()).items())),
        }
    by_key = {_key(e): e for e in entries}
    kinds = {k: classify(c, by_key[k]) for k, c in checks.items()}
    field_mismatch = Counter(f for ks in kinds.values() for f in ks)
    return {
        "strata": strata,
        "manifest_cross_check": {
            "apks": len(checks),
            "all_fields_equal": sum(1 for ks in kinds.values() if not ks),
            "parser_disagrees": sum(1 for ks in kinds.values() if set(ks) - INDEX_ADDITIONS),
            "mismatches_by_field": dict(sorted(field_mismatch.items())),
        },
        "failures": failures,
    }


def table_md(results: dict, lock_source: dict) -> str:
    st = results["strata"]
    cols = list(STRATA)
    def row(label, f):
        return f"| {label} | " + " | ".join(str(f(st[c])) for c in cols) + " |"
    lines = [
        "<!-- Written by `eval/fdroid.py run`. -->",
        "# F-Droid evaluation of `dexpa sdks`",
        "",
        f"Index `{lock_source['index_sha256'][:12]}` (entry.json timestamp {lock_source['entry_timestamp']}), seed `{lock_source['seed']}`. Per app, the newest version is used for presence counts; diffs are between consecutive selected versions.",
        "",
        "| | " + " | ".join(cols) + " |",
        "|---|" + "---:|" * len(cols),
        row("apps", lambda s: s["apps"]),
        row("APKs analysed / selected", lambda s: f"{s['analysed']} / {s['apks']}"),
        row("APKs failed", lambda s: s["failed"]),
        row("apps with any SDK", lambda s: s["apps_with_any_sdk"]),
    ]
    sources = sorted({k for s in st.values() for k in s["apps_by_evidence_source"]})
    for src in sources:
        lines.append(row(f"apps with `{src}` evidence", lambda s, src=src: s["apps_by_evidence_source"].get(src, 0)))
    lines.append(row("median defined classes", lambda s: s["median_classes"]))
    kinds = sorted({k for s in st.values() for k in s["diff_events"]})
    for k in kinds:
        lines.append(row(f"diff `{k}`", lambda s, k=k: s["diff_events"].get(k, 0)))
    lines += ["", "## SDKs by stratum (apps, newest version)", "", "| SDK | " + " | ".join(cols) + " |", "|---|" + "---:|" * len(cols)]
    ids = sorted({i for s in st.values() for i in s["apps_by_sdk"]})
    for i in ids:
        lines.append(f"| `{i}` | " + " | ".join(str(st[c]["apps_by_sdk"].get(i, 0)) for c in cols) + " |")
    cc = results["manifest_cross_check"]
    lines += [
        "",
        "## Manifest parser against index-v2",
        "",
        f"{cc['all_fields_equal']} of {cc['apks']} APKs agree on every field (package, versionCode, versionName, min/target SDK, permissions). "
        f"On {cc['parser_disagrees']} the parser disagrees with the index for a reason other than the index's own additions (`index_*` below).",
        "",
    ]
    for f, n in cc["mismatches_by_field"].items():
        lines.append(f"- `{f}`: {n}")
    if results["failures"]:
        lines += ["", "## Failures", ""] + [f"- `{k}`: {v}" for k, v in sorted(results["failures"].items())]
    return "\n".join(lines) + "\n"


def _json_cmd(cmd: list[str], timeout: float = TOOL_TIMEOUT_SECONDS) -> dict:
    """The JSON a tool prints. Every way it can fail is a RuntimeError, which the caller records
    as that APK's (or that pair's) failure."""
    name = Path(cmd[0]).name
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"{name} timed out after {timeout} s") from None
    except OSError as err:  # the tool exists but cannot start
        raise RuntimeError(f"{name} did not start: {err}") from None
    if r.returncode != 0:
        raise RuntimeError(f"{name} exited {r.returncode}: {r.stderr.strip()[:300]}")
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError as err:
        raise RuntimeError(f"{name} printed something that is not JSON: {err}") from None


def _budget(args) -> Budget:
    return Budget(max_file=args.max_file, max_total=args.max_total, seconds=args.minutes * 60, min_free=args.min_free, pause=args.pause)


def load_lock(path) -> dict:
    lock = tomllib.loads(Path(path).read_text(encoding="utf-8"))
    if lock.get("version") != 1:
        raise VerifyError(f"{path}: unknown lock version {lock.get('version')}")
    return lock


def cmd_fetch(args) -> None:
    lock = load_lock(args.lock)
    state = FetchState(_budget(args))
    for e in lock["apk"]:
        ensure(e, Path(args.dest), args.base_url, state)
    print(f"{len(lock['apk'])} APKs verified in {args.dest}; downloaded {state.downloaded} bytes")


def check_tools(*paths) -> None:
    """Fail before downloading anything if an analysis tool is missing."""
    for p in paths:
        if not (Path(p).is_file() and os.access(p, os.X_OK)):
            raise VerifyError(f"{p} is not an executable file; build it first (just fdroid-eval does)")


def cmd_run(args) -> None:
    check_tools(args.dexpa, args.manifest_tool)
    check_base_url(args.base_url)  # once, before the loop records it as every APK's failure
    lock = load_lock(args.lock)
    out = Path(args.out)
    reports_dir = out / "reports"
    reports_dir.mkdir(parents=True, exist_ok=True)
    state = FetchState(_budget(args))
    reports, checks, failures = {}, {}, {}
    sdks_cmd = [args.dexpa, "sdks"] + (["--exodus", args.exodus] if args.exodus else [])
    # reports/ holds this run's reports only. Every report is rewritten when its APK is analysed,
    # so one left from an earlier run (another lock, an APK that now fails, one an aborted run
    # never reached) could only disagree with results.json.
    for old in reports_dir.glob("*.json"):
        old.unlink()
    aborted = None
    for e in lock["apk"]:
        k = _key(e)
        report_file = reports_dir / f"{k}.json"
        try:
            path = ensure(e, Path(args.dest), args.base_url, state)
        except BudgetError as err:
            aborted = str(err)
            break
        # A 404, a timeout, a hash that does not match, or a broken response (http.client's
        # IncompleteRead and BadStatusLine are not OSError).
        except (VerifyError, OSError, http.client.HTTPException) as err:
            failures[k] = f"fetch: {type(err).__name__}: {err}"
            print(f"{k}: not fetched: {err}", flush=True)
            continue
        try:
            report = _json_cmd(sdks_cmd + [str(path), "--format", "json"], args.tool_timeout)
            check = cross_check(_json_cmd([args.manifest_tool, str(path)], args.tool_timeout), e)
        except RuntimeError as err:
            # Both analyses or neither: a report without its cross-check would count twice.
            failures[k] = str(err)
        else:
            reports[k], checks[k] = report, check
            report_file.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        finally:
            if not args.keep:
                path.unlink(missing_ok=True)
        print(f"{k}: {len(reports.get(k, {}).get('matches', []))} SDKs, {len(checks.get(k, {}))} manifest mismatches", flush=True)
    diffs = defaultdict(list)
    for p in sorted({e["package"] for e in lock["apk"]}):
        ks = [_key(e) for e in sorted((e for e in lock["apk"] if e["package"] == p), key=lambda e: e["version_code"]) if _key(e) in reports]
        for a, b in zip(ks, ks[1:]):
            try:
                diffs[p].append(_json_cmd([args.dexpa, "sdks", "diff", str(reports_dir / f"{a}.json"), str(reports_dir / f"{b}.json"), "--format", "json"], args.tool_timeout))
            except RuntimeError as err:
                failures[f"diff {a} {b}"] = str(err)
    results = summarize(lock["apk"], reports, diffs, checks, failures)
    results["downloaded_bytes"] = state.downloaded
    results["aborted"] = aborted
    results["diffs"] = {p: [d["events"] for d in ds] for p, ds in diffs.items() if any(d["events"] for d in ds)}
    results["manifest_mismatches"] = {k: c for k, c in checks.items() if c}
    (out / "results.json").write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    (out / "TABLE.md").write_text(table_md(results, lock["source"]))
    print(f"wrote {out / 'results.json'} and {out / 'TABLE.md'}; downloaded {state.downloaded} bytes")
    if aborted:
        raise BudgetError(aborted)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("select")
    s.add_argument("--lock", default=str(ROOT / "eval" / "fdroid.lock.toml"))
    s.add_argument("--per-stratum", type=int, default=10)
    s.add_argument("--versions", type=int, default=3)
    s.add_argument("--max-file", type=int, default=MAX_FILE_BYTES)
    s.add_argument("--seed", default="dexpa-m6-2026-11-01")
    for name in ("fetch", "run"):
        p = sub.add_parser(name)
        p.add_argument("--lock", default=str(ROOT / "eval" / "fdroid.lock.toml"))
        p.add_argument("--dest", default=str(CACHE / "apks"))
        p.add_argument("--base-url", default=REPO)
        p.add_argument("--max-file", type=int, default=MAX_FILE_BYTES)
        p.add_argument("--max-total", type=int, default=MAX_TOTAL_BYTES)
        p.add_argument("--minutes", type=float, default=MAX_MINUTES)
        p.add_argument("--min-free", type=int, default=MIN_FREE_BYTES)
        p.add_argument("--pause", type=float, default=PAUSE_SECONDS)
    r = sub.choices["run"]
    r.add_argument("--dexpa", required=True)
    r.add_argument("--manifest-tool", required=True)
    r.add_argument("--exodus")
    r.add_argument("--out", default=str(ROOT / "eval"))
    r.add_argument("--keep", action="store_true")
    r.add_argument("--tool-timeout", type=float, default=TOOL_TIMEOUT_SECONDS, help="seconds per dexpa or manifest-tool call")
    args = ap.parse_args(argv)
    try:
        {"select": cmd_select, "fetch": cmd_fetch, "run": cmd_run}[args.cmd](args)
    except VerifyError as err:
        print(f"error: {err}", file=sys.stderr)
        return 1
    except BudgetError as err:
        print(f"budget: {err}", file=sys.stderr)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
