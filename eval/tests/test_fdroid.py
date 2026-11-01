# Modified by Zero (2026-11-01): new test file (fork M6); run-loop tests (2026-11-02, /review, re-review). See NOTICE.
"""Offline tests for eval/fdroid.py. No network: the "repo" is a temp directory served as file://.

Run: python3 -m unittest discover -s eval/tests   (or `just eval-test`).
"""

import hashlib
import json
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import fdroid  # noqa: E402


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class Repo:
    """A temp dir with APK files, plus lock entries pointing at them."""

    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.repo = self.root / "repo"
        self.dest = self.root / "dest"
        self.repo.mkdir()
        self.base = self.repo.as_uri()

    def add(self, path: str, data: bytes, **over) -> dict:
        (self.repo / path.lstrip("/")).write_bytes(data)
        entry = {
            "package": "org.example",
            "stratum": "control",
            "version_code": 1,
            "version_name": "1.0",
            "name": path,
            "sha256": sha(data),
            "size": len(data),
            "anti_features": [],
            "min_sdk": 21,
            "target_sdk": 34,
            "permissions": [],
        }
        entry.update(over)
        return entry

    def budget(self, **over) -> "fdroid.Budget":
        b = dict(max_file=1000, max_total=10_000, seconds=60, min_free=0, pause=0)
        b.update(over)
        return fdroid.Budget(**b)

    def close(self):
        self.tmp.cleanup()


class FetchTest(unittest.TestCase):
    def setUp(self):
        self.r = Repo()

    def tearDown(self):
        self.r.close()

    def test_good_file_is_fetched_and_verified(self):
        e = self.r.add("/a_1.apk", b"A" * 100)
        state = fdroid.FetchState(self.r.budget())
        path = fdroid.ensure(e, self.r.dest, self.r.base, state)
        self.assertEqual(path.read_bytes(), b"A" * 100)
        self.assertEqual(state.downloaded, 100)
        self.assertEqual(list(self.r.dest.glob("*.part")), [])

    def test_wrong_sha256_is_rejected_and_the_partial_removed(self):
        e = self.r.add("/a_1.apk", b"A" * 100, sha256="0" * 64)
        state = fdroid.FetchState(self.r.budget())
        with self.assertRaises(fdroid.VerifyError):
            fdroid.ensure(e, self.r.dest, self.r.base, state)
        self.assertEqual(list(self.r.dest.iterdir()), [])

    def test_size_mismatch_is_rejected(self):
        e = self.r.add("/a_1.apk", b"A" * 100, size=99)
        with self.assertRaises(fdroid.VerifyError):
            fdroid.ensure(e, self.r.dest, self.r.base, fdroid.FetchState(self.r.budget()))
        self.assertEqual(list(self.r.dest.iterdir()), [])

    def test_a_verified_file_is_not_downloaded_again(self):
        e = self.r.add("/a_1.apk", b"A" * 100)
        fdroid.ensure(e, self.r.dest, self.r.base, fdroid.FetchState(self.r.budget()))
        (self.r.repo / "a_1.apk").unlink()  # the source is gone: a second fetch would fail
        state = fdroid.FetchState(self.r.budget())
        fdroid.ensure(e, self.r.dest, self.r.base, state)
        self.assertEqual(state.downloaded, 0)

    def test_a_corrupt_local_copy_is_replaced(self):
        e = self.r.add("/a_1.apk", b"A" * 100)
        self.r.dest.mkdir()
        (self.r.dest / "a_1.apk").write_bytes(b"B" * 100)
        path = fdroid.ensure(e, self.r.dest, self.r.base, fdroid.FetchState(self.r.budget()))
        self.assertEqual(path.read_bytes(), b"A" * 100)

    def test_total_budget_exhaustion_aborts_before_downloading(self):
        a = self.r.add("/a_1.apk", b"A" * 600)
        b = self.r.add("/b_1.apk", b"B" * 600, package="org.b")
        state = fdroid.FetchState(self.r.budget(max_total=1000))
        fdroid.ensure(a, self.r.dest, self.r.base, state)
        with self.assertRaises(fdroid.BudgetError):
            fdroid.ensure(b, self.r.dest, self.r.base, state)
        self.assertFalse((self.r.dest / "b_1.apk").exists())

    def test_per_file_cap_refuses_a_large_entry(self):
        e = self.r.add("/a_1.apk", b"A" * 2000)
        with self.assertRaises(fdroid.BudgetError):
            fdroid.ensure(e, self.r.dest, self.r.base, fdroid.FetchState(self.r.budget()))

    def test_a_body_longer_than_declared_is_cut_off(self):
        # The lock says 100 bytes; the server sends 900. Reading stops at 101, then fails.
        e = self.r.add("/a_1.apk", b"A" * 900, size=100, sha256=sha(b"A" * 100))
        state = fdroid.FetchState(self.r.budget())
        with self.assertRaises(fdroid.VerifyError):
            fdroid.ensure(e, self.r.dest, self.r.base, state)
        self.assertLessEqual(state.downloaded, 101)
        self.assertEqual(list(self.r.dest.iterdir()), [])

    def test_low_disk_aborts(self):
        e = self.r.add("/a_1.apk", b"A" * 100)
        with self.assertRaises(fdroid.BudgetError):
            fdroid.ensure(
                e, self.r.dest, self.r.base, fdroid.FetchState(self.r.budget(min_free=1 << 62))
            )

    def test_time_budget_aborts(self):
        e = self.r.add("/a_1.apk", b"A" * 100)
        state = fdroid.FetchState(self.r.budget(seconds=0))
        with self.assertRaises(fdroid.BudgetError):
            fdroid.ensure(e, self.r.dest, self.r.base, state)

    def test_names_that_leave_the_directory_are_rejected(self):
        for bad in ["/../x.apk", "/a/b.apk", "x.apk", "/a.apk.sh", "/.apk"]:
            e = self.r.add("/ok.apk", b"A", name=bad)
            with self.assertRaises(fdroid.VerifyError, msg=bad):
                fdroid.ensure(e, self.r.dest, self.r.base, fdroid.FetchState(self.r.budget()))

    def test_only_https_or_file_base_urls(self):
        e = self.r.add("/a_1.apk", b"A")
        with self.assertRaises(fdroid.VerifyError):
            fdroid.ensure(e, self.r.dest, "http://f-droid.org/repo", fdroid.FetchState(self.r.budget()))


