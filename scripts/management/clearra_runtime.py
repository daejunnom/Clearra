#!/usr/bin/env python3
"""Bounded process and dedicated-WSL runtime ownership for Clearra.

This module is deliberately standard-library only.  It is the single
production source file allowed to invoke WSL directly.  Callers use
``clearra_manage.py runtime`` so every process tree receives the same memory,
timeout, output, and cleanup contract.
"""

from __future__ import annotations

import contextlib
import ctypes
import ctypes.wintypes
import dataclasses
import datetime as dt
import hashlib
import io
import json
import os
import pathlib
import platform
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import threading
import time
import uuid
from typing import Any, BinaryIO, Callable, Mapping, Sequence


MIB = 1024 * 1024
GIB = 1024 * MIB
ADMISSION_ERROR = "E_CLEARRA_MEMORY_ADMISSION_DENIED"
CONCURRENCY_ERROR = "E_CLEARRA_RUNTIME_CONCURRENCY_LIMIT"
MEMORY_LIMIT_ERROR = "E_CLEARRA_PROCESS_MEMORY_LIMIT"
TIMEOUT_ERROR = "E_CLEARRA_PROCESS_TIMEOUT"
OUTPUT_LIMIT_ERROR = "E_CLEARRA_PROCESS_OUTPUT_LIMIT"
CANCELLED_ERROR = "E_CLEARRA_PROCESS_CANCELLED"
PARENT_LOST_ERROR = "E_CLEARRA_PROCESS_PARENT_LOST"
NONZERO_ERROR = "E_CLEARRA_PROCESS_NONZERO_EXIT"
TREE_CLEANUP_ERROR = "E_CLEARRA_PROCESS_TREE_CLEANUP"
WSL_DISTRIBUTION = "Clearra-Build"
SIGTERM_NUMBER = int(getattr(signal, "SIGTERM", 15))
SIGKILL_NUMBER = int(getattr(signal, "SIGKILL", 9))


class RuntimePolicyError(RuntimeError):
    """The runtime contract could not safely start or finish a process."""

    def __init__(self, message: str, *, details: Mapping[str, Any] | None = None) -> None:
        super().__init__(message)
        self.details: dict[str, Any] = dict(details or {})


@dataclasses.dataclass(frozen=True)
class MemorySnapshot:
    physical_bytes: int
    available_bytes: int
    commit_limit_bytes: int | None = None
    commit_available_bytes: int | None = None


@dataclasses.dataclass(frozen=True)
class Admission:
    profile: str
    physical_bytes: int
    available_bytes: int
    reserve_bytes: int
    hard_limit_bytes: int
    minimum_bytes: int
    maximum_bytes: int | None
    capacity_basis: str = "physical"
    commit_limit_bytes: int | None = None
    commit_available_bytes: int | None = None
    commit_reserve_bytes: int | None = None
    gc_recovery_headroom_bytes: int = 0
    gc_recovery_headroom_backing: str = "none"


@dataclasses.dataclass
class RuntimeResult:
    command: list[str]
    command_sha256: str
    returncode: int
    reason: str
    error_code: str | None
    started_utc: str
    ended_utc: str
    duration_ms: int
    timeout_seconds: int
    termination_stage: str
    stdout: str
    stderr: str
    output_bytes: int
    output_limit_bytes: int
    admission: dict[str, Any]
    containment: dict[str, Any]
    peak_memory_bytes: int | None
    descendant_processes: int | None
    oom_counter_before: int | None
    oom_counter_after: int | None
    process_tree_stopped: bool

    def receipt(self) -> dict[str, Any]:
        value = dataclasses.asdict(self)
        # Child output can contain environment-specific paths or accidental
        # sensitive material.  It is returned to the caller but never placed
        # in the durable receipt.
        value.pop("stdout", None)
        value.pop("stderr", None)
        return value


def runtime_failure_summary(label: str, result: RuntimeResult) -> str:
    """Describe a child failure without copying its output into a receipt."""
    return (
        f"{label}: returncode={result.returncode} reason={result.reason} "
        f"error_code={result.error_code or 'none'}"
    )


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def command_digest(command: Sequence[str]) -> str:
    # A low-entropy password or token must not become guessable through a
    # durable command hash. The digest identifies the same redacted shape that
    # is written to the receipt.
    material = json.dumps(redact_argv(command), ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(material.encode("utf-8")).hexdigest()


def redact_argv(command: Sequence[str]) -> list[str]:
    redacted: list[str] = []
    redact_next = False
    secret_option = re.compile(
        r"(?:token|secret|password|credential|api[-_]?key|private[-_]?key|ssh[-_]?key)", re.I
    )
    secret_path = re.compile(
        r"(?:^|[\\/])(?:\.env(?:\.[^\\/]*)?|id_(?:rsa|ed25519|ecdsa)(?:\.pub)?|"
        r"[^\\/]*(?:service[-_]?account|credential)[^\\/]*\.json|"
        r"[^\\/]*(?:ssh|private)[-_]?key[^\\/]*|[^\\/]+\.(?:pem|p12|pfx))$",
        re.I,
    )
    for argument in command:
        if redact_next:
            redacted.append("<redacted>")
            redact_next = False
            continue
        if "=" in argument and secret_option.search(argument.split("=", 1)[0]):
            redacted.append(argument.split("=", 1)[0] + "=<redacted>")
            continue
        if secret_option.search(argument):
            if argument.startswith("-"):
                redacted.append(argument)
                redact_next = True
            else:
                redacted.append("<redacted>")
            continue
        if secret_path.search(argument):
            redacted.append("<redacted>")
            continue
        redacted.append(argument)
    return redacted


def memory_snapshot() -> MemorySnapshot:
    if os.name == "nt":
        class MEMORYSTATUSEX(ctypes.Structure):
            _fields_ = [
                ("dwLength", ctypes.wintypes.DWORD),
                ("dwMemoryLoad", ctypes.wintypes.DWORD),
                ("ullTotalPhys", ctypes.c_ulonglong),
                ("ullAvailPhys", ctypes.c_ulonglong),
                ("ullTotalPageFile", ctypes.c_ulonglong),
                ("ullAvailPageFile", ctypes.c_ulonglong),
                ("ullTotalVirtual", ctypes.c_ulonglong),
                ("ullAvailVirtual", ctypes.c_ulonglong),
                ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
            ]

        status = MEMORYSTATUSEX()
        status.dwLength = ctypes.sizeof(status)
        if not ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)):
            raise RuntimePolicyError("could not read Windows memory status")
        return MemorySnapshot(
            int(status.ullTotalPhys),
            int(status.ullAvailPhys),
            int(status.ullTotalPageFile),
            int(status.ullAvailPageFile),
        )

    meminfo: dict[str, int] = {}
    try:
        for line in pathlib.Path("/proc/meminfo").read_text(encoding="ascii").splitlines():
            key, value = line.split(":", 1)
            amount = int(value.strip().split()[0]) * 1024
            meminfo[key] = amount
    except (OSError, ValueError, IndexError) as error:
        raise RuntimePolicyError("could not read Linux memory status") from error
    physical = meminfo.get("MemTotal", 0)
    available = meminfo.get("MemAvailable", meminfo.get("MemFree", 0))
    supervisor_cgroup = _linux_current_cgroup()
    cgroup = supervisor_cgroup
    if cgroup is not None:
        cgroup_limit = _read_int_or_max(cgroup / "memory.max")
        cgroup_current = _read_int_or_max(cgroup / "memory.current")
        if cgroup_limit is not None and cgroup_limit > 0:
            physical = min(physical, cgroup_limit)
            if cgroup_current is not None:
                available = min(available, max(0, cgroup_limit - cgroup_current))
    if physical <= 0 or available < 0:
        raise RuntimePolicyError("Linux memory status is incomplete")
    commit_limit = meminfo.get("CommitLimit")
    committed = meminfo.get("Committed_AS")
    commit_available = (
        max(0, commit_limit - committed)
        if commit_limit is not None and committed is not None
        else None
    )
    return MemorySnapshot(physical, available, commit_limit, commit_available)


def profile_contract(policy: Mapping[str, Any], profile: str) -> Mapping[str, Any]:
    profiles = policy.get("resource_profiles")
    if not isinstance(profiles, Mapping) or profile not in profiles:
        raise RuntimePolicyError(f"unknown runtime resource profile: {profile}")
    contract = profiles[profile]
    if not isinstance(contract, Mapping):
        raise RuntimePolicyError(f"invalid runtime resource profile: {profile}")
    return contract


def calculate_admission(
    policy: Mapping[str, Any],
    profile: str,
    *,
    snapshot: MemorySnapshot | None = None,
    minimum_override_mib: int | None = None,
    platform_name: str | None = None,
) -> Admission:
    contract = profile_contract(policy, profile)
    current = snapshot or memory_snapshot()
    runtime_policy = policy.get("runtime_policy", {})
    reserve_policy = runtime_policy.get("host_reserve", {})
    platform_value = os.name if platform_name is None else platform_name
    admission_basis = str(contract.get("admission_basis") or "physical")
    windows_commit_policy = {
        "windows-commit-control": "windows_commit_control",
        "windows-commit-build-test": "windows_commit_build_test",
    }.get(admission_basis)
    use_windows_commit = (
        platform_value == "nt"
        and windows_commit_policy is not None
        and current.commit_limit_bytes is not None
        and current.commit_available_bytes is not None
    )
    if use_windows_commit:
        commit_policy = runtime_policy.get(str(windows_commit_policy), {})
        reserve = max(
            int(commit_policy.get("minimum_physical_reserve_mib", 1024)) * MIB,
            int(
                current.physical_bytes
                * float(commit_policy.get("physical_reserve_fraction", 0.0625))
            ),
        )
        commit_reserve = max(
            int(commit_policy.get("minimum_commit_reserve_mib", 4096)) * MIB,
            int(
                current.commit_limit_bytes
                * float(commit_policy.get("commit_reserve_fraction", 0.125))
            ),
        )
        capacity_basis = admission_basis
    else:
        reserve_floor = int(reserve_policy.get("minimum_mib", 2048)) * MIB
        reserve_fraction = float(reserve_policy.get("physical_fraction", 0.20))
        reserve = max(reserve_floor, int(current.physical_bytes * reserve_fraction))
        commit_reserve = None
        capacity_basis = "physical"
    minimum = int(contract["minimum_memory_mib"]) * MIB
    if minimum_override_mib is not None:
        minimum = max(minimum, int(minimum_override_mib) * MIB)
    gc_recovery_headroom = int(contract.get("gc_recovery_headroom_mib", 0)) * MIB
    required_hard_limit = minimum + gc_recovery_headroom
    configured_max = contract.get("maximum_memory_mib")
    maximum = int(configured_max) * MIB if configured_max is not None else None
    if maximum is not None and maximum < required_hard_limit:
        raise RuntimePolicyError(
            f"runtime profile cannot preserve its GC recovery headroom: {profile}"
        )
    commit_backed_gc = (
        gc_recovery_headroom
        if capacity_basis == "windows-commit-build-test"
        else 0
    )
    gc_recovery_backing = (
        "windows-commit"
        if commit_backed_gc
        else "physical"
        if gc_recovery_headroom
        else "none"
    )
    candidates = [
        current.physical_bytes - reserve + commit_backed_gc,
        current.available_bytes - reserve + commit_backed_gc,
    ]
    if use_windows_commit:
        assert current.commit_limit_bytes is not None
        assert current.commit_available_bytes is not None
        assert commit_reserve is not None
        candidates.extend(
            [
                current.commit_limit_bytes - commit_reserve,
                current.commit_available_bytes - commit_reserve,
            ]
        )
    if maximum is not None:
        candidates.append(maximum)
    hard_limit = max(0, min(candidates))
    if hard_limit < required_hard_limit:
        raise RuntimePolicyError(
            f"{ADMISSION_ERROR}: profile={profile} minimum_bytes={minimum} "
            f"gc_recovery_headroom_bytes={gc_recovery_headroom} "
            f"gc_recovery_headroom_backing={gc_recovery_backing} "
            f"required_hard_limit_bytes={required_hard_limit} "
            f"available_bytes={current.available_bytes} reserve_bytes={reserve} "
            f"commit_available_bytes={current.commit_available_bytes} "
            f"commit_reserve_bytes={commit_reserve} capacity_basis={capacity_basis} "
            f"admitted_bytes={hard_limit}"
        )
    return Admission(
        profile=profile,
        physical_bytes=current.physical_bytes,
        available_bytes=current.available_bytes,
        reserve_bytes=reserve,
        hard_limit_bytes=hard_limit,
        minimum_bytes=minimum,
        maximum_bytes=maximum,
        capacity_basis=capacity_basis,
        commit_limit_bytes=current.commit_limit_bytes,
        commit_available_bytes=current.commit_available_bytes,
        commit_reserve_bytes=commit_reserve,
        gc_recovery_headroom_bytes=gc_recovery_headroom,
        gc_recovery_headroom_backing=gc_recovery_backing,
    )


