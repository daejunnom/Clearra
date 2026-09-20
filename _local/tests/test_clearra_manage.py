from __future__ import annotations

import importlib.util
import io
import json
import os
import pathlib
import tarfile
import copy
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

    def test_independent_clone_normalizes_github_transports_without_credentials(self) -> None:
        expected = "https://github.com/daejunnom/Clearra.git"
        for remote in (
            "git@github.com:daejunnom/Clearra.git",
            "ssh://git@github.com/daejunnom/Clearra.git",
            "https://github.com/daejunnom/Clearra.git",
            "https://github.com/daejunnom/Clearra",
        ):
            with self.subTest(remote=remote):
                self.assertEqual(MANAGE.github_https_clone_url(remote), expected)

        for remote in (
            "https://token@github.com/daejunnom/Clearra.git",
            "https://example.invalid/daejunnom/Clearra.git",
            "git@github.com:../Clearra.git",
            "git@github.com:daejunnom/../Clearra.git",
        ):
            with self.subTest(remote=remote), self.assertRaises(MANAGE.ManagementError):
                MANAGE.github_https_clone_url(remote)

    def test_local_windows_cargo_tool_bootstrap_uses_one_job(self) -> None:
        local_windows = {"CARGO_BUILD_JOBS": "12"}
        MANAGE.constrain_managed_cargo_install_jobs(local_windows, platform_name="nt")
        self.assertEqual(local_windows["CARGO_BUILD_JOBS"], "1")

        ci_windows = {"CI": "true", "CARGO_BUILD_JOBS": "2"}
        MANAGE.constrain_managed_cargo_install_jobs(ci_windows, platform_name="nt")
        self.assertEqual(ci_windows["CARGO_BUILD_JOBS"], "2")

        local_linux = {"CARGO_BUILD_JOBS": "8"}
        MANAGE.constrain_managed_cargo_install_jobs(local_linux, platform_name="posix")
        self.assertEqual(local_linux["CARGO_BUILD_JOBS"], "8")

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

    def test_force_override_is_one_call_local_tty_capability(self) -> None:
        unmanaged = ROOT / "crates" / "one-call-unmanaged-output.bin"
        with mock.patch.dict(
            os.environ,
            {
                "CI": "",
                "GITHUB_ACTIONS": "",
                "CLEARRA_RELEASE": "",
                "CLEARRA_DEPLOYMENT": "",
            },
            clear=False,
        ), mock.patch.object(sys.stdin, "isatty", return_value=True), mock.patch.object(
            sys.stderr, "isatty", return_value=True
        ):
            accepted = MANAGE.assert_output_path(
                unmanaged,
                self.policy,
                force=True,
                force_reason="isolated local fixture",
            )
        self.assertEqual(accepted, unmanaged.resolve(strict=False))
        with self.assertRaises(MANAGE.ManagementError):
            MANAGE.assert_output_path(unmanaged, self.policy)

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
        def reject_duplicate_keys(pairs):
            value = {}
            for key, item in pairs:
                if key in value:
                    raise AssertionError(f"duplicate JSON key: {key}")
                value[key] = item
            return value

        material = json.loads(
            (ROOT / "config" / "clearra-management.v1.json").read_text(
                encoding="utf-8"
            ),
            object_pairs_hook=reject_duplicate_keys,
        )
        for key in ("repository_roots", "external_roots", "producers"):
            ids = [entry["id"] for entry in material[key]]
            self.assertEqual(len(ids), len(set(ids)), key)
        for key in ("writer_registry", "process_registry"):
            paths = [entry["path"] for entry in material[key]]
            self.assertEqual(len(paths), len(set(paths)), key)
        self.assertEqual(material["package_policy"]["authority"], "pnpm")
        self.assertEqual(material["package_policy"]["publishable_packages"], ["ctk3"])

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

    def test_ruleset_readback_requires_the_exact_closed_policy(self) -> None:
        value = {
            "id": 42,
            "name": "Clearra main fast-forward gate",
            "target": "branch",
            "enforcement": "active",
            "bypass_actors": [],
            "conditions": {
                "ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}
            },
            "rules": [
                {"type": "deletion"},
                {"type": "non_fast_forward"},
                {"type": "required_linear_history"},
                {
                    "type": "required_status_checks",
                    "parameters": {
                        "strict_required_status_checks_policy": True,
                        "do_not_enforce_on_create": True,
                        "required_status_checks": [
                            {"context": "Clearra management policy"}
                        ],
                    },
                },
            ],
        }
        accepted = MANAGE.verify_ruleset_readback(value, self.policy)
        self.assertEqual(accepted["id"], 42)

        invalid_values = []
        bypass = copy.deepcopy(value)
        bypass["bypass_actors"] = [{"actor_id": 1, "actor_type": "RepositoryRole"}]
        invalid_values.append(bypass)
        pull_request = copy.deepcopy(value)
        pull_request["rules"].append({"type": "pull_request", "parameters": {}})
        invalid_values.append(pull_request)
        duplicate_check = copy.deepcopy(value)
        duplicate_check["rules"][-1]["parameters"]["required_status_checks"].append(
            {"context": "Clearra management policy"}
        )
        invalid_values.append(duplicate_check)
        excluded_ref = copy.deepcopy(value)
        excluded_ref["conditions"]["ref_name"]["exclude"] = ["refs/heads/release"]
        invalid_values.append(excluded_ref)
        non_strict = copy.deepcopy(value)
        non_strict["rules"][-1]["parameters"][
            "strict_required_status_checks_policy"
        ] = False
        invalid_values.append(non_strict)
        for invalid in invalid_values:
            with self.subTest(invalid=invalid), self.assertRaises(MANAGE.ManagementError):
                MANAGE.verify_ruleset_readback(invalid, self.policy)

    def test_dependency_update_arguments_reject_path_and_credential_overrides(self) -> None:
        self.assertEqual(
            MANAGE.validate_dependency_update_arguments(
                "pnpm", ["--", "ctk3", "--latest"]
            ),
            ["ctk3", "--latest"],
        )
        for manager, values in (
            ("pnpm", ["--store-dir=outside"]),
            ("pnpm", ["--registry=https://token@example.invalid"]),
            ("cargo", ["--manifest-path", "outside/Cargo.toml"]),
            ("cargo", ["--config=net.token=secret"]),
        ):
            with self.subTest(manager=manager, values=values), self.assertRaises(
                MANAGE.ManagementError
            ):
                MANAGE.validate_dependency_update_arguments(manager, values)

    def test_package_tarball_inspection_seals_members_and_identity(self) -> None:
        parent = ROOT / "_local" / "tmp" / "management-tests"
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=parent) as directory:
            valid = pathlib.Path(directory) / "valid.tgz"
            with tarfile.open(valid, "w:gz") as archive:
                manifest = json.dumps({"name": "ctk3", "version": "0.1.1"}).encode()
                info = tarfile.TarInfo("package/package.json")
                info.size = len(manifest)
                archive.addfile(info, io.BytesIO(manifest))
                source = b"export const value = 1;\n"
                info = tarfile.TarInfo("package/dist/index.js")
                info.size = len(source)
                archive.addfile(info, io.BytesIO(source))
            inspected = MANAGE.inspect_package_tarball(valid, "ctk3", "0.1.1")
            self.assertEqual(inspected["name"], "ctk3")
            self.assertEqual(len(inspected["members"]), 2)

            unsafe = pathlib.Path(directory) / "unsafe.tgz"
            with tarfile.open(unsafe, "w:gz") as archive:
                content = b"escape"
                info = tarfile.TarInfo("package/../escape.txt")
                info.size = len(content)
                archive.addfile(info, io.BytesIO(content))
            with self.assertRaisesRegex(MANAGE.ManagementError, "unsafe member"):
                MANAGE.inspect_package_tarball(unsafe, "ctk3", "0.1.1")

            duplicate = pathlib.Path(directory) / "duplicate.tgz"
            with tarfile.open(duplicate, "w:gz") as archive:
                manifest = json.dumps({"name": "ctk3", "version": "0.1.1"}).encode()
                for _ in range(2):
                    info = tarfile.TarInfo("package/package.json")
                    info.size = len(manifest)
                    archive.addfile(info, io.BytesIO(manifest))
            with self.assertRaisesRegex(MANAGE.ManagementError, "duplicate member"):
                MANAGE.inspect_package_tarball(duplicate, "ctk3", "0.1.1")


if __name__ == "__main__":
    unittest.main()