def version(code, size, af=None, name=None, nativecode=None):
    v = {
        "file": {"name": name or f"/p_{code}_{size}.apk", "sha256": f"{code:064x}", "size": size},
        "manifest": {
            "versionCode": code,
            "versionName": f"v{code}",
            "usesSdk": {"minSdkVersion": 21, "targetSdkVersion": 34},
            "usesPermission": [{"name": "android.permission.INTERNET"}],
            "usesPermissionSdk23": [{"name": "android.permission.CAMERA"}],
        },
    }
    if af is not None:
        v["antiFeatures"] = {k: {} for k in af}
    if nativecode:
        v["manifest"]["nativecode"] = nativecode
    return v


def index(packages):
    return {"repo": {}, "packages": {p: {"metadata": {}, "versions": {str(i): v for i, v in enumerate(vs)}} for p, vs in packages.items()}}


class SelectTest(unittest.TestCase):
    def sample(self):
        pk = {}
        for i in range(5):
            pk[f"org.track{i}"] = [version(c, 10, af=["Tracking"]) for c in (1, 2, 3, 4)]
            pk[f"org.net{i}"] = [version(c, 10, af=["NonFreeNet"]) for c in (1, 2, 3)]
            pk[f"org.clean{i}"] = [version(c, 10, af=[]) for c in (1, 2, 3)]
        pk["org.big"] = [version(c, 10_000, af=["Tracking"]) for c in (1, 2, 3)]
        pk["org.two"] = [version(c, 10) for c in (1, 2)]
        pk["org.mixed"] = [version(c, 10, af=["NonFreeAdd"]) for c in (1, 2, 3)]
        return index(pk)

    def test_strata_versions_and_caps(self):
        entries = fdroid.select(self.sample(), per_stratum=2, versions=3, max_file=1000, seed="s")
        by = {}
        for e in entries:
            by.setdefault(e["stratum"], set()).add(e["package"])
        self.assertEqual({k: len(v) for k, v in by.items()}, {"Tracking": 2, "NonFreeNet": 2, "control": 2})
        pkgs = {e["package"] for e in entries}
        self.assertNotIn("org.big", pkgs, "over the per-file cap")
        self.assertNotIn("org.two", pkgs, "fewer than 3 versions")
        self.assertNotIn("org.mixed", pkgs, "another anti-feature: neither stratum nor control")
        track = sorted(e["version_code"] for e in entries if e["stratum"] == "Tracking")
        self.assertEqual(track, [2, 2, 3, 3, 4, 4], "the 3 newest versions")

    def test_selection_is_deterministic_and_seeded(self):
        a = fdroid.select(self.sample(), 2, 3, 1000, "s")
        self.assertEqual(a, fdroid.select(self.sample(), 2, 3, 1000, "s"))
        others = [fdroid.select(self.sample(), 2, 3, 1000, f"t{i}") for i in range(5)]
        self.assertTrue(any(o != a for o in others), "the seed changes the sample")

    def test_one_apk_per_version_code_the_smallest(self):
        pk = {"org.abi": [version(1, 50, name="/arm.apk", nativecode=["arm64-v8a"]), version(1, 40, name="/x86.apk", nativecode=["x86_64"]), version(2, 10), version(3, 10)]}
        entries = fdroid.select(index(pk), 1, 3, 1000, "s")
        v1 = [e for e in entries if e["version_code"] == 1]
        self.assertEqual([e["name"] for e in v1], ["/x86.apk"])

    def test_lock_round_trips_through_toml(self):
        entries = fdroid.select(self.sample(), 1, 3, 1000, "s")
        source = {"repo": "https://f-droid.org/repo", "index_sha256": "ab" * 32, "seed": "s \"q\" é"}
        text = fdroid.lock_toml(source, entries)
        lock = tomllib.loads(text)
        self.assertEqual(lock["version"], 1)
        self.assertEqual(lock["source"], source)
        self.assertEqual(lock["apk"], entries)
        self.assertEqual(entries[0]["permissions"], ["android.permission.CAMERA", "android.permission.INTERNET"])

    def test_lockfile_keeps_characters_outside_the_basic_plane(self):
        """Re-review (2026-11-02): JSON's surrogate-pair escapes are not valid TOML."""
        entries = [dict(package="p", stratum="control", version_code=1,
                        version_name="1.0 \U0001F600", name="/p_1.apk", sha256="0" * 64, size=1,
                        anti_features=[], min_sdk=21, target_sdk=34, permissions=[])]
        source = {"index_sha256": "ab" * 32, "entry_timestamp": 1, "seed": "s"}
        lock = tomllib.loads(fdroid.lock_toml(source, entries))
        self.assertEqual(lock["apk"][0]["version_name"], "1.0 \U0001F600")