def _runtime_state_root() -> pathlib.Path:
    configured = os.environ.get("CLEARRA_STATE_ROOT")
    if configured:
        return pathlib.Path(configured)
    if os.name == "nt":
        local_app_data = os.environ.get("LOCALAPPDATA")
        base = pathlib.Path(local_app_data) if local_app_data else pathlib.Path.home() / "AppData" / "Local"
        return base / "Clearra" / "state"
    xdg_state = os.environ.get("XDG_STATE_HOME")
    base = pathlib.Path(xdg_state) if xdg_state else pathlib.Path.home() / ".local" / "state"
    return base / "Clearra"


def _process_start_token(pid: int) -> str | None:
    if pid <= 0:
        return None
    if os.name == "nt":
        process_query_limited_information = 0x1000
        kernel32 = ctypes.windll.kernel32
        kernel32.OpenProcess.argtypes = [
            ctypes.wintypes.DWORD,
            ctypes.wintypes.BOOL,
            ctypes.wintypes.DWORD,
        ]
        kernel32.OpenProcess.restype = ctypes.wintypes.HANDLE
        kernel32.GetProcessTimes.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.POINTER(ctypes.wintypes.FILETIME),
            ctypes.POINTER(ctypes.wintypes.FILETIME),
            ctypes.POINTER(ctypes.wintypes.FILETIME),
            ctypes.POINTER(ctypes.wintypes.FILETIME),
        ]
        kernel32.GetProcessTimes.restype = ctypes.wintypes.BOOL
        kernel32.CloseHandle.argtypes = [ctypes.wintypes.HANDLE]
        kernel32.CloseHandle.restype = ctypes.wintypes.BOOL
        handle = kernel32.OpenProcess(process_query_limited_information, False, pid)
        if not handle:
            return None
        try:
            creation = ctypes.wintypes.FILETIME()
            exit_time = ctypes.wintypes.FILETIME()
            kernel = ctypes.wintypes.FILETIME()
            user = ctypes.wintypes.FILETIME()
            if not kernel32.GetProcessTimes(
                handle,
                ctypes.byref(creation),
                ctypes.byref(exit_time),
                ctypes.byref(kernel),
                ctypes.byref(user),
            ):
                return None
            return f"windows:{creation.dwHighDateTime:08x}{creation.dwLowDateTime:08x}"
        finally:
            kernel32.CloseHandle(handle)
    try:
        material = pathlib.Path(f"/proc/{pid}/stat").read_text(encoding="ascii")
        close = material.rfind(")")
        remainder = material[close + 2 :].split()
        return f"linux:{remainder[19]}" if close >= 0 else None
    except (OSError, IndexError):
        return None


def _runtime_slot_owner_active(owner: Mapping[str, Any]) -> bool:
    try:
        pid = int(owner["pid"])
    except (KeyError, TypeError, ValueError):
        return False
    if not _pid_alive(pid):
        return False
    expected = owner.get("process_start_token")
    actual = _process_start_token(pid)
    return expected is None or actual is None or str(expected) == actual


def _recover_stale_runtime_slot(path: pathlib.Path, grace_seconds: float) -> bool:
    marker = path / "owner.json"
    try:
        age = max(0.0, time.time() - path.stat().st_mtime)
    except OSError:
        return True
    owner: Mapping[str, Any] | None = None
    if marker.is_file():
        try:
            value = json.loads(marker.read_text(encoding="utf-8"))
            owner = value if isinstance(value, Mapping) else None
        except (OSError, json.JSONDecodeError):
            owner = None
    if owner is not None and _runtime_slot_owner_active(owner):
        return False
    if owner is None and age < grace_seconds:
        return False
    try:
        entries = {entry.name for entry in path.iterdir()}
    except OSError:
        return True
    if not entries.issubset({"owner.json"}):
        return False
    with contextlib.suppress(FileNotFoundError):
        marker.unlink()
    try:
        path.rmdir()
    except OSError:
        return False
    return True


@dataclasses.dataclass
class _RuntimeSlot:
    path: pathlib.Path
    nonce: str
    class_id: str
    index: int

    @classmethod
    def acquire(
        cls,
        policy: Mapping[str, Any],
        profile: str,
        admission: Admission,
    ) -> "_RuntimeSlot":
        contract = profile_contract(policy, profile)
        class_id = str(contract.get("concurrency_class", profile))
        if not re.fullmatch(r"[a-z0-9-]+", class_id):
            raise RuntimePolicyError(f"invalid runtime concurrency class: {class_id}")
        parallel = policy.get("runtime_policy", {}).get("parallel_admission", {})
        classes = parallel.get("classes", {}) if isinstance(parallel, Mapping) else {}
        class_contract = classes.get(class_id, {}) if isinstance(classes, Mapping) else {}
        maximum = int(class_contract.get("maximum_parallel", 0))
        if maximum <= 0:
            raise RuntimePolicyError(f"runtime concurrency class is not configured: {class_id}")
        stale_grace = float(parallel.get("stale_slot_grace_seconds", 30))
        root = _runtime_state_root() / "runtime-slots" / class_id
        root.mkdir(parents=True, exist_ok=True)
        owner_pid = os.getpid()
        token = _process_start_token(owner_pid)
        active: list[dict[str, Any]] = []
        for index in range(maximum):
            path = root / f"slot-{index}"
            for _attempt in range(2):
                try:
                    path.mkdir()
                except FileExistsError:
                    if _recover_stale_runtime_slot(path, stale_grace):
                        continue
                    marker = path / "owner.json"
                    try:
                        value = json.loads(marker.read_text(encoding="utf-8"))
                        if isinstance(value, Mapping):
                            active.append(
                                {
                                    "slot": index,
                                    "pid": value.get("pid"),
                                    "profile": value.get("profile"),
                                }
                            )
                    except (OSError, json.JSONDecodeError):
                        active.append({"slot": index, "pid": None, "profile": None})
                    break
                nonce = uuid.uuid4().hex
                owner = {
                    "schema_id": "clearra.runtime-slot-owner.v1",
                    "pid": owner_pid,
                    "process_start_token": token,
                    "nonce": nonce,
                    "profile": profile,
                    "class": class_id,
                    "slot": index,
                    "hard_limit_bytes": admission.hard_limit_bytes,
                    "created_utc": utc_now(),
                }
                try:
                    (path / "owner.json").write_text(
                        json.dumps(owner, ensure_ascii=False, separators=(",", ":")),
                        encoding="utf-8",
                    )
                except BaseException:
                    with contextlib.suppress(OSError):
                        (path / "owner.json").unlink()
                    with contextlib.suppress(OSError):
                        path.rmdir()
                    raise
                return cls(path=path, nonce=nonce, class_id=class_id, index=index)
        raise RuntimePolicyError(
            f"{CONCURRENCY_ERROR}: profile={profile} class={class_id} "
            f"maximum_parallel={maximum}",
            details={"active_slots": active},
        )

    def release(self) -> None:
        marker = self.path / "owner.json"
        try:
            owner = json.loads(marker.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return
        if not isinstance(owner, Mapping) or owner.get("nonce") != self.nonce:
            return
        with contextlib.suppress(FileNotFoundError):
            marker.unlink()
        with contextlib.suppress(OSError):
            self.path.rmdir()


class _OutputCollector:
    def __init__(self, limit: int, echo: bool) -> None:
        self.limit = limit
        self.echo = echo
        self.total = 0
        self.exceeded = threading.Event()
        self._lock = threading.Lock()
        self._stdout = bytearray()
        self._stderr = bytearray()

    def reader(self, stream: BinaryIO, destination: bytearray, mirror: BinaryIO) -> None:
        try:
            while True:
                chunk = stream.read(65536)
                if not chunk:
                    return
                with self._lock:
                    self.total += len(chunk)
                    remaining = max(0, self.limit - len(self._stdout) - len(self._stderr))
                    accepted = chunk[:remaining]
                    destination.extend(accepted)
                    if self.total > self.limit:
                        self.exceeded.set()
                if self.echo and accepted:
                    mirror.write(accepted)
                    mirror.flush()
        finally:
            with contextlib.suppress(Exception):
                stream.close()

    def start(self, process: subprocess.Popen[bytes]) -> list[threading.Thread]:
        assert process.stdout is not None and process.stderr is not None
        threads = [
            threading.Thread(
                target=self.reader,
                args=(process.stdout, self._stdout, sys.stdout.buffer),
                daemon=True,
            ),
            threading.Thread(
                target=self.reader,
                args=(process.stderr, self._stderr, sys.stderr.buffer),
                daemon=True,
            ),
        ]
        for thread in threads:
            thread.start()
        return threads

    def finish(self, threads: Sequence[threading.Thread]) -> tuple[str, str]:
        for thread in threads:
            thread.join(timeout=5)
        return (
            self._stdout.decode("utf-8", errors="replace"),
            self._stderr.decode("utf-8", errors="replace"),
        )


if os.name == "nt":
    ULONG_PTR = ctypes.c_size_t
    SIZE_T = ctypes.c_size_t

    class _IO_COUNTERS(ctypes.Structure):
        _fields_ = [
            ("ReadOperationCount", ctypes.c_ulonglong),
            ("WriteOperationCount", ctypes.c_ulonglong),
            ("OtherOperationCount", ctypes.c_ulonglong),
            ("ReadTransferCount", ctypes.c_ulonglong),
            ("WriteTransferCount", ctypes.c_ulonglong),
            ("OtherTransferCount", ctypes.c_ulonglong),
        ]

    class _JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes.c_longlong),
            ("PerJobUserTimeLimit", ctypes.c_longlong),
            ("LimitFlags", ctypes.wintypes.DWORD),
            ("MinimumWorkingSetSize", SIZE_T),
            ("MaximumWorkingSetSize", SIZE_T),
            ("ActiveProcessLimit", ctypes.wintypes.DWORD),
            ("Affinity", ULONG_PTR),
            ("PriorityClass", ctypes.wintypes.DWORD),
            ("SchedulingClass", ctypes.wintypes.DWORD),
        ]

    class _JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("BasicLimitInformation", _JOBOBJECT_BASIC_LIMIT_INFORMATION),
            ("IoInfo", _IO_COUNTERS),
            ("ProcessMemoryLimit", SIZE_T),
            ("JobMemoryLimit", SIZE_T),
            ("PeakProcessMemoryUsed", SIZE_T),
            ("PeakJobMemoryUsed", SIZE_T),
        ]

    class _JOBOBJECT_ASSOCIATE_COMPLETION_PORT(ctypes.Structure):
        _fields_ = [("CompletionKey", ctypes.c_void_p), ("CompletionPort", ctypes.c_void_p)]

    class _JOBOBJECT_BASIC_ACCOUNTING_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("TotalUserTime", ctypes.c_longlong),
            ("TotalKernelTime", ctypes.c_longlong),
            ("ThisPeriodTotalUserTime", ctypes.c_longlong),
            ("ThisPeriodTotalKernelTime", ctypes.c_longlong),
            ("TotalPageFaultCount", ctypes.wintypes.DWORD),
            ("TotalProcesses", ctypes.wintypes.DWORD),
            ("ActiveProcesses", ctypes.wintypes.DWORD),
            ("TotalTerminatedProcesses", ctypes.wintypes.DWORD),
        ]

    class _THREADENTRY32(ctypes.Structure):
        _fields_ = [
            ("dwSize", ctypes.wintypes.DWORD),
            ("cntUsage", ctypes.wintypes.DWORD),
            ("th32ThreadID", ctypes.wintypes.DWORD),
            ("th32OwnerProcessID", ctypes.wintypes.DWORD),
            ("tpBasePri", ctypes.c_long),
            ("tpDeltaPri", ctypes.c_long),
            ("dwFlags", ctypes.wintypes.DWORD),
        ]


