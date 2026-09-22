from __future__ import annotations

import copy
import hashlib
import importlib.util
import io
import json
import os
import pathlib
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest import mock


ROOT = pathlib.Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "clearra_runtime", ROOT / "scripts" / "management" / "clearra_runtime.py"
)
assert SPEC and SPEC.loader
RUNTIME = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = RUNTIME
SPEC.loader.exec_module(RUNTIME)


def load_policy() -> dict:
    return json.loads(
        (ROOT / "config" / "clearra-management.v1.json").read_text(encoding="utf-8")
    )


class RuntimeContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.policy = load_policy()
        (ROOT / "_local" / "tmp").mkdir(parents=True, exist_ok=True)

    def test_windows_control_start_admission_uses_only_critical_reserves(self) -> None:
        admission = RUNTIME.calculate_admission(
            self.policy,
            "control",
            snapshot=RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                2 * RUNTIME.GIB,
                32 * RUNTIME.GIB,
                12 * RUNTIME.GIB,
            ),
            platform_name="nt",
        )
        self.assertEqual(admission.reserve_bytes, 128 * RUNTIME.MIB)
        self.assertEqual(admission.commit_reserve_bytes, 256 * RUNTIME.MIB)
        self.assertEqual(admission.hard_limit_bytes, 512 * RUNTIME.MIB)
        self.assertEqual(admission.capacity_basis, "runtime-pressure")
        self.assertEqual(admission.start_admission_mode, "critical-reserve")

        with self.assertRaisesRegex(
            RUNTIME.RuntimePolicyError, RUNTIME.ADMISSION_ERROR
        ):
            RUNTIME.calculate_admission(
                self.policy,
                "control",
                snapshot=RUNTIME.MemorySnapshot(
                    16 * RUNTIME.GIB,
                    64 * RUNTIME.MIB,
                    32 * RUNTIME.GIB,
                    12 * RUNTIME.GIB,
                ),
                platform_name="nt",
            )

    def test_windows_build_start_is_decoupled_from_declared_working_set(self) -> None:
        admission = RUNTIME.calculate_admission(
            self.policy,
            "build-test",
            snapshot=RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                768 * RUNTIME.MIB,
                32 * RUNTIME.GIB,
                2 * RUNTIME.GIB,
            ),
            platform_name="nt",
        )
        self.assertEqual(admission.reserve_bytes, 128 * RUNTIME.MIB)
        self.assertEqual(admission.commit_reserve_bytes, 256 * RUNTIME.MIB)
        self.assertEqual(admission.minimum_bytes, 3 * RUNTIME.GIB)
        self.assertEqual(admission.hard_limit_bytes, 6 * RUNTIME.GIB)
        self.assertEqual(admission.maximum_bytes, 6 * RUNTIME.GIB)
        self.assertEqual(admission.capacity_basis, "runtime-pressure")

        for available, commit_available in (
            (64 * RUNTIME.MIB, 14 * RUNTIME.GIB),
            (8 * RUNTIME.GIB, 128 * RUNTIME.MIB),
        ):
            with self.subTest(
                available=available, commit_available=commit_available
            ), self.assertRaisesRegex(
                RUNTIME.RuntimePolicyError, RUNTIME.ADMISSION_ERROR
            ):
                RUNTIME.calculate_admission(
                    self.policy,
                    "build-test",
                    snapshot=RUNTIME.MemorySnapshot(
                        16 * RUNTIME.GIB,
                        available,
                        32 * RUNTIME.GIB,
                        commit_available,
                    ),
                    platform_name="nt",
                )

    def test_non_windows_relaxed_profiles_keep_stable_tree_limits(self) -> None:
        for profile in ("build-test", "verification"):
            admission = RUNTIME.calculate_admission(
                self.policy,
                profile,
                snapshot=RUNTIME.MemorySnapshot(
                    16 * RUNTIME.GIB,
                    8 * RUNTIME.GIB,
                    32 * RUNTIME.GIB,
                    20 * RUNTIME.GIB,
                ),
                platform_name="posix",
            )
            self.assertEqual(admission.reserve_bytes, 128 * RUNTIME.MIB)
            self.assertEqual(admission.capacity_basis, "runtime-pressure")

        benchmark = RUNTIME.calculate_admission(
            self.policy,
            "benchmark-search",
            snapshot=RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                768 * RUNTIME.MIB,
                32 * RUNTIME.GIB,
                2 * RUNTIME.GIB,
            ),
            platform_name="posix",
        )
        self.assertEqual(benchmark.reserve_bytes, 128 * RUNTIME.MIB)
        self.assertEqual(benchmark.minimum_bytes, 4 * RUNTIME.GIB)
        self.assertEqual(
            benchmark.hard_limit_bytes, 16 * RUNTIME.GIB - 512 * RUNTIME.MIB
        )
        self.assertEqual(benchmark.capacity_basis, "runtime-pressure")
        self.assertEqual(benchmark.start_admission_mode, "critical-reserve")

        cloud = RUNTIME.calculate_admission(
            self.policy,
            "cloud-job",
            snapshot=RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                RUNTIME.GIB,
                32 * RUNTIME.GIB,
                2 * RUNTIME.GIB,
            ),
            platform_name="posix",
        )
        self.assertEqual(cloud.hard_limit_bytes, 16 * RUNTIME.GIB)

    def test_windows_verification_profile_has_fixed_tree_limit(self) -> None:
        admission = RUNTIME.calculate_admission(
            self.policy,
            "verification",
            snapshot=RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                4 * RUNTIME.GIB,
                32 * RUNTIME.GIB,
                14 * RUNTIME.GIB,
            ),
            platform_name="nt",
        )
        self.assertEqual(admission.minimum_bytes, 768 * RUNTIME.MIB)
        self.assertEqual(admission.hard_limit_bytes, 2 * RUNTIME.GIB)
        self.assertEqual(admission.maximum_bytes, 2 * RUNTIME.GIB)
        self.assertEqual(admission.capacity_basis, "runtime-pressure")

    def test_memory_pressure_tracks_external_physical_and_commit_growth(self) -> None:
        healthy = RUNTIME.memory_pressure_status(
            self.policy,
            RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                2 * RUNTIME.GIB,
                32 * RUNTIME.GIB,
                4 * RUNTIME.GIB,
            ),
            platform_name="nt",
        )
        self.assertFalse(healthy["under_pressure"])
        self.assertEqual(healthy["physical_reserve_bytes"], 512 * RUNTIME.MIB)
        self.assertEqual(healthy["commit_reserve_bytes"], RUNTIME.GIB)

        pressured = RUNTIME.memory_pressure_status(
            self.policy,
            RUNTIME.MemorySnapshot(
                16 * RUNTIME.GIB,
                384 * RUNTIME.MIB,
                32 * RUNTIME.GIB,
                768 * RUNTIME.MIB,
            ),
            platform_name="nt",
        )
        self.assertTrue(pressured["under_pressure"])
        self.assertFalse(pressured["critical"])
        self.assertEqual(
            pressured["reasons"], ["physical-available", "commit-available"]
        )

    def test_child_full_gc_requires_an_exact_protocol_acknowledgement(self) -> None:
        with tempfile.TemporaryDirectory(
            dir=ROOT / "_local" / "tmp"
        ) as directory, mock.patch.dict(
            os.environ, {"CLEARRA_STATE_ROOT": directory}
        ):
            channel = RUNTIME._create_gc_pressure_channel(
                "clearra.memory-pressure.v1"
            )
            acknowledgement = pathlib.Path(channel["acknowledgement"])
            acknowledgement.write_text(
                json.dumps(
                    {
                        "schema_id": "clearra.memory-pressure.v1",
                        "request_id": "wrong-request",
                        "action": "full-gc",
                        "status": "completed",
                    }
                ),
                encoding="utf-8",
            )
            self.assertFalse(
                RUNTIME._gc_pressure_acknowledged(
                    channel, request_id="expected-request"
                )
            )
            acknowledgement.write_text(
                json.dumps(
                    {
                        "schema_id": "clearra.memory-pressure.v1",
                        "request_id": "expected-request",
                        "action": "full-gc",
                        "status": "completed",
                    }
                ),
                encoding="utf-8",
            )
            self.assertTrue(
                RUNTIME._gc_pressure_acknowledged(
                    channel, request_id="expected-request"
                )
            )
            self.assertTrue(RUNTIME._close_gc_pressure_channel(channel))

    def test_parallel_heavy_runtime_slot_is_single_owner_and_reusable(self) -> None:
        admission = RUNTIME.Admission(
            profile="build-test",
            physical_bytes=16 * RUNTIME.GIB,
            available_bytes=8 * RUNTIME.GIB,
            reserve_bytes=4 * RUNTIME.GIB,
            hard_limit_bytes=4 * RUNTIME.GIB,
            minimum_bytes=3 * RUNTIME.GIB,
            maximum_bytes=None,
        )
        with tempfile.TemporaryDirectory(dir=ROOT / "_local" / "tmp") as directory:
            with mock.patch.dict(os.environ, {"CLEARRA_STATE_ROOT": directory}):
                first = RUNTIME._RuntimeSlot.acquire(self.policy, "build-test", admission)
                try:
                    with self.assertRaisesRegex(
                        RUNTIME.RuntimePolicyError, RUNTIME.CONCURRENCY_ERROR
                    ):
                        RUNTIME._RuntimeSlot.acquire(self.policy, "build-test", admission)
                finally:
                    first.release()
                second = RUNTIME._RuntimeSlot.acquire(self.policy, "build-test", admission)
                second.release()

    def test_runtime_slot_proof_allows_only_the_live_owner_tree(self) -> None:
        admission = RUNTIME.Admission(
            profile="verification",
            physical_bytes=16 * RUNTIME.GIB,
            available_bytes=8 * RUNTIME.GIB,
            reserve_bytes=128 * RUNTIME.MIB,
            hard_limit_bytes=2 * RUNTIME.GIB,
            minimum_bytes=768 * RUNTIME.MIB,
            maximum_bytes=2 * RUNTIME.GIB,
        )
        with tempfile.TemporaryDirectory(dir=ROOT / "_local" / "tmp") as directory:
            environment = {"CLEARRA_STATE_ROOT": directory}
            with mock.patch.dict(os.environ, environment, clear=False):
                slot = RUNTIME._RuntimeSlot.acquire(
                    self.policy,
                    "verification",
                    admission,
                    timeout_seconds=5400,
                    maximum_descendant_processes=512,
                )
                try:
                    with mock.patch.dict(
                        os.environ, slot.inheritance_environment(), clear=False
                    ):
                        inherited = RUNTIME.inherited_runtime_boundary(self.policy)
                        self.assertIsNotNone(inherited)
                        assert inherited is not None
                        self.assertEqual(inherited["outer_profile"], "verification")
                        self.assertEqual(inherited["concurrency_class"], "memory-intensive")
                        self.assertEqual(inherited["hard_limit_bytes"], 2 * RUNTIME.GIB)
                        self.assertEqual(inherited["timeout_seconds"], 5400)
                        self.assertEqual(inherited["maximum_descendant_processes"], 512)

                    invalid = slot.inheritance_environment()
                    invalid["CLEARRA_RUNTIME_BOUNDARY_NONCE"] = "not-the-owner"
                    with mock.patch.dict(os.environ, invalid, clear=False), self.assertRaisesRegex(
                        RUNTIME.RuntimePolicyError, RUNTIME.INHERITED_BOUNDARY_ERROR
                    ):
                        RUNTIME.inherited_runtime_boundary(self.policy)
                finally:
                    slot.release()

    def test_inherited_command_uses_outer_boundary_without_another_slot(self) -> None:
        boundary = {
            "kind": "inherited-runtime-boundary",
            "outer_profile": "verification",
            "concurrency_class": "memory-intensive",
            "concurrency_slot": 0,
            "owner_pid": os.getpid(),
            "hard_limit_bytes": 2 * RUNTIME.GIB,
            "maximum_descendant_processes": 512,
            "timeout_seconds": 5400,
            "slot_path": "fixture",
            "cleanup_owner": "outer-supervisor",
        }
        result = RUNTIME.run_inherited_host_process(
            [sys.executable, "-c", "print('nested-ok')"],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=self.policy,
            profile="verification",
            echo=False,
            boundary=boundary,
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.reason, "normal")
        self.assertEqual(result.stdout.strip(), "nested-ok")
        self.assertEqual(result.containment["cleanup_owner"], "outer-supervisor")
        self.assertTrue(result.containment["cleanup_deferred_to_outer"])

    def test_relaxed_admission_preserves_minimum_without_preallocation(self) -> None:
        admission = RUNTIME.calculate_admission(
            self.policy,
            "benchmark-search",
            snapshot=RUNTIME.MemorySnapshot(16 * RUNTIME.GIB, 768 * RUNTIME.MIB),
            minimum_override_mib=6144,
            platform_name="posix",
        )
        self.assertEqual(admission.minimum_bytes, 6 * RUNTIME.GIB)
        self.assertGreaterEqual(admission.hard_limit_bytes, admission.minimum_bytes)

        with self.assertRaisesRegex(
            RUNTIME.RuntimePolicyError, RUNTIME.ADMISSION_ERROR
        ):
            RUNTIME.calculate_admission(
                self.policy,
                "benchmark-search",
                snapshot=RUNTIME.MemorySnapshot(16 * RUNTIME.GIB, 64 * RUNTIME.MIB),
                minimum_override_mib=6144,
                platform_name="posix",
            )

    def test_argv_redaction_preserves_shape_without_secret_values(self) -> None:
        actual = RUNTIME.redact_argv(
            [
                "tool",
                "--token",
                "do-not-record",
                "--api-key=also-hidden",
                "--ordinary",
                "visible",
                "--SshKeyPath",
                "C:/keys/generic-name",
                "C:/managed/credential-files/cloud.json",
                "C:/managed/project/.env.production",
            ]
        )
        self.assertEqual(
            actual,
            [
                "tool",
                "--token",
                "<redacted>",
                "--api-key=<redacted>",
                "--ordinary",
                "visible",
                "--SshKeyPath",
                "<redacted>",
                "<redacted>",
                "<redacted>",
            ],
        )
        self.assertEqual(
            RUNTIME.command_digest(["tool", "--password", "first"]),
            RUNTIME.command_digest(["tool", "--password", "second"]),
        )

    def test_zero_timeout_is_rejected_instead_of_using_the_profile_default(self) -> None:
        policy = self._integration_policy()
        with self.assertRaisesRegex(
            RUNTIME.RuntimePolicyError, "timeout exceeds profile maximum"
        ):
            RUNTIME.run_host_process(
                [sys.executable, "-B", "-c", "raise SystemExit(0)"],
                cwd=ROOT,
                env=os.environ.copy(),
                policy=policy,
                profile="control",
                timeout_override_seconds=0,
                echo=False,
            )

    def test_wsl_export_bootstrap_appends_only_owned_first_boot_metadata(self) -> None:
        temporary_root = ROOT / "_local" / "tmp"
        with tempfile.TemporaryDirectory(dir=temporary_root) as directory:
            archive_path = pathlib.Path(directory) / "ubuntu-export.tar"
            original = b"source-owned"
            with tarfile.open(archive_path, "w", format=tarfile.PAX_FORMAT) as archive:
                information = tarfile.TarInfo("source.txt")
                information.size = len(original)
                archive.addfile(information, io.BytesIO(original))
                for name, payload in (
                    ("etc/wsl.conf", b"[boot]\nsystemd=true\n"),
                    ("etc/clearra/runtime.json", b'{"source_owned":true}\n'),
                ):
                    information = tarfile.TarInfo(name)
                    information.size = len(payload)
                    archive.addfile(information, io.BytesIO(payload))

            RUNTIME._append_wsl_bootstrap_metadata(
                archive_path,
                transaction="fixture-transaction",
                source_distribution="Ubuntu",
            )

            with tarfile.open(archive_path, "r") as archive:
                self.assertEqual(archive.extractfile("source.txt").read(), original)
                wsl_config = archive.extractfile("etc/wsl.conf").read().decode("utf-8")
                marker = json.loads(
                    archive.extractfile("etc/clearra/runtime.json")
                    .read()
                    .decode("utf-8")
                )
            self.assertIn("systemd=false", wsl_config)
            self.assertIn("default=root", wsl_config)
            self.assertNotIn("systemd=true", wsl_config)
            self.assertEqual(marker["schema_id"], "clearra.wsl-provision-owner.v1")
            self.assertEqual(marker["status"], "imported")
            self.assertEqual(marker["created_transaction"], "fixture-transaction")

    def test_wsl_source_digest_matches_the_bytes_written_to_the_archive(self) -> None:
        temporary_root = ROOT / "_local" / "tmp"
        with tempfile.TemporaryDirectory(
            dir=temporary_root
        ) as source_directory, tempfile.TemporaryDirectory(
            dir=temporary_root
        ) as output_directory:
            source_root = pathlib.Path(source_directory)
            payload = b"archive-authority"
            (source_root / "fixture.txt").write_bytes(payload)
            listing = SimpleNamespace(returncode=0, stdout="fixture.txt\0")
            with mock.patch.object(RUNTIME, "run_host_process", return_value=listing):
                archive, digest, count = RUNTIME._source_archive(
                    source_root,
                    pathlib.Path(output_directory) / "bundle",
                    lambda _path: False,
                    self.policy,
                )
            expected = hashlib.sha256()
            name = b"fixture.txt"
            expected.update(len(name).to_bytes(4, "big"))
            expected.update(name)
            expected.update(hashlib.sha256(payload).digest())
            self.assertEqual(digest, expected.hexdigest())
            self.assertEqual(count, 1)
            with tarfile.open(archive, "r:gz") as bundle:
                self.assertEqual(bundle.extractfile("fixture.txt").read(), payload)

    def test_wsl_parent_loss_powers_off_only_the_dedicated_guest(self) -> None:
        session = (ROOT / "scripts" / "runtime" / "clearra-wsl-session.sh").read_text(
            encoding="utf-8"
        )
        sanitizer = (
            ROOT / "scripts" / "runtime" / "clearra-wsl-sanitize.sh"
        ).read_text(encoding="utf-8")
        self.assertIn("systemctl poweroff --force --force --no-block", session)
        self.assertIn("/sbin/poweroff -f", sanitizer)
        self.assertNotIn("cat >/dev/null", session)
        self.assertNotIn("cat >/dev/null", sanitizer)
        self.assertIn("CLEARRA_WSL_CGROUP_REMOVED", session)
        forbidden_global_shutdown = "wsl --" + "shutdown"
        self.assertNotIn(forbidden_global_shutdown, session.casefold())
        self.assertNotIn(forbidden_global_shutdown, sanitizer.casefold())

    def test_wsl_sanitizer_rebuilds_the_dedicated_prefix(self) -> None:
        material = (
            ROOT / "scripts" / "runtime" / "clearra-wsl-sanitize.sh"
        ).read_text(encoding="utf-8")
        preserve = material.index('bootstrap_rustup="/run/clearra-bootstrap-rustup-')
        remove = material.index("rm -rf -- /opt/clearra")
        rebuild = material.index("install -d -m 0755 /opt/clearra/bootstrap")
        self.assertLess(preserve, remove)
        self.assertLess(remove, rebuild)
        self.assertIn(
            'install -m 0755 "$bootstrap_rustup" /opt/clearra/bootstrap/rustup',
            material,
        )

    def test_wsl_provision_verifies_the_manifest_cargo_version(self) -> None:
        provision = (
            ROOT / "scripts" / "runtime" / "clearra-wsl-provision.sh"
        ).read_text(encoding="utf-8")
        runtime = (ROOT / "scripts" / "management" / "clearra_runtime.py").read_text(
            encoding="utf-8"
        )
        cargo = self.policy["toolchains"]["cargo"]
        self.assertIn('--cargo-version) CARGO_VERSION="$2"', provision)
        self.assertIn('[[ "$cargo_actual" == "cargo ${CARGO_VERSION}"* ]]', provision)
        self.assertIn('"--cargo-version",', runtime)
        self.assertRegex(cargo, r"^\d+\.\d+\.\d+$")

    def test_legal_board_wsl_entry_is_source_bound_and_benchmark_bounded(self) -> None:
        entry = self.policy["runtime_policy"]["wsl"]["entrypoints"][
            "legal-board-generate"
        ]
        self.assertEqual(entry["profile"], "benchmark-search")
        self.assertTrue(entry["requires_source"])
        self.assertEqual(entry["timeout_seconds"], 7200)
        runtime = (ROOT / "scripts" / "management" / "clearra_runtime.py").read_text(
            encoding="utf-8"
        )
        guest = (ROOT / "scripts" / "runtime" / "clearra-wsl-guest.sh").read_text(
            encoding="utf-8"
        )
        self.assertIn('"legal-board-generate": ("--layers",)', runtime)
        self.assertIn("legal-board-generate)", guest)
        self.assertIn(
            'exec bash "$SOURCE_ROOT/scripts/tools/wsl-legal-board-generate.sh"',
            guest,
        )

    def test_existing_dedicated_distribution_must_use_the_managed_install_root(self) -> None:
        supervisor = object.__new__(RUNTIME.WslSupervisor)
        supervisor._distro_storage_snapshot = lambda _distribution: {
            "base_path": "C:/managed/Clearra-Build"
        }
        accepted = supervisor._assert_registered_install_root(
            "Clearra-Build", pathlib.Path("C:/managed/Clearra-Build")
        )
        self.assertEqual(accepted["base_path"], "C:/managed/Clearra-Build")
        with self.assertRaisesRegex(
            RUNTIME.RuntimePolicyError, "differs from the managed install root"
        ):
            supervisor._assert_registered_install_root(
                "Clearra-Build", pathlib.Path("C:/other/Clearra-Build")
            )

    def test_wsl_cleanup_list_uses_the_bounded_cleanup_timeout(self) -> None:
        supervisor = object.__new__(RUNTIME.WslSupervisor)
        observed: list[dict] = []

        def control(_arguments, **options):
            observed.append(options)
            return SimpleNamespace(returncode=0, stdout="")

        supervisor._control = control
        self.assertEqual(supervisor._list(running=True, cleanup_only=True), [])
        self.assertEqual(observed, [{"timeout": 60, "cleanup_only": True}])

    def test_wsl_termination_failure_preserves_the_guest_failure_receipt(self) -> None:
        supervisor = object.__new__(RUNTIME.WslSupervisor)
        supervisor.root = ROOT
        supervisor.policy = self.policy
        supervisor.contract = self.policy["runtime_policy"]["wsl"]
        supervisor.temporary_root = ROOT / "_local" / "tmp"
        supervisor.state_root = ROOT / "_local" / "state"
        supervisor.secret_predicate = lambda _path: False
        supervisor.output_validator = lambda path: path
        supervisor.wsl = "wsl.exe"
        supervisor.distribution = "Clearra-Build"
        supervisor._list = lambda *args, **kwargs: []
        lease = mock.MagicMock()
        lease.__enter__.return_value = {"recovered_stale_owner": False}
        lease.__exit__.return_value = False
        supervisor.lease = mock.MagicMock(return_value=lease)
        supervisor.terminate = mock.MagicMock(
            side_effect=RUNTIME.RuntimePolicyError("dedicated distribution remained running")
        )
        admission = RUNTIME.Admission(
            profile="control",
            physical_bytes=16 * RUNTIME.GIB,
            available_bytes=8 * RUNTIME.GIB,
            reserve_bytes=4 * RUNTIME.GIB,
            hard_limit_bytes=RUNTIME.GIB,
            minimum_bytes=256 * RUNTIME.MIB,
            maximum_bytes=RUNTIME.GIB,
        )
        guest_output = "\n".join(
            [
                "CLEARRA_WSL_RESULT=exit-code",
                "CLEARRA_WSL_CONTROL_GROUP=/system.slice/clearra-fixture.service",
                "CLEARRA_WSL_MEMORY_PEAK=4096",
                "CLEARRA_WSL_REMAINING_TASKS=0",
                "CLEARRA_WSL_UNIT_ACTIVE_STATE=inactive",
                "CLEARRA_WSL_CGROUP_REMOVED=true",
            ]
        )
        runtime = RUNTIME.RuntimeResult(
            command=["wsl.exe"],
            command_sha256="0" * 64,
            returncode=23,
            reason="nonzero",
            error_code=RUNTIME.NONZERO_ERROR,
            started_utc="2026-01-01T00:00:00+00:00",
            ended_utc="2026-01-01T00:00:01+00:00",
            duration_ms=1000,
            timeout_seconds=300,
            termination_stage="none",
            stdout=guest_output,
            stderr="",
            output_bytes=len(guest_output),
            output_limit_bytes=1024,
            admission={},
            containment={},
            peak_memory_bytes=4096,
            descendant_processes=1,
            oom_counter_before=0,
            oom_counter_after=0,
            process_tree_stopped=True,
        )
        with mock.patch.object(
            RUNTIME, "calculate_admission", return_value=admission
        ), mock.patch.object(
            RUNTIME, "run_host_process", return_value=runtime
        ), mock.patch.object(
            RUNTIME, "windows_to_wsl_mount", return_value="/mnt/c/clearra-script"
        ), self.assertRaisesRegex(
            RUNTIME.RuntimePolicyError, RUNTIME.TREE_CLEANUP_ERROR
        ) as captured:
            supervisor.run_entry("fixture-nonzero", [])

        self.assertEqual(captured.exception.details["runtime"]["returncode"], 23)
        self.assertEqual(captured.exception.details["runtime"]["reason"], "nonzero")
        self.assertEqual(
            captured.exception.details["runtime"]["error_code"],
            RUNTIME.NONZERO_ERROR,
        )
        self.assertIn("remained running", captured.exception.details["termination_error"])

    def _integration_policy(
        self, *, output_limit: int = 1024 * 1024, memory_mib: int = 256
    ) -> dict:
        policy = copy.deepcopy(self.policy)
        control = policy["resource_profiles"]["control"]
        control["minimum_memory_mib"] = memory_mib
        control["maximum_memory_mib"] = memory_mib
        control["timeout_seconds"] = 5
        control["maximum_timeout_seconds"] = 10
        control["output_limit_bytes"] = output_limit
        return policy

    def _require_windows_admission(self, policy: dict) -> None:
        if os.name != "nt":
            self.skipTest("Windows Job Object integration runs on Windows")
        try:
            RUNTIME.calculate_admission(policy, "control")
        except RUNTIME.RuntimePolicyError as error:
            if os.environ.get("CI"):
                self.fail(f"Windows CI memory admission is unavailable: {error}")
            self.skipTest(f"host memory admission is currently unavailable: {error}")

    def _require_linux_admission(self, policy: dict) -> None:
        if os.name == "nt":
            self.skipTest("Linux process-group/cgroup integration runs on Linux")
        try:
            RUNTIME.calculate_admission(policy, "control")
        except RUNTIME.RuntimePolicyError as error:
            if os.environ.get("CI"):
                self.fail(f"Linux CI memory admission is unavailable: {error}")
            self.skipTest(f"host memory admission is currently unavailable: {error}")

    def test_linux_lease_wrapper_records_containment(self) -> None:
        policy = self._integration_policy()
        self._require_linux_admission(policy)
        result = RUNTIME.run_host_process(
            [sys.executable, "-B", "-c", "print('posix-bounded-runtime-ok')"],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=policy,
            profile="control",
            echo=False,
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.reason, "normal")
        self.assertIsNone(result.error_code)
        self.assertIn("posix-bounded-runtime-ok", result.stdout)
        self.assertNotIn("Fatal Python error", result.stderr)
        self.assertTrue(result.process_tree_stopped)
        self.assertEqual(
            result.containment["lease_wrapper"],
            "scripts/management/clearra_process_wrapper.py",
        )

    def test_persistent_host_pressure_fail_closes_owned_tree_after_gc(self) -> None:
        policy = self._integration_policy()
        policy["runtime_policy"]["memory_pressure"]["sample_interval_seconds"] = 0.02
        policy["runtime_policy"]["memory_pressure"]["recovery_grace_seconds"] = 0.02
        healthy = RUNTIME.MemorySnapshot(
            16 * RUNTIME.GIB,
            2 * RUNTIME.GIB,
            32 * RUNTIME.GIB,
            4 * RUNTIME.GIB,
        )
        pressured = RUNTIME.MemorySnapshot(
            16 * RUNTIME.GIB,
            384 * RUNTIME.MIB,
            32 * RUNTIME.GIB,
            768 * RUNTIME.MIB,
        )
        samples = iter((healthy, pressured, pressured))

        def snapshot() -> RUNTIME.MemorySnapshot:
            return next(samples, pressured)

        with tempfile.TemporaryDirectory(
            dir=ROOT / "_local" / "tmp"
        ) as directory, mock.patch.dict(
            os.environ, {"CLEARRA_STATE_ROOT": directory}
        ), mock.patch.object(
            RUNTIME, "memory_snapshot", side_effect=snapshot
        ):
            result = RUNTIME.run_host_process(
                [sys.executable, "-B", "-c", "import time; time.sleep(60)"],
                cwd=ROOT,
                env=os.environ.copy(),
                policy=policy,
                profile="control",
                echo=False,
            )

        self.assertEqual(result.reason, "host-memory-pressure")
        self.assertEqual(result.error_code, RUNTIME.HOST_MEMORY_PRESSURE_ERROR)
        self.assertTrue(result.process_tree_stopped)
        self.assertTrue(result.memory_pressure["fail_closed"])
        self.assertEqual(result.memory_pressure["supervisor_full_gc_runs"], 1)
        self.assertEqual(result.memory_pressure["cooperative_full_gc_completions"], 0)
        self.assertFalse(
            result.memory_pressure["last_cooperative_full_gc_completed"]
        )
        self.assertTrue(result.memory_pressure["channel_removed"])

    def test_benchmark_pressure_action_uses_gc_before_fail_close(self) -> None:
        policy = self._integration_policy()
        policy["resource_profiles"]["control"]["memory_pressure_action"] = (
            policy["resource_profiles"]["benchmark-search"][
                "memory_pressure_action"
            ]
        )
        policy["runtime_policy"]["memory_pressure"]["sample_interval_seconds"] = 0.02
        healthy = RUNTIME.MemorySnapshot(
            16 * RUNTIME.GIB,
            2 * RUNTIME.GIB,
            32 * RUNTIME.GIB,
            4 * RUNTIME.GIB,
        )
        pressured = RUNTIME.MemorySnapshot(
            16 * RUNTIME.GIB,
            384 * RUNTIME.MIB,
            32 * RUNTIME.GIB,
            768 * RUNTIME.MIB,
        )
        samples = iter((healthy, pressured))

        def snapshot() -> RUNTIME.MemorySnapshot:
            return next(samples, pressured)

        with tempfile.TemporaryDirectory(
            dir=ROOT / "_local" / "tmp"
        ) as directory, mock.patch.dict(
            os.environ, {"CLEARRA_STATE_ROOT": directory}
        ), mock.patch.object(
            RUNTIME, "memory_snapshot", side_effect=snapshot
        ):
            result = RUNTIME.run_host_process(
                [sys.executable, "-B", "-c", "import time; time.sleep(60)"],
                cwd=ROOT,
                env=os.environ.copy(),
                policy=policy,
                profile="control",
                echo=False,
            )

        self.assertEqual(result.reason, "host-memory-pressure")
        self.assertEqual(result.memory_pressure["events"], 1)
        self.assertEqual(result.memory_pressure["supervisor_full_gc_runs"], 1)
        self.assertEqual(result.memory_pressure["cooperative_gc_requests"], 1)
        self.assertTrue(result.memory_pressure["fail_closed"])

    def test_acknowledged_child_full_gc_still_fail_closes_if_pressure_remains(
        self,
    ) -> None:
        policy = self._integration_policy()
        policy["runtime_policy"]["memory_pressure"]["sample_interval_seconds"] = 0.05
        policy["runtime_policy"]["memory_pressure"]["recovery_grace_seconds"] = 0.5
        healthy = RUNTIME.MemorySnapshot(
            16 * RUNTIME.GIB,
            2 * RUNTIME.GIB,
            32 * RUNTIME.GIB,
            4 * RUNTIME.GIB,
        )
        pressured = RUNTIME.MemorySnapshot(
            16 * RUNTIME.GIB,
            384 * RUNTIME.MIB,
            32 * RUNTIME.GIB,
            768 * RUNTIME.MIB,
        )
        samples = iter((healthy, pressured))

        def snapshot() -> RUNTIME.MemorySnapshot:
            return next(samples, pressured)

        program = (
            "import gc,json,os,pathlib,time;"
            "request=pathlib.Path(os.environ['CLEARRA_MEMORY_PRESSURE_REQUEST_PATH']);"
            "ack=pathlib.Path(os.environ['CLEARRA_MEMORY_PRESSURE_ACK_PATH']);"
            "deadline=time.time()+10;"
            "\nwhile not request.is_file() and time.time()<deadline: time.sleep(0.01)\n"
            "payload=json.loads(request.read_text(encoding='utf-8'));"
            "gc.collect();"
            "ack.write_text(json.dumps({'schema_id':os.environ['CLEARRA_MEMORY_PRESSURE_PROTOCOL'],'request_id':payload['request_id'],'action':'full-gc','status':'completed'}),encoding='utf-8');"
            "time.sleep(60)"
        )
        with tempfile.TemporaryDirectory(
            dir=ROOT / "_local" / "tmp"
        ) as directory, mock.patch.dict(
            os.environ, {"CLEARRA_STATE_ROOT": directory}
        ), mock.patch.object(
            RUNTIME, "memory_snapshot", side_effect=snapshot
        ):
            result = RUNTIME.run_host_process(
                [sys.executable, "-B", "-c", program],
                cwd=ROOT,
                env=os.environ.copy(),
                policy=policy,
                profile="control",
                echo=False,
            )

        self.assertEqual(result.reason, "host-memory-pressure")
        self.assertEqual(result.memory_pressure["cooperative_full_gc_completions"], 1)
        self.assertTrue(
            result.memory_pressure["last_cooperative_full_gc_completed"]
        )
        self.assertTrue(result.memory_pressure["fail_closed"])

    def test_linux_timeout_kills_detached_descendant(self) -> None:
        policy = self._integration_policy()
        self._require_linux_admission(policy)
        with tempfile.TemporaryDirectory(dir=ROOT / "_local" / "tmp") as directory:
            pid_path = pathlib.Path(directory) / "detached.pid"
            program = (
                "import pathlib,subprocess,sys,time;"
                "p=subprocess.Popen([sys.executable,'-B','-c','import time;time.sleep(60)'],start_new_session=True);"
                f"pathlib.Path({str(pid_path)!r}).write_text(str(p.pid),encoding='ascii');"
                "time.sleep(60)"
            )
            result = RUNTIME.run_host_process(
                [sys.executable, "-B", "-c", program],
                cwd=ROOT,
                env=os.environ.copy(),
                policy=policy,
                profile="control",
                timeout_override_seconds=1,
                echo=False,
            )
            self.assertEqual(result.reason, "timeout")
            self.assertEqual(result.error_code, RUNTIME.TIMEOUT_ERROR)
            self.assertTrue(result.process_tree_stopped)
            detached = int(pid_path.read_text(encoding="ascii"))
            self.assertFalse(RUNTIME._pid_alive(detached))

    def test_linux_parent_loss_ends_tree_and_owned_cgroup(self) -> None:
        policy = self._integration_policy()
        self._require_linux_admission(policy)
        temporary_root = ROOT / "_local" / "tmp"
        temporary_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=temporary_root) as directory:
            directory_path = pathlib.Path(directory)
            child_pid_path = directory_path / "child.pid"
            child_cgroup_path = directory_path / "child.cgroup"
            helper_path = directory_path / "supervisor.py"
            policy_path = directory_path / "policy.json"
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            helper_path.write_text(
                "import importlib.util,json,os,pathlib,sys\n"
                f"root=pathlib.Path({str(ROOT)!r})\n"
                "spec=importlib.util.spec_from_file_location('runtime_under_crash',root/'scripts/management/clearra_runtime.py')\n"
                "module=importlib.util.module_from_spec(spec);sys.modules[spec.name]=module;spec.loader.exec_module(module)\n"
                f"pid_path=pathlib.Path({str(child_pid_path)!r})\n"
                f"cgroup_path=pathlib.Path({str(child_cgroup_path)!r})\n"
                "program=\"import os,pathlib,time;pathlib.Path(%r).write_text(str(os.getpid()),encoding='ascii');pathlib.Path(%r).write_text(next(line[3:] for line in pathlib.Path('/proc/self/cgroup').read_text().splitlines() if line.startswith('0::')),encoding='ascii');time.sleep(60)\" % (str(pid_path),str(cgroup_path))\n"
                f"policy=json.loads(pathlib.Path({str(policy_path)!r}).read_text(encoding='utf-8'))\n"
                "module.run_host_process([sys.executable,'-B','-c',program],cwd=root,env=os.environ.copy(),policy=policy,profile='control',echo=False)\n",
                encoding="utf-8",
            )
            supervisor = subprocess.Popen(
                [sys.executable, "-B", str(helper_path)],
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 10
            while (
                (not child_pid_path.is_file() or not child_cgroup_path.is_file())
                and time.monotonic() < deadline
            ):
                time.sleep(0.05)
            self.assertTrue(child_pid_path.is_file(), "child process did not start")
            self.assertTrue(child_cgroup_path.is_file(), "child cgroup was not reported")
            child_pid = int(child_pid_path.read_text(encoding="ascii"))
            relative_cgroup = child_cgroup_path.read_text(encoding="ascii").strip().lstrip("/")
            supervisor.kill()
            supervisor.wait(timeout=5)
            deadline = time.monotonic() + 10
            while RUNTIME._pid_alive(child_pid) and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertFalse(RUNTIME._pid_alive(child_pid))
            cgroup = pathlib.Path("/sys/fs/cgroup") / relative_cgroup
            if cgroup.name.startswith("clearra-"):
                deadline = time.monotonic() + 5
                while cgroup.exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertFalse(cgroup.exists(), "orphaned Clearra cgroup remains")

    def test_linux_owned_cgroup_reports_descendant_oom_without_retry(self) -> None:
        policy = self._integration_policy(memory_mib=96)
        self._require_linux_admission(policy)
        probe = RUNTIME._LinuxCgroup.create(96 * RUNTIME.MIB, 32)
        if probe is None:
            self.skipTest("a writable delegated cgroup is unavailable on this Linux host")
        self.assertTrue(probe.close())
        leaf = (
            "chunks=[]\n"
            "while True:\n"
            "    chunks.append(bytearray(16*1024*1024))\n"
            "    chunks[-1][0]=1\n"
        )
        middle = (
            "import subprocess,sys,time;"
            f"subprocess.Popen([sys.executable,'-B','-c',{leaf!r}]);"
            "time.sleep(60)"
        )
        program = (
            "import subprocess,sys,time;"
            f"subprocess.Popen([sys.executable,'-B','-c',{middle!r}]);"
            "time.sleep(60)"
        )
        result = RUNTIME.run_host_process(
            [sys.executable, "-B", "-c", program],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=policy,
            profile="control",
            timeout_override_seconds=5,
            echo=False,
        )
        self.assertEqual(result.reason, "oom")
        self.assertEqual(result.error_code, RUNTIME.MEMORY_LIMIT_ERROR)
        self.assertTrue(result.process_tree_stopped)

    def test_windows_job_normal_exit_records_tree_metrics(self) -> None:
        policy = self._integration_policy()
        self._require_windows_admission(policy)
        result = RUNTIME.run_host_process(
            [sys.executable, "-B", "-c", "print('bounded-runtime-ok')"],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=policy,
            profile="control",
            echo=False,
        )
        self.assertEqual(result.reason, "normal")
        self.assertIsNone(result.error_code)
        self.assertIn("bounded-runtime-ok", result.stdout)
        self.assertGreaterEqual(result.descendant_processes or 0, 1)
        self.assertEqual(result.containment["kind"], "windows-job-object")
        self.assertTrue(result.process_tree_stopped)

    def test_windows_nonzero_exit_is_receipted_without_signal_constants(self) -> None:
        policy = self._integration_policy()
        self._require_windows_admission(policy)
        result = RUNTIME.run_host_process(
            [sys.executable, "-B", "-c", "raise SystemExit(23)"],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=policy,
            profile="control",
            echo=False,
        )
        self.assertEqual(result.returncode, 23)
        self.assertEqual(result.reason, "nonzero")
        self.assertEqual(result.error_code, RUNTIME.NONZERO_ERROR)
        self.assertTrue(result.process_tree_stopped)

    def test_windows_output_limit_is_not_reported_as_oom(self) -> None:
        policy = self._integration_policy(output_limit=1024)
        self._require_windows_admission(policy)
        result = RUNTIME.run_host_process(
            [sys.executable, "-B", "-c", "import sys; sys.stdout.write('x'*65536)"],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=policy,
            profile="control",
            echo=False,
        )
        self.assertEqual(result.reason, "output-limit")
        self.assertEqual(result.error_code, RUNTIME.OUTPUT_LIMIT_ERROR)

    def test_windows_timeout_kills_term_ignoring_descendant(self) -> None:
        policy = self._integration_policy()
        self._require_windows_admission(policy)
        with tempfile.TemporaryDirectory(dir=ROOT / "_local" / "tmp") as directory:
            pid_path = pathlib.Path(directory) / "descendant.pid"
            program = (
                "import pathlib,subprocess,sys,time;"
                "p=subprocess.Popen([sys.executable,'-B','-c','import time;time.sleep(60)']);"
                f"pathlib.Path({str(pid_path)!r}).write_text(str(p.pid),encoding='ascii');"
                "time.sleep(60)"
            )
            result = RUNTIME.run_host_process(
                [sys.executable, "-B", "-c", program],
                cwd=ROOT,
                env=os.environ.copy(),
                policy=policy,
                profile="control",
                timeout_override_seconds=1,
                echo=False,
            )
            self.assertEqual(result.reason, "timeout")
            self.assertEqual(result.error_code, RUNTIME.TIMEOUT_ERROR)
            self.assertTrue(result.process_tree_stopped)
            descendant = int(pid_path.read_text(encoding="ascii"))
            self.assertFalse(RUNTIME._pid_alive(descendant))

    def test_windows_aggregate_job_limit_reports_descendant_oom_without_retry(self) -> None:
        policy = self._integration_policy(memory_mib=96)
        self._require_windows_admission(policy)
        leaf = (
            "chunks=[]\n"
            "while True:\n"
            "    chunks.append(bytearray(16*1024*1024))\n"
            "    chunks[-1][0]=1\n"
        )
        middle = (
            "import subprocess,sys,time;"
            f"subprocess.Popen([sys.executable,'-B','-c',{leaf!r}]);"
            "time.sleep(60)"
        )
        program = (
            "import subprocess,sys,time;"
            f"subprocess.Popen([sys.executable,'-B','-c',{middle!r}]);"
            "time.sleep(60)"
        )
        result = RUNTIME.run_host_process(
            [sys.executable, "-B", "-c", program],
            cwd=ROOT,
            env=os.environ.copy(),
            policy=policy,
            profile="control",
            timeout_override_seconds=5,
            echo=False,
        )
        self.assertEqual(result.reason, "oom")
        self.assertEqual(result.error_code, RUNTIME.MEMORY_LIMIT_ERROR)

    def test_windows_parent_loss_closes_job_and_kills_child(self) -> None:
        policy = self._integration_policy()
        self._require_windows_admission(policy)
        temporary_root = ROOT / "_local" / "tmp"
        temporary_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=temporary_root) as directory:
            directory_path = pathlib.Path(directory)
            child_pid_path = directory_path / "child.pid"
            helper_path = directory_path / "supervisor.py"
            policy_path = directory_path / "policy.json"
            policy_path.write_text(json.dumps(policy), encoding="utf-8")
            helper_path.write_text(
                "import importlib.util,json,os,pathlib,sys\n"
                f"root=pathlib.Path({str(ROOT)!r})\n"
                "spec=importlib.util.spec_from_file_location('runtime_under_crash',root/'scripts/management/clearra_runtime.py')\n"
                "module=importlib.util.module_from_spec(spec);sys.modules[spec.name]=module;spec.loader.exec_module(module)\n"
                f"pid_path=pathlib.Path({str(child_pid_path)!r})\n"
                "program=\"import pathlib,time;pathlib.Path(%r).write_text(str(__import__('os').getpid()),encoding='ascii');time.sleep(60)\" % str(pid_path)\n"
                f"policy=json.loads(pathlib.Path({str(policy_path)!r}).read_text(encoding='utf-8'))\n"
                "module.run_host_process([sys.executable,'-B','-c',program],cwd=root,env=os.environ.copy(),policy=policy,profile='control',echo=False)\n",
                encoding="utf-8",
            )
            supervisor = subprocess.Popen(
                [sys.executable, "-B", str(helper_path)],
                cwd=ROOT,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 10
            while not child_pid_path.is_file() and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertTrue(child_pid_path.is_file(), "child process did not start")
            child_pid = int(child_pid_path.read_text(encoding="ascii"))
            supervisor.kill()
            supervisor.wait(timeout=5)
            deadline = time.monotonic() + 5
            while RUNTIME._pid_alive(child_pid) and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertFalse(RUNTIME._pid_alive(child_pid))


if __name__ == "__main__":
    unittest.main()