class EntryJarTest(unittest.TestCase):
    FP = "43:23:8D:51:2C:1E:5E:B2:D6:56:9F:4A:3A:FB:F5:52:34:18:B8:2E:0A:3E:D1:55:27:70:AB:B9:A9:C9:CC:AB"

    def check(self, verify_out, cert_out):
        def runner(cmd):
            return verify_out if cmd[0] == "jarsigner" else cert_out
        return fdroid.check_entry_jar("entry.jar", fdroid.FDROID_SIGNER_SHA256, runner)

    def test_verified_jar_with_the_pinned_signer(self):
        self.check("jar verified.\n\nWarning: self-signed", f"\t SHA256: {self.FP}\n")

    def test_unsigned_jar(self):
        with self.assertRaises(fdroid.VerifyError):
            self.check("jar is unsigned.\n", "")

    def test_other_signer(self):
        with self.assertRaises(fdroid.VerifyError):
            self.check("jar verified.\n", "\t SHA256: " + ":".join(["00"] * 32) + "\n")

    def test_two_signers(self):
        with self.assertRaises(fdroid.VerifyError):
            self.check("jar verified.\n", f"\t SHA256: {self.FP}\n\t SHA256: {self.FP}\n")


class EntryJsonTest(unittest.TestCase):
    """/cso gap (2026-11-02): jarsigner (Java) and zipfile (Python) may pick different copies of a
    duplicated name, so a jar with duplicate entry names is refused outright."""

    def jar(self, entries):
        import warnings
        import zipfile
        d = tempfile.TemporaryDirectory()
        self.addCleanup(d.cleanup)
        path = Path(d.name) / "entry.jar"
        with warnings.catch_warnings(), zipfile.ZipFile(path, "w") as z:
            warnings.simplefilter("ignore")
            for name, data in entries:
                z.writestr(name, data)
        return path

    def test_reads_entry_json(self):
        self.assertEqual(fdroid.read_entry_json(self.jar([("META-INF/MANIFEST.MF", "x"), ("entry.json", '{"a": 1}')])), {"a": 1})

    def test_duplicate_names_are_refused(self):
        path = self.jar([("entry.json", '{"a": 1}'), ("entry.json", '{"a": 2}')])
        with self.assertRaises(fdroid.VerifyError):
            fdroid.read_entry_json(path)