class _WindowsJob:
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS = 0x00000008
    JOB_OBJECT_LIMIT_JOB_MEMORY = 0x00000200
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000
    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
    JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION = 1
    JOB_OBJECT_ASSOCIATE_COMPLETION_PORT = 7
    JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT = 3
    JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT = 9
    JOB_OBJECT_MSG_JOB_MEMORY_LIMIT = 10
    INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
    TH32CS_SNAPTHREAD = 0x00000004
    THREAD_SUSPEND_RESUME = 0x0002

    def __init__(self, memory_limit: int, process_limit: int) -> None:
        if os.name != "nt":
            raise RuntimePolicyError("Windows Job Objects are available only on Windows")
        kernel32 = ctypes.windll.kernel32
        kernel32.CreateJobObjectW.restype = ctypes.wintypes.HANDLE
        kernel32.CreateJobObjectW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p]
        kernel32.CreateIoCompletionPort.restype = ctypes.wintypes.HANDLE
        kernel32.CreateIoCompletionPort.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.wintypes.HANDLE,
            ULONG_PTR,
            ctypes.wintypes.DWORD,
        ]
        kernel32.SetInformationJobObject.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.wintypes.DWORD,
        ]
        kernel32.SetInformationJobObject.restype = ctypes.wintypes.BOOL
        kernel32.GetQueuedCompletionStatus.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.POINTER(ctypes.wintypes.DWORD),
            ctypes.POINTER(ULONG_PTR),
            ctypes.POINTER(ctypes.c_void_p),
            ctypes.wintypes.DWORD,
        ]
        kernel32.GetQueuedCompletionStatus.restype = ctypes.wintypes.BOOL
        kernel32.AssignProcessToJobObject.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.wintypes.HANDLE,
        ]
        kernel32.AssignProcessToJobObject.restype = ctypes.wintypes.BOOL
        kernel32.CreateToolhelp32Snapshot.argtypes = [
            ctypes.wintypes.DWORD,
            ctypes.wintypes.DWORD,
        ]
        kernel32.CreateToolhelp32Snapshot.restype = ctypes.wintypes.HANDLE
        kernel32.Thread32First.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.POINTER(_THREADENTRY32),
        ]
        kernel32.Thread32First.restype = ctypes.wintypes.BOOL
        kernel32.Thread32Next.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.POINTER(_THREADENTRY32),
        ]
        kernel32.Thread32Next.restype = ctypes.wintypes.BOOL
        kernel32.OpenThread.argtypes = [
            ctypes.wintypes.DWORD,
            ctypes.wintypes.BOOL,
            ctypes.wintypes.DWORD,
        ]
        kernel32.OpenThread.restype = ctypes.wintypes.HANDLE
        kernel32.ResumeThread.argtypes = [ctypes.wintypes.HANDLE]
        kernel32.ResumeThread.restype = ctypes.wintypes.DWORD
        kernel32.TerminateJobObject.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.wintypes.UINT,
        ]
        kernel32.TerminateJobObject.restype = ctypes.wintypes.BOOL
        kernel32.QueryInformationJobObject.argtypes = [
            ctypes.wintypes.HANDLE,
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.wintypes.DWORD,
            ctypes.POINTER(ctypes.wintypes.DWORD),
        ]
        kernel32.QueryInformationJobObject.restype = ctypes.wintypes.BOOL
        kernel32.CloseHandle.argtypes = [ctypes.wintypes.HANDLE]
        kernel32.CloseHandle.restype = ctypes.wintypes.BOOL
        self._kernel32 = kernel32
        self.handle = kernel32.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError()
        self.port = kernel32.CreateIoCompletionPort(
            ctypes.c_void_p(self.INVALID_HANDLE_VALUE), None, ULONG_PTR(0), 1
        )
        if not self.port:
            self.close()
            raise ctypes.WinError()
        limits = _JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        limits.BasicLimitInformation.LimitFlags = (
            self.JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | self.JOB_OBJECT_LIMIT_JOB_MEMORY
            | self.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        )
        limits.BasicLimitInformation.ActiveProcessLimit = process_limit
        limits.JobMemoryLimit = memory_limit
        if not kernel32.SetInformationJobObject(
            self.handle,
            self.JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            ctypes.byref(limits),
            ctypes.sizeof(limits),
        ):
            self.close()
            raise ctypes.WinError()
        association = _JOBOBJECT_ASSOCIATE_COMPLETION_PORT(None, self.port)
        if not kernel32.SetInformationJobObject(
            self.handle,
            self.JOB_OBJECT_ASSOCIATE_COMPLETION_PORT,
            ctypes.byref(association),
            ctypes.sizeof(association),
        ):
            self.close()
            raise ctypes.WinError()
        self.messages: list[int] = []
        self._stop = threading.Event()
        self._monitor = threading.Thread(target=self._monitor_messages, daemon=True)
        self._monitor.start()

    def _monitor_messages(self) -> None:
        message = ctypes.wintypes.DWORD()
        key = ULONG_PTR()
        overlapped = ctypes.c_void_p()
        while not self._stop.is_set():
            ok = self._kernel32.GetQueuedCompletionStatus(
                self.port,
                ctypes.byref(message),
                ctypes.byref(key),
                ctypes.byref(overlapped),
                200,
            )
            if ok:
                value = int(message.value)
                self.messages.append(value)
                if value in {
                    self.JOB_OBJECT_MSG_ACTIVE_PROCESS_LIMIT,
                    self.JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT,
                    self.JOB_OBJECT_MSG_JOB_MEMORY_LIMIT,
                } and self.handle:
                    # A hard resource boundary is atomic for the whole task.
                    # Do not leave siblings running after one member exceeds it.
                    self._kernel32.TerminateJobObject(self.handle, 137)

    def assign_and_resume(self, process: subprocess.Popen[bytes]) -> None:
        if not self._kernel32.AssignProcessToJobObject(self.handle, process._handle):
            raise ctypes.WinError()
        snapshot = self._kernel32.CreateToolhelp32Snapshot(self.TH32CS_SNAPTHREAD, 0)
        if snapshot == self.INVALID_HANDLE_VALUE:
            raise ctypes.WinError()
        try:
            entry = _THREADENTRY32()
            entry.dwSize = ctypes.sizeof(entry)
            found = self._kernel32.Thread32First(snapshot, ctypes.byref(entry))
            thread_id = None
            while found:
                if int(entry.th32OwnerProcessID) == process.pid:
                    thread_id = int(entry.th32ThreadID)
                    break
                found = self._kernel32.Thread32Next(snapshot, ctypes.byref(entry))
            if thread_id is None:
                raise RuntimePolicyError("could not locate the suspended child thread")
            thread = self._kernel32.OpenThread(self.THREAD_SUSPEND_RESUME, False, thread_id)
            if not thread:
                raise ctypes.WinError()
            try:
                if self._kernel32.ResumeThread(thread) == 0xFFFFFFFF:
                    raise ctypes.WinError()
            finally:
                self._kernel32.CloseHandle(thread)
        finally:
            self._kernel32.CloseHandle(snapshot)

    def terminate(self, code: int = 137) -> None:
        if self.handle:
            self._kernel32.TerminateJobObject(self.handle, code)

    def wait_until_empty(self, timeout: float) -> bool:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            _peak, _total, active, _oom = self.metrics()
            if active == 0:
                return True
            time.sleep(0.05)
        _peak, _total, active, _oom = self.metrics()
        return active == 0

    def metrics(self) -> tuple[int | None, int | None, int | None, bool]:
        if not self.handle:
            return None, None, None, False
        limits = _JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        returned = ctypes.wintypes.DWORD()
        ok = self._kernel32.QueryInformationJobObject(
            self.handle,
            self.JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            ctypes.byref(limits),
            ctypes.sizeof(limits),
            ctypes.byref(returned),
        )
        peak = int(limits.PeakJobMemoryUsed) if ok else None
        accounting = _JOBOBJECT_BASIC_ACCOUNTING_INFORMATION()
        accounting_ok = self._kernel32.QueryInformationJobObject(
            self.handle,
            self.JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
            ctypes.byref(accounting),
            ctypes.sizeof(accounting),
            ctypes.byref(returned),
        )
        descendants = int(accounting.TotalProcesses) if accounting_ok else None
        active = int(accounting.ActiveProcesses) if accounting_ok else None
        oom = any(
            message in {self.JOB_OBJECT_MSG_PROCESS_MEMORY_LIMIT, self.JOB_OBJECT_MSG_JOB_MEMORY_LIMIT}
            for message in self.messages
        )
        return peak, descendants, active, oom

    def close(self) -> None:
        self._stop.set()
        if getattr(self, "port", None):
            self._kernel32.CloseHandle(self.port)
            self.port = None
        if getattr(self, "handle", None):
            self._kernel32.CloseHandle(self.handle)
            self.handle = None
        monitor = getattr(self, "_monitor", None)
        if monitor is not None and monitor.is_alive():
            monitor.join(timeout=1)


def _terminate_windows_job(
    process: subprocess.Popen[bytes],
    job: _WindowsJob,
    grace: float,
    code: int = 137,
) -> str:
    break_sent = False
    with contextlib.suppress(OSError, ValueError):
        process.send_signal(signal.CTRL_BREAK_EVENT)
        break_sent = True
    if break_sent:
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            _peak, _total, active, _oom = job.metrics()
            if active == 0:
                return "ctrl-break"
            time.sleep(0.05)
    job.terminate(code)
    job.wait_until_empty(max(5.0, grace))
    return "job-kill"


def _linux_current_cgroup() -> pathlib.Path | None:
    if os.name == "nt":
        return None
    try:
        for line in pathlib.Path("/proc/self/cgroup").read_text(encoding="ascii").splitlines():
            if line.startswith("0::"):
                relative = line[3:].lstrip("/")
                return pathlib.Path("/sys/fs/cgroup") / relative
    except OSError:
        return None
    return None


def _read_int_or_max(path: pathlib.Path) -> int | None:
    try:
        value = path.read_text(encoding="ascii").strip()
        return None if value == "max" else int(value)
    except (OSError, ValueError):
        return None


def _memory_events(path: pathlib.Path | None) -> dict[str, int]:
    if path is None:
        return {}
    result: dict[str, int] = {}
    try:
        for line in (path / "memory.events").read_text(encoding="ascii").splitlines():
            key, value = line.split()
            result[key] = int(value)
    except (OSError, ValueError):
        return {}
    return result


