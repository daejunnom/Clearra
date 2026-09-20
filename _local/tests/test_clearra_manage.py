from __future__ import annotations

import importlib.util
import json
import os
import pathlib
import sys
import tempfile
import unittest
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("clearra_manage", ROOT / "_local" / "clearra_manage.py")
assert SPEC and SPEC.loader
MANAGE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MANAGE
SPEC.loader.exec_module(MANAGE)


class ManagementPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.policy = MANAGE.load_policy()

    def test_manifest_binds_exact_release_toolchains(self) -> None:
        self.assertEqual(
            self.policy["toolchains"],
            {
                "node": "22.23.2",
                "npm": "10.9.8",
                "pnpm": "11.5.0",
                "rust": "1.98.1",
                "cargo": "1.98.1",
                "wasm_bindgen": "0.2.126",
            },
        )
        self.assertEqual(self.policy["canonical_source"]["release_run_id"], 35508135887)

    def test_repository_roots_accept_only_descendants(self) -> None:
        self.assertTrue(MANAGE.allowed_repository_path(ROOT / "coverage" / "cargo" / "report", self.policy))
        self.assertTrue(MANAGE.allowed_repository_path(ROOT / "_local" / "artifacts" / "test" / "one", self.policy))
        self.assertFalse(MANAGE.allowed_repository_path(ROOT / "crates" / "generated.bin", self.policy))
        self.assertFalse(MANAGE.allowed_repository_path(ROOT.parent / "coverage" / "escape", self.policy))
        self.assertFalse(MANAGE.allowed_repository_path(ROOT / "docs" / "research" / "raw.json", self.policy))

    def test_relative_escape_atomic_temp_case_and_unc_policy(self) -> None:
        atomic = ROOT / "_local" / "tmp" / "management" / "run" / "result.tmp"
        self.assertEqual(MANAGE.assert_output_path(atomic, self.policy), atomic.resolve(strict=False))
        with self.assertRaises(MANAGE.ManagementError):
            MANAGE.assert_output_path(ROOT / "build" / ".." / ".." / "escaped.bin", self.policy)
        self.assertFalse(
            MANAGE.allowed_repository_path(pathlib.Path(r"\\server\share\clearra-output"), self.policy)
        )
        if os.name == "nt":
            case_variant = pathlib.Path(str(ROOT / "build" / "cargo").upper())
            self.assertTrue(MANAGE.allowed_repository_path(case_variant, self.policy))

    def test_link_escape_is_rejected_when_supported(self) -> None:
        parent = ROOT / "_local" / "tmp" / "management-tests"
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            link = pathlib.Path(directory) / "outside-link"
            try:
                link.symlink_to(ROOT / "crates", target_is_directory=True)
            except OSError as error:
                self.skipTest(f"symbolic links are unavailable: {error}")
            with self.assertRaises(MANAGE.ManagementError):
                MANAGE.assert_output_path(link / "generated.bin", self.policy)

    def test_force_override_is_disabled_in_ci_even_with_a_tty(self) -> None:
        unmanaged = ROOT / "crates" / "unmanaged-output.bin"
        with mock.patch.dict(os.environ, {"CI": "true"}, clear=False), mock.patch.object(
            sys.stdin, "isatty", return_value=True
        ), mock.patch.object(sys.stderr, "isatty", return_value=True):
            with self.assertRaises(MANAGE.ManagementError):
                MANAGE.assert_output_path(
                    unmanaged,
                    self.policy,
                    force=True,
                    force_reason="test-only reason",
                )

    def test_cargo_output_overrides_are_rejected_before_execution(self) -> None:
        for option in (
            "--target-dir=outside",
            "--artifact-dir",
            "--build-dir=outside",
            "--out-dir=outside",
            "--config=build.target-dir='outside'",
        ):
            with self.subTest(option=option), self.assertRaises(MANAGE.ManagementError):
                MANAGE.validate_managed_command("cargo", ["cargo", "check", option])

    def test_secret_names_are_never_accepted(self) -> None:
        for name in (".env", ".env.production", "deploy.key", "service-account-prod.json", "API-KEY.txt"):
            self.assertTrue(MANAGE.is_secret_path(pathlib.Path("nested") / name, self.policy), name)
        self.assertFalse(MANAGE.is_secret_path(pathlib.Path("docs") / "research" / "results.md", self.policy))

    def test_worktree_parser_preserves_branch_and_detached_identity(self) -> None:
        parsed = MANAGE.parse_worktrees(
            "worktree C:/repo\nHEAD " + "1" * 40 + "\nbranch refs/heads/main\n\n"
            "worktree C:/other\nHEAD " + "2" * 40 + "\ndetached\n"
        )
        self.assertEqual(parsed[0].branch, "refs/heads/main")
        self.assertTrue(parsed[1].detached)

    def test_warning_contains_override_and_recommendation(self) -> None:
        self.assertIn("--force-unmanaged-output", MANAGE.WARNING)
        self.assertIn("강제 실행은 권장하지 않습니다", MANAGE.WARNING)

    def test_policy_json_is_stable_and_has_unique_ids(self) -> None:
        material = json.loads((ROOT / "config" / "clearra-management.v1.json").read_text(encoding="utf-8"))
        for key in ("repository_roots", "external_roots", "producers"):
            ids = [entry["id"] for entry in material[key]]
            self.assertEqual(len(ids), len(set(ids)), key)
        for key in ("writer_registry", "process_registry"):
            paths = [entry["path"] for entry in material[key]]
            self.assertEqual(len(paths), len(set(paths)), key)

    def test_writer_registry_exactly_matches_detected_writers(self) -> None:
        registered = {entry["path"] for entry in self.policy["writer_registry"]}
        actual = {
            path.relative_to(ROOT).as_posix()
            for path in MANAGE.repository_policy_files()
            if path.is_file()
            and not MANAGE.is_secret_path(path, self.policy)
            and MANAGE.writer_candidate(path)
        }
        self.assertEqual(registered, actual)
        MANAGE.verify_static_management_policy(self.policy)

    def test_process_registry_exactly_matches_detected_processes(self) -> None:
        registered = {entry["path"] for entry in self.policy["process_registry"]}
        actual = {
            path.relative_to(ROOT).as_posix()
            for path in MANAGE.repository_policy_files()
            if path.is_file()
            and not MANAGE.is_secret_path(path, self.policy)
            and MANAGE.process_candidate(path)
        }
        self.assertEqual(registered, actual)

    def test_shared_store_delta_records_file_identity_without_contents(self) -> None:
        before = {"a": {"file_id": "1:1", "bytes": 2, "mtime_ns": 3}}
        after = {
            "a": {"file_id": "1:1", "bytes": 4, "mtime_ns": 5},
            "b": {"file_id": "1:2", "bytes": 7, "mtime_ns": 8},
        }
        delta = MANAGE.tree_identity_delta(pathlib.Path("store"), before, after)
        self.assertEqual(delta["byte_delta"], 9)
        self.assertEqual([item["path"] for item in delta["added"]], ["b"])
        self.assertEqual([item["path"] for item in delta["changed"]], ["a"])

    def test_directory_measurement_marks_a_bounded_result_as_a_lower_bound(self) -> None:
        parent = ROOT / "_local" / "tmp" / "management-tests"
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            root = pathlib.Path(directory)
            (root / "a.bin").write_bytes(b"a")
            (root / "b.bin").write_bytes(b"bb")
            bounded = MANAGE.directory_measurement(root, max_entries=1)
            self.assertFalse(bounded["complete"])
            self.assertEqual(bounded["measurement_kind"], "bounded-lower-bound")
            self.assertEqual(bounded["sampled_entries"], 1)
            self.assertGreaterEqual(bounded["bytes"], 1)

            exact = MANAGE.directory_measurement(root)
            self.assertTrue(exact["complete"])
            self.assertEqual(exact["measurement_kind"], "exact")
            self.assertEqual(exact["bytes"], 3)


if __name__ == "__main__":
    unittest.main()