class CrossCheckTest(unittest.TestCase):
    def entry(self, **over):
        e = {"package": "org.x", "version_code": 7, "version_name": "1.7", "min_sdk": 21, "target_sdk": 34, "permissions": ["a.B", "c.D"]}
        e.update(over)
        return e

    def manifest(self, **over):
        m = {
            "package": "org.x",
            "version_code": {"type": "int", "value": 7},
            "version_name": {"type": "string", "value": "1.7"},
            "min_sdk": {"type": "int", "value": 21},
            "target_sdk": {"type": "int", "value": 34},
            "uses_permissions": ["c.D", "a.B", "a.B"],
        }
        m.update(over)
        return m

    def test_equal(self):
        self.assertEqual(fdroid.cross_check(self.manifest(), self.entry()), {})

    def test_each_field_is_compared(self):
        got = fdroid.cross_check(
            self.manifest(package="org.y", target_sdk={"type": "int", "value": 33}, uses_permissions=["a.B"]),
            self.entry(),
        )
        self.assertEqual(set(got), {"package", "target_sdk", "permissions"})

    def test_reference_version_name_is_its_own_category(self):
        got = fdroid.cross_check(self.manifest(version_name={"type": "reference", "value": 2131755037}), self.entry())
        self.assertEqual(set(got), {"version_name_reference"})

    def test_missing_min_sdk_means_one(self):
        # Android's default; the index records 1 when uses-sdk is absent.
        self.assertEqual(fdroid.cross_check(self.manifest(min_sdk=None), self.entry(min_sdk=1)), {})



class ClassifyTest(unittest.TestCase):
    E = {"permissions": ["android.permission.READ_EXTERNAL_STORAGE", "android.permission.WRITE_EXTERNAL_STORAGE"], "version_name": "v0.5.7"}

    def test_read_implied_by_write_is_an_index_addition(self):
        diff = {"permissions": [[], ["android.permission.READ_EXTERNAL_STORAGE"]]}
        self.assertEqual(fdroid.classify(diff, self.E), ["index_implied_read_permission"])

    def test_read_missing_without_write_is_still_a_mismatch(self):
        e = {"permissions": ["android.permission.READ_EXTERNAL_STORAGE"], "version_name": ""}
        diff = {"permissions": [[], ["android.permission.READ_EXTERNAL_STORAGE"]]}
        self.assertEqual(fdroid.classify(diff, e), ["permissions"])

    def test_other_permission_differences_stay_mismatches(self):
        diff = {"permissions": [["a.X"], ["android.permission.READ_EXTERNAL_STORAGE"]]}
        self.assertEqual(fdroid.classify(diff, self.E), ["permissions"])

    def test_version_name_absent_from_the_apk(self):
        self.assertEqual(fdroid.classify({"version_name": [None, "v0.5.7"]}, self.E), ["index_version_name_not_in_apk"])
        self.assertEqual(fdroid.classify({"version_name": ["1.0", "v0.5.7"]}, self.E), ["version_name"])

    def test_summary_counts_parser_mismatches_apart_from_index_additions(self):
        entries = [{"package": "p", "stratum": "control", "version_code": 1, **self.E}]
        checks = {"p_1": {"permissions": [[], ["android.permission.READ_EXTERNAL_STORAGE"]]}}
        cc = fdroid.summarize(entries, {}, {}, checks, {})["manifest_cross_check"]
        self.assertEqual(cc["all_fields_equal"], 0)
        self.assertEqual(cc["parser_disagrees"], 0)
        self.assertEqual(cc["mismatches_by_field"], {"index_implied_read_permission": 1})