class _LinuxCgroup:
    """Best-effort delegated cgroup-v2 owner for one process tree."""

    def __init__(self, path: pathlib.Path) -> None:
        self.path = path

    @classmethod
    def create(cls, memory_limit: int, process_limit: int) -> "_LinuxCgroup | None":
        if os.name == "nt":
            return None
        parent = _linux_current_cgroup()
        if parent is None:
            return None
        path = parent / f"clearra-{os.getpid()}-{uuid.uuid4().hex[:12]}"
        try:
            path.mkdir(mode=0o700)
            required = (
                path / "memory.max",
                path / "memory.swap.max",
                path / "memory.oom.group",
                path / "pids.max",
                path / "cgroup.procs",
            )
            if not all(item.exists() for item in required):
                raise OSError("delegated memory and pids controllers are unavailable")
            (path / "memory.max").write_text(str(memory_limit), encoding="ascii")
            (path / "memory.swap.max").write_text("0", encoding="ascii")
            (path / "memory.oom.group").write_text("1", encoding="ascii")
            (path / "pids.max").write_text(str(process_limit), encoding="ascii")
            return cls(path)
        except OSError:
            with contextlib.suppress(OSError):
                path.rmdir()
            return None

    def child_setup(self) -> None:
        os.setsid()
        (self.path / "cgroup.procs").write_text(str(os.getpid()), encoding="ascii")

    def kill(self) -> None:
        kill_file = self.path / "cgroup.kill"
        if kill_file.is_file():
            with contextlib.suppress(OSError):
                kill_file.write_text("1", encoding="ascii")
                return
        try:
            pids = [int(value) for value in (self.path / "cgroup.procs").read_text(
                encoding="ascii"
            ).split()]
        except (OSError, ValueError):
            pids = []
        for pid in pids:
            with contextlib.suppress(ProcessLookupError):
                os.kill(pid, SIGKILL_NUMBER)

    def metrics(self) -> tuple[int | None, int | None, dict[str, int]]:
        peak = _read_int_or_max(self.path / "memory.peak")
        process_peak = _read_int_or_max(self.path / "pids.peak")
        return peak, process_peak, _memory_events(self.path)

    def close(self) -> bool:
        self.kill()
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            try:
                if not (self.path / "cgroup.procs").read_text(encoding="ascii").strip():
                    break
            except OSError:
                break
            time.sleep(0.05)
        with contextlib.suppress(OSError):
            self.path.rmdir()
        return not self.path.exists()


def _terminate_posix_group(process: subprocess.Popen[bytes], grace: float) -> str:
    with contextlib.suppress(ProcessLookupError):
        os.killpg(process.pid, SIGTERM_NUMBER)
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline:
        process.poll()
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return "term"
        time.sleep(0.05)
    try:
        os.killpg(process.pid, SIGKILL_NUMBER)
        stage = "kill"
    except ProcessLookupError:
        stage = "term"
    if process.poll() is None:
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
    return stage


def _posix_group_alive(process_group: int) -> bool:
    try:
        os.killpg(process_group, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def run_host_process(
    command: Sequence[str],
    *,
    cwd: pathlib.Path,
    env: Mapping[str, str],
    policy: Mapping[str, Any],
    profile: str,
    input_bytes: bytes | None = None,
    keep_stdin_open: bool = False,
    echo: bool = True,
    minimum_override_mib: int | None = None,
    timeout_override_seconds: int | None = None,
    cleanup_only: bool = False,
    monitor_parent: bool = True,
    admission_override: Admission | None = None,
) -> RuntimeResult:
    if not command:
        raise RuntimePolicyError("runtime command is empty")
    executable = shutil.which(command[0], path=env.get("PATH"))
    if not executable:
        raise RuntimePolicyError(f"required command is unavailable: {command[0]}")
    requested = [command[0], *command[1:]]
    invoked = [executable, *command[1:]]
    parent_pid = os.getppid() if monitor_parent else None
    contract = profile_contract(policy, profile)
    timeout = int(
        contract["timeout_seconds"]
        if timeout_override_seconds is None
        else timeout_override_seconds
    )
    if timeout <= 0 or timeout > int(contract["maximum_timeout_seconds"]):
        raise RuntimePolicyError(
            f"runtime timeout exceeds profile maximum: profile={profile} timeout={timeout}"
        )
    if cleanup_only:
        if admission_override is not None:
            raise RuntimePolicyError("cleanup admission cannot be overridden")
        if os.name != "nt" or profile != "control":
            raise RuntimePolicyError("cleanup admission is restricted to Windows control operations")
        if minimum_override_mib is not None:
            raise RuntimePolicyError("cleanup admission does not accept a memory override")
        snapshot = memory_snapshot()
        reserve_policy = policy.get("runtime_policy", {}).get("host_reserve", {})
        reserve = max(
            int(reserve_policy.get("minimum_mib", 2048)) * MIB,
            int(snapshot.physical_bytes * float(reserve_policy.get("physical_fraction", 0.20))),
        )
        cleanup_limit = int(contract["minimum_memory_mib"]) * MIB
        admission = Admission(
            profile=profile,
            physical_bytes=snapshot.physical_bytes,
            available_bytes=snapshot.available_bytes,
            reserve_bytes=reserve,
            hard_limit_bytes=cleanup_limit,
            minimum_bytes=cleanup_limit,
            maximum_bytes=cleanup_limit,
        )
    elif admission_override is not None:
        if admission_override.profile != profile:
            raise RuntimePolicyError("precomputed admission profile does not match runtime profile")
        admission = admission_override
    else:
        admission = calculate_admission(
            policy,
            profile,
            minimum_override_mib=minimum_override_mib,
        )
    process_limit = int(contract["maximum_descendant_processes"])
    output_limit = int(contract["output_limit_bytes"])
    grace = float(contract["termination_grace_seconds"])
    containment_contexts = policy.get("runtime_policy", {}).get(
        "hard_containment_context_env", {}
    )
    active_hard_contexts = sorted(
        context
        for context, variable in containment_contexts.items()
        if str(env.get(str(variable), "")).strip().casefold()
        in {"1", "true", "yes", "on"}
    )
    hard_containment_required = bool(contract.get("hard_containment_required")) or bool(
        active_hard_contexts
    )
    started_utc = utc_now()
    started = time.monotonic()
    collector = _OutputCollector(output_limit, echo)
    termination_stage = "none"
    reason = "nonzero"
    error_code: str | None = NONZERO_ERROR
    job: _WindowsJob | None = None
    owned_cgroup: _LinuxCgroup | None = None
    supervisor_cgroup = _linux_current_cgroup()
    cgroup = supervisor_cgroup
    containment: dict[str, Any]
    creationflags = 0
    start_new_session = False
    preexec_fn: Callable[[], None] | None = None
    if os.name == "nt":
        creationflags = subprocess.CREATE_NEW_PROCESS_GROUP | 0x00000004  # CREATE_SUSPENDED
        job = _WindowsJob(admission.hard_limit_bytes, process_limit)
        containment = {
            "kind": "windows-job-object",
            "aggregate_memory_limit_bytes": admission.hard_limit_bytes,
            "active_process_limit": process_limit,
            "kill_on_job_close": True,
            "cleanup_admission_override": cleanup_only,
            "parent_pid": parent_pid,
            "parent_loss_kills_tree": monitor_parent,
            "hard_containment_required": hard_containment_required,
            "hard_containment_contexts": active_hard_contexts,
        }
    else:
        owned_cgroup = _LinuxCgroup.create(admission.hard_limit_bytes, process_limit)
        if owned_cgroup is not None:
            cgroup = owned_cgroup.path
            preexec_fn = owned_cgroup.child_setup
            containment = {
                "kind": "dedicated-cgroup-v2",
                "path": str(cgroup),
                "memory_max_bytes": admission.hard_limit_bytes,
                "memory_swap_max_bytes": 0,
                "pids_max": process_limit,
                "memory_oom_group": True,
                "parent_pid": parent_pid,
                "parent_loss_kills_tree": monitor_parent,
                "lease_wrapper": "scripts/management/clearra_process_wrapper.py",
                "hard_containment_required": hard_containment_required,
                "hard_containment_contexts": active_hard_contexts,
            }
        else:
            start_new_session = True
            memory_max = _read_int_or_max(cgroup / "memory.max") if cgroup else None
            memory_swap_max = (
                _read_int_or_max(cgroup / "memory.swap.max") if cgroup else None
            )
            pids_max = _read_int_or_max(cgroup / "pids.max") if cgroup else None
            if hard_containment_required and (
                memory_max is None or pids_max is None
            ):
                raise RuntimePolicyError(
                    "hard process containment is unavailable: inherited cgroup v2 "
                    "must have finite memory.max and pids.max"
                )
            containment = {
                "kind": "inherited-cgroup-v2" if memory_max is not None else "process-group",
                "path": str(cgroup) if cgroup else None,
                "memory_max_bytes": memory_max,
                "memory_swap_max_bytes": memory_swap_max,
                "pids_max": pids_max,
                "requested_memory_limit_bytes": admission.hard_limit_bytes,
                "requested_process_limit": process_limit,
                "parent_pid": parent_pid,
                "parent_loss_kills_tree": monitor_parent,
                "lease_wrapper": "scripts/management/clearra_process_wrapper.py",
                "hard_containment_required": hard_containment_required,
                "hard_containment_contexts": active_hard_contexts,
            }
        wrapper = pathlib.Path(__file__).with_name("clearra_process_wrapper.py")
        if not wrapper.is_file():
            raise RuntimePolicyError(f"POSIX runtime lease wrapper is missing: {wrapper}")
        invoked = [
            sys.executable,
            "-B",
            str(wrapper),
            "--parent-pid",
            str(os.getpid()),
            "--grace-seconds",
            str(max(0.0, grace - 1.0)),
            "--input-size",
            str(len(input_bytes or b"")),
        ]
        if supervisor_cgroup is not None:
            invoked.extend(("--parent-cgroup", str(supervisor_cgroup)))
        if owned_cgroup is not None:
            invoked.extend(("--owned-cgroup", str(owned_cgroup.path)))
        invoked.extend(("--", executable, *command[1:]))
    try:
        runtime_slot = _RuntimeSlot.acquire(policy, profile, admission)
    except BaseException:
        if job is not None:
            job.close()
        if owned_cgroup is not None:
            owned_cgroup.close()
        raise
    containment["concurrency_class"] = runtime_slot.class_id
    containment["concurrency_slot"] = runtime_slot.index
    events_before = _memory_events(cgroup)
    stdin_mode: Any = (
        subprocess.PIPE
        if (os.name != "nt" or input_bytes is not None or keep_stdin_open)
        else subprocess.DEVNULL
    )
    process: subprocess.Popen[bytes] | None = None
    threads: list[threading.Thread] = []
    process_tree_stopped = False
    try:
        process = subprocess.Popen(
            invoked,
            cwd=cwd,
            env=dict(env),
            stdin=stdin_mode,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
            creationflags=creationflags,
            start_new_session=start_new_session,
            preexec_fn=preexec_fn,
        )
        if job is not None:
            try:
                job.assign_and_resume(process)
            except Exception:
                with contextlib.suppress(Exception):
                    process.kill()
                    process.wait(timeout=5)
                raise
        if input_bytes is not None:
            assert process.stdin is not None
            process.stdin.write(input_bytes)
            if os.name == "nt":
                process.stdin.close()
            else:
                process.stdin.flush()
        threads = collector.start(process)
        deadline = started + timeout
        while True:
            if parent_pid is not None and not _pid_alive(parent_pid):
                reason = "parent-lost"
                error_code = PARENT_LOST_ERROR
                if job is not None:
                    job.terminate()
                    termination_stage = "job-kill"
                else:
                    termination_stage = _terminate_posix_group(process, grace)
                break
            if collector.exceeded.is_set():
                reason = "output-limit"
                error_code = OUTPUT_LIMIT_ERROR
                if job is not None:
                    termination_stage = _terminate_windows_job(process, job, grace)
                else:
                    termination_stage = _terminate_posix_group(process, grace)
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                reason = "timeout"
                error_code = TIMEOUT_ERROR
                if job is not None:
                    termination_stage = _terminate_windows_job(process, job, grace)
                else:
                    termination_stage = _terminate_posix_group(process, grace)
                break
            try:
                process.wait(timeout=min(1.0, remaining))
                break
            except subprocess.TimeoutExpired:
                continue
        if process.poll() is None:
            process.wait(timeout=max(5.0, grace))
    except KeyboardInterrupt:
        reason = "user-cancel"
        error_code = CANCELLED_ERROR
        if process is not None and process.poll() is None:
            if job is not None:
                termination_stage = _terminate_windows_job(process, job, grace, 130)
            else:
                termination_stage = _terminate_posix_group(process, grace)
        if process is not None:
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=max(5.0, grace))
    finally:
        try:
            if (
                process is not None
                and (os.name != "nt" or keep_stdin_open)
                and process.stdin is not None
            ):
                with contextlib.suppress(Exception):
                    process.stdin.close()
            peak = None
            descendants = None
            job_oom = False
            if job is not None:
                peak, descendants, active_descendants, job_oom = job.metrics()
                if active_descendants:
                    job.terminate()
                    process_tree_stopped = job.wait_until_empty(max(5.0, grace))
                    if termination_stage == "none":
                        termination_stage = "job-close-descendants"
                else:
                    process_tree_stopped = True
                job.close()
            owned_events_after: dict[str, int] | None = None
            if owned_cgroup is not None:
                peak, descendants, owned_events_after = owned_cgroup.metrics()
                process_tree_stopped = owned_cgroup.close()
            elif (
                os.name != "nt"
                and process is not None
                and _posix_group_alive(process.pid)
            ):
                termination_stage = _terminate_posix_group(process, grace)
                process_tree_stopped = not _posix_group_alive(process.pid)
            elif os.name != "nt":
                process_tree_stopped = True
            stdout, stderr = collector.finish(threads)
        finally:
            runtime_slot.release()
    returncode = process.returncode if process is not None and process.returncode is not None else 125
    events_after = owned_events_after if owned_events_after is not None else _memory_events(cgroup)
    oom_before = events_before.get("oom_kill")
    oom_after = events_after.get("oom_kill")
    cgroup_oom = (
        oom_before is not None and oom_after is not None and oom_after > oom_before
        and returncode in {-SIGKILL_NUMBER, 128 + SIGKILL_NUMBER}
    )
    if job_oom or cgroup_oom:
        reason = "oom"
        error_code = MEMORY_LIMIT_ERROR
    elif collector.exceeded.is_set():
        reason = "output-limit"
        error_code = OUTPUT_LIMIT_ERROR
    elif reason not in {"timeout", "output-limit", "user-cancel", "parent-lost"}:
        if returncode == 0:
            reason = "normal"
            error_code = None
        elif returncode in {-SIGTERM_NUMBER, 128 + SIGTERM_NUMBER}:
            reason = "term"
            error_code = NONZERO_ERROR
        elif returncode in {-SIGKILL_NUMBER, 128 + SIGKILL_NUMBER}:
            reason = "kill"
            error_code = NONZERO_ERROR
        else:
            reason = "nonzero"
            error_code = NONZERO_ERROR
    if not process_tree_stopped and returncode == 0:
        returncode = 125
        reason = "tree-not-stopped"
        error_code = TREE_CLEANUP_ERROR
    return RuntimeResult(
        command=redact_argv(requested),
        command_sha256=command_digest(requested),
        returncode=returncode,
        reason=reason,
        error_code=error_code,
        started_utc=started_utc,
        ended_utc=utc_now(),
        duration_ms=round((time.monotonic() - started) * 1000),
        timeout_seconds=timeout,
        termination_stage=termination_stage,
        stdout=stdout,
        stderr=stderr,
        output_bytes=collector.total,
        output_limit_bytes=output_limit,
        admission=dataclasses.asdict(admission),
        containment=containment,
        peak_memory_bytes=peak,
        descendant_processes=descendants,
        oom_counter_before=oom_before,
        oom_counter_after=oom_after,
        process_tree_stopped=process_tree_stopped,
    )


def windows_to_wsl_mount(path: pathlib.Path) -> str:
    resolved = path.resolve(strict=False)
    drive, tail = os.path.splitdrive(str(resolved))
    if not drive or drive.startswith("\\\\"):
        raise RuntimePolicyError(f"WSL input must be on a local drive: {resolved}")
    letter = drive.rstrip(":").lower()
    components = tail.replace("\\", "/").lstrip("/")
    return f"/mnt/{letter}/{components}"


def _decode_wsl_output(value: bytes) -> str:
    if b"\x00" in value:
        with contextlib.suppress(UnicodeDecodeError):
            return value.decode("utf-16-le").replace("\ufeff", "")
    return value.decode("utf-8", errors="replace")


def _is_link_or_reparse(path: pathlib.Path) -> bool:
    try:
        information = os.lstat(path)
    except OSError:
        return False
    attributes = getattr(information, "st_file_attributes", 0)
    return path.is_symlink() or bool(attributes & 0x400)


def _reject_source_link(root: pathlib.Path, path: pathlib.Path) -> None:
    cursor = root
    for part in path.relative_to(root).parts:
        cursor /= part
        if _is_link_or_reparse(cursor):
            raise RuntimePolicyError(
                f"WSL source projection refuses a link or junction: {cursor}"
            )


def _reject_absolute_link_path(path: pathlib.Path) -> None:
    absolute = pathlib.Path(os.path.abspath(os.fspath(path)))
    cursor = pathlib.Path(absolute.anchor)
    for part in absolute.parts[1:]:
        cursor /= part
        if _is_link_or_reparse(cursor):
            raise RuntimePolicyError(
                f"WSL host input refuses a link or junction: {cursor}"
            )


def _source_archive(
    root: pathlib.Path,
    temporary_root: pathlib.Path,
    secret_predicate: Callable[[pathlib.Path], bool],
    policy: Mapping[str, Any],
) -> tuple[pathlib.Path, str, int]:
    tracked = run_host_process(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=root,
        env=os.environ.copy(),
        policy=policy,
        profile="control",
        echo=False,
    )
    if tracked.returncode != 0:
        raise RuntimePolicyError("could not enumerate tracked WSL source inputs")
    names = [entry for entry in tracked.stdout.split("\0") if entry]
    files: list[tuple[str, pathlib.Path]] = []
    for name in sorted(names):
        relative = pathlib.PurePosixPath(name)
        if relative.name == "package-lock.json" or any(
            part
            in {
                "node_modules",
                "dist",
                "dist-server",
                "build",
                "coverage",
                "models",
                "checkpoints",
                ".cache",
            }
            for part in relative.parts
        ):
            continue
        path = root / pathlib.Path(*relative.parts)
        if secret_predicate(path):
            raise RuntimePolicyError(
                "prohibited credential path blocked during WSL source projection; contents were not inspected"
            )
        if not path.is_file():
            continue
        _reject_source_link(root, path)
        files.append((name, path))
    temporary_root.mkdir(parents=True, exist_ok=True)
    partial_archive = temporary_root / "source.partial.tar.gz"
    digest = hashlib.sha256()
    try:
        with tarfile.open(partial_archive, "w:gz", format=tarfile.PAX_FORMAT) as bundle:
            for name, path in files:
                _reject_source_link(root, path)
                before = path.stat()
                information = bundle.gettarinfo(str(path), arcname=name)
                if not information.isfile():
                    raise RuntimePolicyError(
                        f"WSL source projection accepts only regular files: {path}"
                    )
                information.uid = 0
                information.gid = 0
                information.uname = ""
                information.gname = ""
                file_digest = hashlib.sha256()
                with path.open("rb") as source:
                    bundle.addfile(information, _DigestingReader(source, file_digest))
                after = path.stat()
                identity_before = (
                    before.st_dev,
                    before.st_ino,
                    before.st_size,
                    before.st_mtime_ns,
                )
                identity_after = (
                    after.st_dev,
                    after.st_ino,
                    after.st_size,
                    after.st_mtime_ns,
                )
                if identity_before != identity_after:
                    raise RuntimePolicyError(
                        f"WSL source changed while its bounded archive was created: {path}"
                    )
                encoded = name.encode("utf-8")
                digest.update(len(encoded).to_bytes(4, "big"))
                digest.update(encoded)
                digest.update(file_digest.digest())
        source_digest = digest.hexdigest()
        archive = temporary_root / f"source-{source_digest[:16]}.tar.gz"
        os.replace(partial_archive, archive)
    except Exception:
        partial_archive.unlink(missing_ok=True)
        with contextlib.suppress(OSError):
            temporary_root.rmdir()
        raise
    return archive, source_digest, len(files)


class _DigestingReader:
    def __init__(self, source: BinaryIO, digest: Any) -> None:
        self.source = source
        self.digest = digest

    def read(self, size: int = -1) -> bytes:
        chunk = self.source.read(size)
        self.digest.update(chunk)
        return chunk


def _append_wsl_bootstrap_metadata(
    export_path: pathlib.Path,
    *,
    transaction: str,
    source_distribution: str,
) -> None:
    """Append first-boot controls without reading any exported file content."""
    marker = json.dumps(
        {
            "schema_id": "clearra.wsl-provision-owner.v1",
            "source_distribution": source_distribution,
            "created_transaction": transaction,
            "created_utc": utc_now(),
            "status": "imported",
        },
        ensure_ascii=False,
        indent=2,
    ).encode("utf-8") + b"\n"
    first_boot = (
        b"[boot]\n"
        b"systemd=false\n\n"
        b"[interop]\n"
        b"appendWindowsPath=false\n\n"
        b"[user]\n"
        b"default=root\n"
    )
    with tarfile.open(export_path, "a", format=tarfile.PAX_FORMAT) as archive:
        for name, payload, mode in (
            ("etc/wsl.conf", first_boot, 0o644),
            ("etc/clearra/runtime.json", marker, 0o644),
        ):
            information = tarfile.TarInfo(name)
            information.size = len(payload)
            information.mode = mode
            information.uid = 0
            information.gid = 0
            information.uname = ""
            information.gname = ""
            information.mtime = 0
            archive.addfile(information, io.BytesIO(payload))


def _pid_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    if os.name == "nt":
        SYNCHRONIZE = 0x00100000
        handle = ctypes.windll.kernel32.OpenProcess(SYNCHRONIZE, False, pid)
        if not handle:
            return False
        try:
            return ctypes.windll.kernel32.WaitForSingleObject(handle, 0) == 0x102
        finally:
            ctypes.windll.kernel32.CloseHandle(handle)
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