class SummarizeTest(unittest.TestCase):
    def test_counts_use_the_newest_version_and_every_diff(self):
        def e(pkg, stratum, code):
            return {"package": pkg, "stratum": stratum, "version_code": code}

        def rep(*matches, classes=10):
            return {"class_count": classes, "matches": [{"id": i, "evidence": [{"source": s} for s in srcs]} for i, srcs in matches]}

        entries = [e("t", "Tracking", 1), e("t", "Tracking", 2), e("c", "control", 5), e("c", "control", 6)]
        reports = {
            "t_1": rep(),  # older version: no SDK, must not count
            "t_2": rep(("sentry", ["dex_class", "manifest_meta"]), ("acra", ["dex_class"]), classes=30),
            "c_5": rep(classes=7),
        }
        diffs = {"t": [{"events": [{"event": "sdk_added", "id": "sentry", "confidence": "strong"}, {"event": "sdk_added", "id": "acra", "confidence": "uncertain"}]}]}
        checks = {"t_1": {}, "t_2": {"version_name_reference": [1, "x"]}, "c_5": {}}
        r = fdroid.summarize(entries, reports, diffs, checks, {"c_6": "boom"})
        t, c = r["strata"]["Tracking"], r["strata"]["control"]
        self.assertEqual((t["apps"], t["apks"], t["analysed"], t["failed"]), (1, 2, 2, 0))
        self.assertEqual(t["apps_with_any_sdk"], 1)
        self.assertEqual(t["apps_by_evidence_source"], {"dex_class": 1, "manifest_meta": 1})
        self.assertEqual(t["apps_by_sdk"], {"acra": 1, "sentry": 1})
        self.assertEqual(t["median_classes"], 30)
        self.assertEqual(t["diff_events"], {"sdk_added:strong": 1, "sdk_added:uncertain": 1})
        self.assertEqual(t["versions_found"], {2: 1})
        self.assertEqual((c["analysed"], c["failed"], c["apps_with_any_sdk"]), (1, 1, 0))
        self.assertEqual(r["strata"]["NonFreeNet"]["apps"], 0)
        self.assertEqual(r["manifest_cross_check"], {"apks": 3, "all_fields_equal": 2, "parser_disagrees": 1, "mismatches_by_field": {"version_name_reference": 1}})
        md = fdroid.table_md(r, {"index_sha256": "ab" * 32, "entry_timestamp": 1, "seed": "s"})
        self.assertIn("| apps with any SDK | 1 | 0 | 0 |", md)
        self.assertIn("| `sentry` | 1 | 0 | 0 |", md)


class RunTest(unittest.TestCase):
    def test_missing_tools_fail_before_any_download(self):
        with tempfile.TemporaryDirectory() as d:
            tool = Path(d) / "tool"
            with self.assertRaises(fdroid.VerifyError):
                fdroid.check_tools(str(tool))
            tool.write_text("#!/bin/sh\n")
            with self.assertRaises(fdroid.VerifyError, msg="not executable"):
                fdroid.check_tools(str(tool))
            tool.chmod(0o755)
            fdroid.check_tools(str(tool))
            rc = fdroid.main(["run", "--dexpa", str(Path(d) / "nope"), "--manifest-tool", str(tool), "--dest", d])
            self.assertEqual(rc, 1)


FAKE_DEXPA = """#!/bin/sh
# Stands in for `dexpa`: `sdks diff A B ...` prints no events, `sdks FILE ...` a minimal report.
if [ "$2" = "diff" ]; then echo '{"events": []}'; exit 0; fi
case "$2" in *bad*) echo "boom" >&2; exit 1;; esac
echo '{"matches": [], "class_count": 1}'
"""

BAD_DIFF_DEXPA = """#!/bin/sh
# Like FAKE_DEXPA, but `sdks diff` prints something that is not JSON.
if [ "$2" = "diff" ]; then echo 'not json'; exit 0; fi
echo '{"matches": [], "class_count": 1}'
"""

FAKE_MANIFEST = """#!/bin/sh
case "$1" in *nomanifest*) echo "no manifest" >&2; exit 1;; esac
echo '{"package": "org.example", "version_code": {"type": "int", "value": 1}, "version_name": {"type": "string", "value": "1.0"}, "min_sdk": {"type": "int", "value": 21}, "target_sdk": {"type": "int", "value": 34}, "uses_permissions": []}'
"""

NO_INTERPRETER = """#!/nonexistent/interpreter
"""

SLOW_TOOL = """#!/bin/sh
sleep 5
"""