class WslSupervisor:
    def __init__(
        self,
        *,
        root: pathlib.Path,
        policy: Mapping[str, Any],
        state_root: pathlib.Path,
        temporary_root: pathlib.Path,
        secret_predicate: Callable[[pathlib.Path], bool],
        output_validator: Callable[[pathlib.Path], pathlib.Path],
    ) -> None:
        if os.name != "nt":
            raise RuntimePolicyError("dedicated WSL management is available only on Windows")
        self.root = root
        self.policy = policy
        self.state_root = state_root
        self.temporary_root = temporary_root
        self.secret_predicate = secret_predicate
        self.output_validator = output_validator
        self.wsl = shutil.which("wsl.exe")
        if not self.wsl:
            raise RuntimePolicyError("WSL is unavailable")
        self.contract = policy["runtime_policy"]["wsl"]
        self.distribution = str(self.contract["distribution"])

    def _control(
        self,
        arguments: Sequence[str],
        *,
        timeout: int = 300,
        echo: bool = False,
        keep_stdin_open: bool = False,
        cleanup_only: bool = False,
        profile: str | None = None,
        admission_override: Admission | None = None,
    ) -> RuntimeResult:
        environment = os.environ.copy()
        control_max = int(profile_contract(self.policy, "control")["maximum_timeout_seconds"])
        if cleanup_only and timeout > 60:
            raise RuntimePolicyError("WSL cleanup control timeout may not exceed 60 seconds")
        selected_profile = profile or (
            "control" if cleanup_only or timeout <= control_max else "build-test"
        )
        return run_host_process(
            [self.wsl, *arguments],
            cwd=self.root,
            env=environment,
            policy=self.policy,
            profile=selected_profile,
            echo=echo,
            keep_stdin_open=keep_stdin_open,
            timeout_override_seconds=timeout,
            cleanup_only=cleanup_only,
            monitor_parent=not cleanup_only,
            admission_override=admission_override,
        )

    def _list(self, running: bool = False, *, cleanup_only: bool = False) -> list[str]:
        arguments = ["--list", "--quiet"]
        if running:
            arguments.append("--running")
        result = self._control(
            arguments,
            timeout=60 if cleanup_only else 300,
            cleanup_only=cleanup_only,
        )
        if result.returncode != 0:
            raise RuntimePolicyError(
                runtime_failure_summary("could not list WSL distributions", result)
            )
        value = _decode_wsl_output(result.stdout.encode("utf-8", errors="replace"))
        return [line.strip().replace("\x00", "") for line in value.splitlines() if line.strip().replace("\x00", "")]

    def _lease_path(self) -> pathlib.Path:
        return self.state_root / "runtime" / "wsl-session-owner.json"

    def _remove_owned_source_temp(self, run_id: str) -> bool:
        if not re.fullmatch(r"[A-Za-z0-9._-]+", run_id):
            # An unreadable or malformed stale lease may not authorize any
            # filesystem removal.  Recovery can still terminate the dedicated
            # distribution and replace the atomic lease file.
            return False
        root = (self.temporary_root / "wsl-source").resolve(strict=False)
        target = (root / run_id).resolve(strict=False)
        try:
            target.relative_to(root)
        except ValueError as error:
            raise RuntimePolicyError("stale WSL source path escaped its managed root") from error
        if _is_link_or_reparse(target):
            raise RuntimePolicyError("stale WSL source path is a link or junction")
        if target.is_dir():
            shutil.rmtree(target)
        return not target.exists()

    @contextlib.contextmanager
    def lease(self, run_id: str):
        path = self._lease_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        recovered = False
        stale_source_removed = False
        owner: dict[str, Any] | None = None
        try:
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            try:
                owner = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                owner = {"pid": 0, "status": "unreadable"}
            if _pid_alive(int(owner.get("pid") or 0)):
                raise RuntimePolicyError(
                    "Clearra-Build already has a live session owner: "
                    f"pid={owner.get('pid')} run_id={owner.get('run_id')}"
                )
            self.terminate()
            stale_source_removed = self._remove_owned_source_temp(
                str(owner.get("run_id") or "")
            )
            path.unlink(missing_ok=True)
            recovered = True
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        material = {
            "schema_id": "clearra.wsl-session-owner.v1",
            "pid": os.getpid(),
            "run_id": run_id,
            "created_utc": utc_now(),
        }
        with os.fdopen(descriptor, "w", encoding="utf-8") as destination:
            json.dump(material, destination, ensure_ascii=False, indent=2)
            destination.write("\n")
        try:
            if owner is None and self.distribution in self._list(
                running=True, cleanup_only=True
            ):
                owner = {"status": "orphaned-dedicated-distribution", "pid": 0}
                self.terminate()
                recovered = True
            yield {
                "recovered_stale_owner": recovered,
                "previous_owner": owner,
                "stale_source_removed": stale_source_removed,
            }
        finally:
            try:
                current = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                current = {}
            if current.get("run_id") == run_id:
                path.unlink(missing_ok=True)

    def terminate(self) -> dict[str, Any]:
        before = self._list(running=True, cleanup_only=True)
        attempted = self.distribution in before
        result = (
            self._control(
                ["--terminate", self.distribution], timeout=60, cleanup_only=True
            )
            if attempted
            else None
        )
        deadline = time.monotonic() + 15
        after = before
        while time.monotonic() < deadline:
            after = self._list(running=True, cleanup_only=True)
            if self.distribution not in after:
                break
            time.sleep(0.5)
        if self.distribution in after:
            raise RuntimePolicyError(f"dedicated WSL distribution did not stop: {self.distribution}")
        return {
            "running_before": before,
            "terminate_attempted": attempted,
            "terminate_returncode": result.returncode if result else None,
            "running_after": after,
            "dedicated_distribution_stopped": True,
        }

    def marker_digest(self) -> str:
        versions = self.policy["toolchains"]
        sources = self.policy["toolchain_sources"]
        material = json.dumps(
            {"toolchains": versions, "sources": sources},
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        return hashlib.sha256(material.encode("utf-8")).hexdigest()

    def _distro_storage_snapshot(self, distribution: str) -> dict[str, Any]:
        """Read registration and VHD metadata without opening filesystem contents."""
        import winreg

        registry = r"Software\Microsoft\Windows\CurrentVersion\Lxss"
        base_path: str | None = None
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, registry) as root_key:
            index = 0
            while True:
                try:
                    child_name = winreg.EnumKey(root_key, index)
                except OSError:
                    break
                index += 1
                with winreg.OpenKey(root_key, child_name) as child_key:
                    try:
                        candidate = str(winreg.QueryValueEx(child_key, "DistributionName")[0])
                    except OSError:
                        continue
                    if candidate.casefold() != distribution.casefold():
                        continue
                    base_path = str(winreg.QueryValueEx(child_key, "BasePath")[0])
                    break
        if not base_path:
            raise RuntimePolicyError(
                f"could not locate WSL registration metadata: {distribution}"
            )
        storage = pathlib.Path(os.path.expandvars(base_path)) / "ext4.vhdx"
        try:
            information = storage.stat()
        except OSError as error:
            raise RuntimePolicyError(
                f"could not read WSL filesystem metadata without opening its contents: {distribution}"
            ) from error
        return {
            "distribution": distribution,
            "base_path": str(pathlib.Path(base_path)),
            "storage_path": str(storage),
            "file_id": f"{information.st_dev}:{information.st_ino}",
            "bytes": information.st_size,
            "mtime_ns": information.st_mtime_ns,
        }

    def _provision_owner_root(self) -> pathlib.Path:
        return self.state_root / "runtime" / "wsl-provisions"

    def _expected_install_root(self) -> pathlib.Path:
        return self.output_validator(
            pathlib.Path(
                os.path.expandvars(str(self.contract["install_root_windows"]))
            )
        )

    def _assert_registered_install_root(
        self, distribution: str, expected_install: pathlib.Path
    ) -> dict[str, Any]:
        storage = self._distro_storage_snapshot(distribution)
        registered = pathlib.Path(str(storage["base_path"])).resolve(strict=False)
        if registered != expected_install.resolve(strict=False):
            raise RuntimePolicyError(
                f"registered {distribution} storage differs from the managed install root"
            )
        return storage

    def _write_provision_owner(
        self,
        path: pathlib.Path,
        *,
        transaction: str,
        install_root: pathlib.Path,
        export_path: pathlib.Path,
        status: str,
    ) -> None:
        material = {
            "schema_id": "clearra.wsl-provision-owner.v1",
            "distribution": self.distribution,
            "transaction": transaction,
            "owner_pid": os.getpid(),
            "install_root": str(install_root),
            "export_path": str(export_path),
            "status": status,
            "updated_utc": utc_now(),
        }
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_suffix(".json.partial")
        temporary.write_text(
            json.dumps(material, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        os.replace(temporary, path)

    def _owned_provision_path(self, value: str) -> pathlib.Path:
        path = pathlib.Path(value).resolve(strict=False)
        allowed = (self.temporary_root / "wsl-provision").resolve(strict=False)
        try:
            path.relative_to(allowed)
        except ValueError as error:
            raise RuntimePolicyError(
                "WSL provision ownership receipt contains an unmanaged temporary path"
            ) from error
        return path

    def _unregister_owned_import(
        self, transaction: str, install_root: pathlib.Path
    ) -> bool:
        expected_install = self._expected_install_root().resolve(strict=False)
        resolved_install = self.output_validator(install_root)
        if resolved_install != expected_install:
            raise RuntimePolicyError("owned WSL install path does not match policy")
        if self.distribution not in self._list(cleanup_only=True):
            if resolved_install.exists():
                if _is_link_or_reparse(resolved_install):
                    raise RuntimePolicyError("owned WSL install path became a link or junction")
                shutil.rmtree(resolved_install)
            return True
        self._assert_registered_install_root(self.distribution, expected_install)
        script = windows_to_wsl_mount(
            self.root / "scripts" / "runtime" / "clearra-wsl-provision.sh"
        )
        ownership = self._control(
            [
                "-d",
                self.distribution,
                "--user",
                "root",
                "--exec",
                "/bin/bash",
                script,
                "--verify-owner",
                transaction,
            ],
            timeout=60,
            cleanup_only=True,
        )
        if ownership.returncode != 0:
            return False
        self.terminate()
        removed = self._control(
            ["--unregister", self.distribution], timeout=60, cleanup_only=True
        )
        if removed.returncode != 0:
            raise RuntimePolicyError(
                runtime_failure_summary(
                    "owned partial WSL import could not be unregistered", removed
                )
            )
        if self.distribution in self._list(cleanup_only=True):
            raise RuntimePolicyError("owned partial WSL import remained after unregister")
        if resolved_install.exists():
            if _is_link_or_reparse(resolved_install):
                raise RuntimePolicyError("owned WSL install path became a link or junction")
            shutil.rmtree(resolved_install)
        return True

    def _recover_abandoned_provisions(self) -> list[dict[str, Any]]:
        recovered: list[dict[str, Any]] = []
        root = self._provision_owner_root()
        if not root.is_dir():
            return recovered
        for receipt in sorted(root.glob("*.json")):
            try:
                value = json.loads(receipt.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raise RuntimePolicyError(
                    f"unreadable WSL provision ownership receipt: {receipt}"
                ) from error
            if (
                value.get("schema_id") != "clearra.wsl-provision-owner.v1"
                or value.get("distribution") != self.distribution
            ):
                raise RuntimePolicyError(
                    f"invalid WSL provision ownership receipt: {receipt}"
                )
            owner_pid = int(value.get("owner_pid") or 0)
            if owner_pid != os.getpid() and _pid_alive(owner_pid):
                raise RuntimePolicyError(
                    "Clearra-Build provisioning already has a live owner: "
                    f"pid={owner_pid} transaction={value.get('transaction')}"
                )
            export_path = self._owned_provision_path(str(value.get("export_path", "")))
            with contextlib.suppress(OSError):
                export_path.unlink(missing_ok=True)
            with contextlib.suppress(OSError):
                export_path.parent.rmdir()
            status = str(value.get("status", "unknown"))
            unregistered = False
            if status != "complete":
                install_root = pathlib.Path(str(value.get("install_root", "")))
                unregistered = self._unregister_owned_import(
                    str(value.get("transaction", "")), install_root
                )
                if not unregistered:
                    raise RuntimePolicyError(
                        "a partial Clearra-Build import lacks the matching ownership marker; "
                        "it was preserved"
                    )
            receipt.unlink(missing_ok=True)
            recovered.append(
                {
                    "transaction": value.get("transaction"),
                    "prior_status": status,
                    "temporary_export_removed": not export_path.exists(),
                    "owned_partial_import_unregistered": unregistered,
                }
            )
        return recovered

    def provision(self, source_distribution: str) -> dict[str, Any]:
        if source_distribution != str(self.contract["source_distribution"]):
            raise RuntimePolicyError(
                "WSL source distribution differs from the committed management policy"
            )
        if source_distribution == self.distribution:
            raise RuntimePolicyError("source and dedicated WSL distributions must differ")
        run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ") + "-" + uuid.uuid4().hex[:12]
        provision_root = self.output_validator(
            self.temporary_root / "wsl-provision" / run_id
        )
        export_path = self.output_validator(provision_root / "ubuntu-export.tar")
        install_root = self._expected_install_root()
        owner_receipt = self._provision_owner_root() / f"{run_id}.json"
        imported = False
        configured = False
        existing = False
        recovery: list[dict[str, Any]] = []
        source_before: dict[str, Any] | None = None
        source_after: dict[str, Any] | None = None
        export_runtime: RuntimeResult | None = None
        import_runtime: RuntimeResult | None = None
        sanitization_runtime: RuntimeResult | None = None
        sanitization_termination: dict[str, Any] | None = None
        configuration_runtime: RuntimeResult | None = None
        configuration_termination: dict[str, Any] | None = None
        provision_error: RuntimePolicyError | None = None
        with self.lease(run_id) as provision_lease:
            recovery = self._recover_abandoned_provisions()
            installed = self._list()
            if self.distribution in installed:
                self._assert_registered_install_root(self.distribution, install_root)
                existing = True
            else:
                if source_distribution not in installed:
                    raise RuntimePolicyError(
                        f"source WSL distribution is unavailable: {source_distribution}"
                    )
                running = self._list(running=True)
                if source_distribution in running:
                    raise RuntimePolicyError(
                        f"source WSL distribution must already be stopped: {source_distribution}"
                    )
                if install_root.exists():
                    raise RuntimePolicyError(
                        "dedicated WSL install root already exists without a registered distribution"
                    )
                source_before = self._distro_storage_snapshot(source_distribution)
                install_root.parent.mkdir(parents=True, exist_ok=True)
                self._write_provision_owner(
                    owner_receipt,
                    transaction=run_id,
                    install_root=install_root,
                    export_path=export_path,
                    status="created",
                )
                try:
                    # The ownership receipt exists before the temporary
                    # directory, so a host crash cannot leave an unreceipted
                    # export path.
                    provision_root.mkdir(parents=True, exist_ok=False)
                    exported = self._control(
                        ["--export", source_distribution, str(export_path)],
                        timeout=3600,
                        echo=True,
                    )
                    export_runtime = exported
                    if exported.returncode != 0 or not export_path.is_file():
                        raise RuntimePolicyError(
                            runtime_failure_summary("WSL export failed", exported)
                        )
                    if source_distribution in self._list(running=True):
                        raise RuntimePolicyError(
                            "source WSL distribution started during export; provisioning stopped"
                        )
                    source_after = self._distro_storage_snapshot(source_distribution)
                    stable_fields = ("base_path", "storage_path", "file_id", "bytes", "mtime_ns")
                    if any(source_before[key] != source_after[key] for key in stable_fields):
                        raise RuntimePolicyError(
                            "source WSL filesystem metadata changed during export; provisioning stopped"
                        )
                    _append_wsl_bootstrap_metadata(
                        export_path,
                        transaction=run_id,
                        source_distribution=source_distribution,
                    )
                    self._write_provision_owner(
                        owner_receipt,
                        transaction=run_id,
                        install_root=install_root,
                        export_path=export_path,
                        status="exported",
                    )
                    imported_result = self._control(
                        [
                            "--import",
                            self.distribution,
                            str(install_root),
                            str(export_path),
                            "--version",
                            "2",
                        ],
                        timeout=3600,
                        echo=True,
                    )
                    import_runtime = imported_result
                    if imported_result.returncode != 0:
                        raise RuntimePolicyError(
                            runtime_failure_summary("WSL import failed", imported_result)
                        )
                    imported = True
                    self._write_provision_owner(
                        owner_receipt,
                        transaction=run_id,
                        install_root=install_root,
                        export_path=export_path,
                        status="imported",
                    )
                    sanitize_script = windows_to_wsl_mount(
                        self.root / "scripts" / "runtime" / "clearra-wsl-sanitize.sh"
                    )
                    provision_contract = profile_contract(self.policy, "build-test")
                    sanitize_admission = calculate_admission(self.policy, "build-test")
                    sanitized_result = self._control(
                        [
                            "-d",
                            self.distribution,
                            "--user",
                            "root",
                            "--exec",
                            "/bin/bash",
                            sanitize_script,
                            "--transaction",
                            run_id,
                            "--source-distro",
                            source_distribution,
                            "--memory-max",
                            str(sanitize_admission.hard_limit_bytes),
                            "--tasks-max",
                            str(provision_contract["maximum_descendant_processes"]),
                        ],
                        timeout=900,
                        echo=True,
                        keep_stdin_open=True,
                        profile="build-test",
                        admission_override=sanitize_admission,
                    )
                    sanitization_runtime = sanitized_result
                    if sanitized_result.error_code == MEMORY_LIMIT_ERROR:
                        raise RuntimePolicyError(
                            f"{MEMORY_LIMIT_ERROR}: WSL sanitization exceeded its cgroup limit"
                        )
                    if sanitized_result.returncode != 0:
                        raise RuntimePolicyError(
                            runtime_failure_summary(
                                "WSL sanitization failed", sanitized_result
                            )
                        )
                    sanitization_termination = self.terminate()
                    self._write_provision_owner(
                        owner_receipt,
                        transaction=run_id,
                        install_root=install_root,
                        export_path=export_path,
                        status="sanitized",
                    )
                    script = windows_to_wsl_mount(
                        self.root / "scripts" / "runtime" / "clearra-wsl-provision.sh"
                    )
                    versions = self.policy["toolchains"]
                    node_digest = self.policy["toolchain_sources"]["node"][
                        "distribution_sha256"
                    ]["linux-x64.tar.xz"]
                    session_script = windows_to_wsl_mount(
                        self.root / "scripts" / "runtime" / "clearra-wsl-session.sh"
                    )
                    provision_admission = calculate_admission(self.policy, "build-test")
                    cleanup_budget = 60
                    configured_result = self._control(
                        [
                            "-d",
                            self.distribution,
                            "--user",
                            "root",
                            "--exec",
                            "/bin/bash",
                            session_script,
                            "--unit",
                            "clearra-provision-" + re.sub(r"[^A-Za-z0-9_.-]", "-", run_id),
                            "--memory-max",
                            str(provision_admission.hard_limit_bytes),
                            "--tasks-max",
                            str(provision_contract["maximum_descendant_processes"]),
                            "--runtime-max",
                            str(int(provision_contract["maximum_timeout_seconds"]) - cleanup_budget),
                            "--grace",
                            str(provision_contract["termination_grace_seconds"]),
                            "--guest",
                            script,
                            "--mode",
                            "provision",
                            "--run-as",
                            "root",
                            "--",
                            "--transaction",
                            run_id,
                            "--source-distro",
                            source_distribution,
                            "--toolchain-digest",
                            self.marker_digest(),
                            "--node-version",
                            str(versions["node"]),
                            "--node-sha256",
                            str(node_digest),
                            "--npm-version",
                            str(versions["npm"]),
                            "--pnpm-version",
                            str(versions["pnpm"]),
                            "--rust-version",
                            str(versions["rust"]),
                            "--cargo-version",
                            str(versions["cargo"]),
                            "--wasm-bindgen-version",
                            str(versions["wasm_bindgen"]),
                        ],
                        timeout=5400,
                        echo=True,
                        keep_stdin_open=True,
                        profile="build-test",
                        admission_override=provision_admission,
                    )
                    configuration_runtime = configured_result
                    if configured_result.error_code == MEMORY_LIMIT_ERROR or "CLEARRA_WSL_RESULT=oom-kill" in (
                        configured_result.stdout + "\n" + configured_result.stderr
                    ):
                        raise RuntimePolicyError(
                            f"{MEMORY_LIMIT_ERROR}: WSL provisioning exceeded its cgroup limit"
                        )
                    if configured_result.returncode != 0:
                        raise RuntimePolicyError(
                            runtime_failure_summary(
                                "WSL provisioning failed", configured_result
                            )
                        )
                    configured = True
                    self._write_provision_owner(
                        owner_receipt,
                        transaction=run_id,
                        install_root=install_root,
                        export_path=export_path,
                        status="configured",
                    )
                except Exception as caught:
                    details = {
                        "transaction": run_id,
                        "source_distribution": source_distribution,
                        "distribution": self.distribution,
                        "imported": imported,
                        "configured": configured,
                        "source_immutability": {
                            "before": source_before,
                            "after": source_after,
                            "unchanged": (
                                source_before == source_after
                                if source_before is not None and source_after is not None
                                else None
                            ),
                        },
                        "steps": {
                            "export": export_runtime.receipt() if export_runtime else None,
                            "import": import_runtime.receipt() if import_runtime else None,
                            "sanitize": (
                                sanitization_runtime.receipt()
                                if sanitization_runtime
                                else None
                            ),
                            "sanitize_termination": sanitization_termination,
                            "configure": (
                                configuration_runtime.receipt()
                                if configuration_runtime
                                else None
                            ),
                        },
                    }
                    if isinstance(caught, RuntimePolicyError):
                        caught.details.update(details)
                        provision_error = caught
                        raise
                    provision_error = RuntimePolicyError(
                        f"WSL provisioning failed: {caught}", details=details
                    )
                    raise provision_error from caught
                finally:
                    with contextlib.suppress(OSError):
                        export_path.unlink(missing_ok=True)
                    with contextlib.suppress(OSError):
                        provision_root.rmdir()
                    try:
                        configuration_termination = self.terminate()
                    except RuntimePolicyError as cleanup_error:
                        if provision_error is None:
                            raise
                        provision_error.details["cleanup_error"] = str(cleanup_error)
                    if provision_error is not None:
                        provision_error.details["configuration_termination"] = (
                            configuration_termination
                        )
                    try:
                        if not configured and owner_receipt.exists():
                            removed = self._unregister_owned_import(run_id, install_root)
                            if removed:
                                owner_receipt.unlink(missing_ok=True)
                    except RuntimePolicyError as cleanup_error:
                        if provision_error is None:
                            raise
                        provision_error.details["partial_import_cleanup_error"] = str(
                            cleanup_error
                        )
        verification = self.verify()
        if existing:
            return {
                "status": "existing-verified",
                "recovered_abandoned_provisions": recovery,
                "verification": verification,
            }
        self._write_provision_owner(
            owner_receipt,
            transaction=run_id,
            install_root=install_root,
            export_path=export_path,
            status="complete",
        )
        owner_receipt.unlink(missing_ok=True)
        return {
            "status": "provisioned",
            "source_distribution": source_distribution,
            "distribution": self.distribution,
            "transaction": run_id,
            "install_root": str(install_root),
            "temporary_export_removed": not export_path.exists(),
            "source_immutability": {
                "before": source_before,
                "after": source_after,
                "unchanged": source_before == source_after,
                "stopped_after": source_distribution not in self._list(running=True),
            },
            "steps": {
                "export": export_runtime.receipt() if export_runtime else None,
                "import": import_runtime.receipt() if import_runtime else None,
                "sanitize": (
                    sanitization_runtime.receipt() if sanitization_runtime else None
                ),
                "sanitize_termination": sanitization_termination,
                "configure": (
                    configuration_runtime.receipt() if configuration_runtime else None
                ),
                "configure_termination": configuration_termination,
            },
            "recovered_abandoned_provisions": recovery,
            "provision_lease": provision_lease,
            "verification": verification,
        }

    def verify(self) -> dict[str, Any]:
        if self.distribution not in self._list():
            raise RuntimePolicyError(f"dedicated WSL distribution is unavailable: {self.distribution}")
        result = self.run_entry("verify", [], profile="control")
        if result["runtime"]["returncode"] != 0:
            raise RuntimePolicyError("dedicated WSL verification failed")
        return result

    def run_entry(
        self,
        entry: str,
        arguments: Sequence[str],
        *,
        profile: str | None = None,
        minimum_override_mib: int | None = None,
    ) -> dict[str, Any]:
        entries = self.contract["entrypoints"]
        if entry not in entries:
            raise RuntimePolicyError(f"unregistered WSL guest entrypoint: {entry}")
        entry_contract = entries[entry]
        selected_profile = profile or str(entry_contract["profile"])
        if selected_profile != entry_contract["profile"]:
            raise RuntimePolicyError(
                f"WSL entrypoint profile mismatch: {entry} requires {entry_contract['profile']}"
            )
        if any("\x00" in value or "\r" in value or "\n" in value for value in arguments):
            raise RuntimePolicyError("WSL entrypoint arguments may not contain control characters")
        normalized_arguments = list(arguments)
        output_path_options = {
            "wasm-build": ("--staging",),
            "pc-runtime-build-batch": ("--host-output",),
            "oracle-local-layers-v080": ("--output",),
        }.get(entry, ())
        for path_option in output_path_options:
            option_indexes = [
                index
                for index, value in enumerate(normalized_arguments)
                if value == path_option
            ]
            if len(option_indexes) != 1:
                raise RuntimePolicyError(f"{entry} requires {path_option}")
            index = option_indexes[0] + 1
            if index >= len(normalized_arguments):
                raise RuntimePolicyError(f"{path_option} requires a Windows path")
            path = pathlib.Path(normalized_arguments[index])
            if self.secret_predicate(path):
                raise RuntimePolicyError(
                    "prohibited credential path blocked; contents were not inspected"
                )
            resolved_path = self.output_validator(path)
            normalized_arguments[index] = windows_to_wsl_mount(resolved_path)
        input_path_options = {
            "oracle-local-layers-v080": ("--accepted-ctk3",),
        }.get(entry, ())
        for path_option in input_path_options:
            option_indexes = [
                index
                for index, value in enumerate(normalized_arguments)
                if value == path_option
            ]
            if len(option_indexes) != 1:
                raise RuntimePolicyError(f"{entry} requires {path_option}")
            index = option_indexes[0] + 1
            if index >= len(normalized_arguments):
                raise RuntimePolicyError(f"{path_option} requires a Windows path")
            path = pathlib.Path(normalized_arguments[index])
            if self.secret_predicate(path):
                raise RuntimePolicyError(
                    "prohibited credential path blocked; contents were not inspected"
                )
            _reject_absolute_link_path(path)
            if not path.is_dir():
                raise RuntimePolicyError(
                    f"WSL input must be an existing regular directory: {path}"
                )
            normalized_arguments[index] = windows_to_wsl_mount(path.resolve(strict=True))
        if entry == "oracle-local-layers-v080":
            if "--repository-root" in normalized_arguments:
                raise RuntimePolicyError(
                    "Oracle WSL repository authority is supplied only by the supervisor"
                )
            normalized_arguments = [
                "--repository-root",
                windows_to_wsl_mount(self.root),
                *normalized_arguments,
            ]
        if entry == "posix-syntax-audit":
            indexes = [
                index + 1
                for index, value in enumerate(normalized_arguments)
                if value == "--host-path"
            ]
            if not indexes or any(index >= len(normalized_arguments) for index in indexes):
                raise RuntimePolicyError("POSIX syntax audit requires --host-path")
            for index in indexes:
                path = pathlib.Path(normalized_arguments[index])
                if self.secret_predicate(path):
                    raise RuntimePolicyError(
                        "prohibited credential path blocked; contents were not inspected"
                    )
                resolved_path = path.resolve(strict=True)
                try:
                    resolved_path.relative_to(self.root.resolve())
                except ValueError as error:
                    raise RuntimePolicyError(
                        "POSIX syntax audit accepts only a repository source path"
                    ) from error
                normalized_arguments[index] = windows_to_wsl_mount(resolved_path)
        run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ") + "-" + uuid.uuid4().hex[:12]
        admission = calculate_admission(
            self.policy,
            selected_profile,
            minimum_override_mib=minimum_override_mib,
        )
        profile_contract_value = profile_contract(self.policy, selected_profile)
        archive: pathlib.Path | None = None
        source_digest = "none"
        source_count = 0
        internal_arguments = normalized_arguments
        lifecycle: dict[str, Any] = {}
        before: list[str] = []
        runtime: RuntimeResult | None = None
        lease_acquired = False
        termination_error: RuntimePolicyError | None = None
        try:
            with self.lease(run_id) as lease:
                lease_acquired = True
                lifecycle["lease"] = lease
                if entry_contract.get("requires_source"):
                    source_temp = self.temporary_root / "wsl-source" / run_id
                    archive, source_digest, source_count = _source_archive(
                        self.root, source_temp, self.secret_predicate, self.policy
                    )
                    internal_arguments = [
                        "--source-archive",
                        windows_to_wsl_mount(archive),
                        "--source-digest",
                        source_digest,
                        *internal_arguments,
                    ]
                before = self._list(running=True)
                session_script = windows_to_wsl_mount(
                    self.root / "scripts" / "runtime" / "clearra-wsl-session.sh"
                )
                guest_script = windows_to_wsl_mount(
                    self.root / "scripts" / "runtime" / "clearra-wsl-guest.sh"
                )
                requested_timeout = int(
                    entry_contract.get(
                        "timeout_seconds", profile_contract_value["timeout_seconds"]
                    )
                )
                maximum_timeout = int(profile_contract_value["maximum_timeout_seconds"])
                cleanup_budget = min(60, max(1, maximum_timeout // 5))
                guest_timeout = min(requested_timeout, maximum_timeout - cleanup_budget)
                if guest_timeout <= 0:
                    raise RuntimePolicyError(
                        f"WSL profile leaves no bounded cleanup interval: {selected_profile}"
                    )
                unit = "clearra-" + re.sub(r"[^a-zA-Z0-9_.-]", "-", run_id)
                versions = self.policy["toolchains"]
                admission = calculate_admission(
                    self.policy,
                    selected_profile,
                    minimum_override_mib=minimum_override_mib,
                )
                command = [
                    self.wsl,
                    "-d",
                    self.distribution,
                    "--user",
                    "root",
                    "--exec",
                    "/bin/bash",
                    session_script,
                    "--unit",
                    unit,
                    "--memory-max",
                    str(admission.hard_limit_bytes),
                    "--tasks-max",
                    str(profile_contract_value["maximum_descendant_processes"]),
                    "--runtime-max",
                    str(guest_timeout),
                    "--grace",
                    str(profile_contract_value["termination_grace_seconds"]),
                    "--guest",
                    guest_script,
                    "--mode",
                    "runtime",
                    "--run-as",
                    "clearra",
                    "--entry",
                    entry,
                    "--marker-digest",
                    self.marker_digest(),
                    "--node-version",
                    str(versions["node"]),
                    "--npm-version",
                    str(versions["npm"]),
                    "--pnpm-version",
                    str(versions["pnpm"]),
                    "--rust-version",
                    str(versions["rust"]),
                    "--cargo-version",
                    str(versions["cargo"]),
                    "--wasm-bindgen-version",
                    str(versions["wasm_bindgen"]),
                    "--",
                    *internal_arguments,
                ]
                runtime = run_host_process(
                    command,
                    cwd=self.root,
                    env=os.environ.copy(),
                    policy=self.policy,
                    profile=selected_profile,
                    keep_stdin_open=True,
                    echo=True,
                    minimum_override_mib=minimum_override_mib,
                    timeout_override_seconds=min(
                        maximum_timeout, guest_timeout + cleanup_budget
                    ),
                    admission_override=admission,
                )
                combined = runtime.stdout + "\n" + runtime.stderr
                guest_metadata: dict[str, Any] = {
                    "unit": unit + ".service",
                    "requested_timeout_seconds": requested_timeout,
                    "runtime_timeout_seconds": guest_timeout,
                    "cleanup_budget_seconds": cleanup_budget,
                }
                for key, output_key in (
                    ("CLEARRA_WSL_RESULT", "result"),
                    ("CLEARRA_WSL_CONTROL_GROUP", "control_group"),
                    ("CLEARRA_WSL_MEMORY_PEAK", "memory_peak"),
                    ("CLEARRA_WSL_REMAINING_TASKS", "remaining_tasks"),
                    ("CLEARRA_WSL_UNIT_ACTIVE_STATE", "unit_active_state"),
                    ("CLEARRA_WSL_CGROUP_REMOVED", "cgroup_removed"),
                ):
                    matches = re.findall(rf"(?m)^{key}=(.*)$", combined)
                    guest_metadata[output_key] = matches[-1].strip() if matches else None
                if "CLEARRA_WSL_RESULT=oom-kill" in combined:
                    runtime.reason = "oom"
                    runtime.error_code = MEMORY_LIMIT_ERROR
                elif "CLEARRA_WSL_RESULT=parent-lost" in combined:
                    runtime.reason = "parent-lost"
                    runtime.error_code = PARENT_LOST_ERROR
                elif "CLEARRA_WSL_RESULT=timeout" in combined:
                    runtime.reason = "timeout"
                    runtime.error_code = TIMEOUT_ERROR
                cleanup_incomplete = (
                    guest_metadata.get("remaining_tasks") != "0"
                    or guest_metadata.get("unit_active_state") != "inactive"
                    or guest_metadata.get("cgroup_removed") != "true"
                    or not guest_metadata.get("control_group")
                )
                if cleanup_incomplete:
                    runtime.returncode = 125
                    runtime.reason = "tree-not-stopped"
                    runtime.error_code = TREE_CLEANUP_ERROR
                elif (
                    runtime.returncode == 0
                    and guest_metadata.get("result") != "success"
                ):
                    runtime.returncode = 125
                    runtime.reason = "tree-not-stopped"
                    runtime.error_code = TREE_CLEANUP_ERROR
                lifecycle["guest"] = guest_metadata
        finally:
            try:
                if lease_acquired:
                    try:
                        lifecycle["termination"] = self.terminate()
                    except RuntimePolicyError as error:
                        termination_error = error
            finally:
                if archive is not None:
                    parent = archive.parent
                    archive.unlink(missing_ok=True)
                    with contextlib.suppress(OSError):
                        parent.rmdir()
        if termination_error is not None:
            raise RuntimePolicyError(
                f"{TREE_CLEANUP_ERROR}: dedicated WSL termination verification failed",
                details={
                    "runtime": runtime.receipt() if runtime is not None else None,
                    "guest": lifecycle.get("guest"),
                    "termination_error": str(termination_error),
                },
            ) from termination_error
        if runtime is None:
            raise RuntimePolicyError("WSL runtime completed without a process result")
        after = list(lifecycle["termination"]["running_after"])
        return {
            "schema_id": "clearra.wsl-runtime.v1",
            "run_id": run_id,
            "distribution": self.distribution,
            "entry": entry,
            "profile": selected_profile,
            "source_digest": source_digest,
            "source_file_count": source_count,
            "runtime": runtime.receipt(),
            "runtime_stdout": runtime.stdout,
            "runtime_stderr": runtime.stderr,
            "running_before": before,
            "running_after": after,
            "lifecycle": lifecycle,
            "dedicated_distribution_stopped": self.distribution not in after,
        }


def runtime_audit(
    *,
    root: pathlib.Path,
    policy: Mapping[str, Any],
) -> dict[str, Any]:
    snapshot = memory_snapshot()
    profiles: dict[str, Any] = {}
    for profile in policy.get("resource_profiles", {}):
        try:
            profiles[profile] = {"accepted": True, **dataclasses.asdict(calculate_admission(policy, profile))}
        except RuntimePolicyError as error:
            profiles[profile] = {"accepted": False, "error": str(error)}
    result: dict[str, Any] = {
        "schema_id": "clearra.runtime-audit.v1",
        "platform": platform.platform(),
        "memory": dataclasses.asdict(snapshot),
        "profiles": profiles,
        "process_registry_entries": len(policy.get("process_registry", [])),
    }
    if os.name == "nt":
        user_profile = pathlib.Path(os.environ.get("USERPROFILE", pathlib.Path.home()))
        config = user_profile / ".wslconfig"
        relevant: dict[str, str] = {}
        if config.is_file():
            for line in config.read_text(encoding="utf-8", errors="replace").splitlines():
                match = re.match(
                    r"\s*(memory|processors|swap|autoMemoryReclaim|instanceIdleTimeout)\s*=\s*(.*?)\s*$",
                    line,
                    re.I,
                )
                if match:
                    relevant[match.group(1)] = match.group(2)
        result["wsl_global_config"] = {
            "path": str(config),
            "exists": config.is_file(),
            "relevant_values": relevant,
            "managed_by_clearra": False,
        }
    return result