class RunLoopTest(unittest.TestCase):
    """/review (2026-11-02): `run` end to end over a file:// repo, with stand-in tools."""

    def setUp(self):
        self.r = Repo()
        self.addCleanup(self.r.close)
        self.out = self.r.root / "out"

    def tool(self, name, text):
        path = self.r.root / name
        path.write_text(text)
        path.chmod(0o755)
        return str(path)

    def run_main(self, entries, dexpa=FAKE_DEXPA, manifest=FAKE_MANIFEST, extra=()):
        lock = self.r.root / "lock.toml"
        lock.write_text(fdroid.lock_toml({"index_sha256": "ab" * 32, "entry_timestamp": 1, "seed": "s"}, entries))
        return fdroid.main([
            "run", "--lock", str(lock), "--dest", str(self.r.dest), "--base-url", self.r.base,
            "--out", str(self.out), "--min-free", "0", "--pause", "0",
            "--dexpa", self.tool("dexpa", dexpa), "--manifest-tool", self.tool("manifest", manifest), *extra,
        ])

    def results(self):
        return json.loads((self.out / "results.json").read_text())

    def test_a_failed_fetch_is_recorded_and_the_run_goes_on(self):
        good = self.r.add("/org.example_1.apk", b"A" * 10, version_code=1)
        gone = self.r.add("/org.example_2.apk", b"B" * 10, version_code=2)
        (self.r.repo / "org.example_2.apk").unlink()  # a 404 on the real repo
        wrong = self.r.add("/org.example_3.apk", b"C" * 10, version_code=3, sha256="0" * 64)
        self.assertEqual(self.run_main([good, gone, wrong]), 0)
        res = self.results()
        self.assertEqual(sorted(res["failures"]), ["org.example_2", "org.example_3"])
        self.assertEqual(res["strata"]["control"]["analysed"], 1)
        self.assertTrue((self.out / "TABLE.md").exists())

    def test_a_failed_fetch_removes_its_old_report(self):
        gone = self.r.add("/org.example_2.apk", b"B" * 10, version_code=2)
        (self.r.repo / "org.example_2.apk").unlink()
        reports = self.out / "reports"
        reports.mkdir(parents=True)
        (reports / "org.example_2.json").write_text("{}")  # from an earlier run
        self.assertEqual(self.run_main([gone]), 0)
        self.assertFalse((reports / "org.example_2.json").exists())

    def test_an_aborted_run_leaves_no_report_it_did_not_write(self):
        a = self.r.add("/org.example_1.apk", b"A" * 10, version_code=1)
        b = self.r.add("/org.example_2.apk", b"B" * 500, version_code=2)
        reports = self.out / "reports"
        reports.mkdir(parents=True)
        (reports / "org.example_2.json").write_text("{}")  # from an earlier run
        self.assertEqual(self.run_main([a, b], extra=("--max-total", "100")), 3)
        self.assertEqual(sorted(p.name for p in reports.iterdir()), ["org.example_1.json"])
        self.assertIsNotNone(self.results()["aborted"])

    def test_a_diff_that_is_not_json_is_that_pairs_failure(self):
        a = self.r.add("/org.example_1.apk", b"A" * 10, version_code=1)
        b = self.r.add("/org.example_2.apk", b"B" * 10, version_code=2)
        self.assertEqual(self.run_main([a, b], dexpa=BAD_DIFF_DEXPA), 0)
        self.assertEqual(list(self.results()["failures"]), ["diff org.example_1 org.example_2"])

    def test_a_tool_that_cannot_start_is_that_apks_failure(self):
        e = self.r.add("/org.example_1.apk", b"A" * 10)
        self.assertEqual(self.run_main([e], manifest=NO_INTERPRETER), 0)
        self.assertIn("org.example_1", self.results()["failures"])

    def test_a_hung_tool_times_out_as_that_apks_failure(self):
        e = self.r.add("/org.example_1.apk", b"A" * 10)
        self.assertEqual(self.run_main([e], dexpa=SLOW_TOOL, extra=("--tool-timeout", "0.5")), 0)
        self.assertIn("timed out", self.results()["failures"]["org.example_1"])

    def test_a_manifest_failure_is_a_failure_only(self):
        e = self.r.add("/nomanifest_1.apk", b"A" * 10, package="nomanifest")
        self.assertEqual(self.run_main([e]), 0)
        st = self.results()["strata"]["control"]
        self.assertEqual((st["analysed"], st["failed"]), (0, 1), "not counted as both")
        self.assertFalse((self.out / "reports" / "nomanifest_1.json").exists())

    def test_a_refused_base_url_stops_the_run_instead_of_failing_each_apk(self):
        e = self.r.add("/org.example_1.apk", b"A" * 10)
        self.r.base = "http://f-droid.org/repo"
        self.assertEqual(self.run_main([e]), 1)

    def test_reports_from_earlier_runs_do_not_survive(self):
        e = self.r.add("/bad_1.apk", b"A" * 10, package="bad")
        reports = self.out / "reports"
        reports.mkdir(parents=True)
        (reports / "bad_1.json").write_text("{}")  # this APK's report from an earlier run
        (reports / "gone.app_7.json").write_text("{}")  # an app no longer in the lock
        self.assertEqual(self.run_main([e]), 0)
        self.assertEqual(list(reports.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
