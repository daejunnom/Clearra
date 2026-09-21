#!/usr/bin/env python3
"""Clearra storage, toolchain, dependency, and Git policy owner.

The command intentionally uses only the Python standard library.  It may be
invoked before JavaScript or Rust dependencies have been installed.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import fnmatch
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid
import zipfile
from collections import Counter
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any, Iterable, Iterator, Sequence


LOCAL_MODULE_ROOT = pathlib.Path(__file__).resolve().parent
if str(LOCAL_MODULE_ROOT) not in sys.path:
    sys.path.insert(0, str(LOCAL_MODULE_ROOT))
import clearra_runtime as runtime_owner


ROOT = pathlib.Path(__file__).resolve().parents[2]
POLICY_PATH = ROOT / "config" / "clearra-management.v1.json"
ERROR_CODE = "E_CLEARRA_STORAGE_PATH_NOT_ALLOWED"
WARNING = (
    f"{ERROR_CODE}: 이 위치는 Clearra 생성물 관리 정책에 포함되지 않습니다.\n"
    "로컬 대화형 실행에서는 --force-unmanaged-output --force-reason \"<이유>\"로 "
    "이번 실행만 강제할 수 있습니다.\n강제 실행은 권장하지 않습니다."
)


class ManagementError(RuntimeError):
    pass


def load_policy(path: pathlib.Path = POLICY_PATH) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema_id") != "clearra.management.v1" or value.get("policy_version") != 1:
        raise ManagementError("unsupported Clearra management policy")
    return value


def run(
    command: Sequence[str],
    *,
    cwd: pathlib.Path | None = None,
    check: bool = True,
    input_text: str | None = None,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    requested = list(command)
    resolved = shutil.which(requested[0])
    invoked = [resolved or requested[0], *requested[1:]]
    effective_cwd = cwd or ROOT
    try:
        result = subprocess.run(
            invoked,
            cwd=effective_cwd,
            check=False,
            text=True,
            encoding="utf-8",
            errors="replace",
            input=input_text,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
        )
    except FileNotFoundError as error:
        if check:
            raise ManagementError(f"required command is unavailable: {requested[0]}") from error
        result = subprocess.CompletedProcess(requested, 127, "", f"{error}")
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ManagementError(f"command failed ({result.returncode}): {' '.join(requested)}\n{detail}")
    return result


def git(*arguments: str, cwd: pathlib.Path | None = None, check: bool = True) -> str:
    return run(("git", *arguments), cwd=cwd, check=check).stdout.strip()


def delete_refs_atomically(refs: Sequence[tuple[str, str]]) -> None:
    """Delete refs in one transaction without platform newline translation."""
    if not refs:
        return
    material = "start\0"
    material += "".join(f"delete {ref}\0{old_sha}\0" for ref, old_sha in refs)
    material += "prepare\0commit\0"
    run(("git", "update-ref", "--stdin", "-z"), input_text=material)


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def write_json_atomic(path: pathlib.Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + f".{uuid.uuid4().hex[:12]}.tmp")
    temporary.write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    temporary.replace(path)


def utc_stamp() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")


def state_root() -> pathlib.Path:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise ManagementError("LOCALAPPDATA is required on Windows")
        return pathlib.Path(base) / "Clearra" / "state"
    base = os.environ.get("XDG_STATE_HOME")
    if not base:
        home = os.environ.get("HOME")
        if not home:
            raise ManagementError("HOME or XDG_STATE_HOME is required")
        base = str(pathlib.Path(home) / ".local" / "state")
    return pathlib.Path(base) / "Clearra"


def cache_root() -> pathlib.Path:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise ManagementError("LOCALAPPDATA is required on Windows")
        return pathlib.Path(base) / "Clearra" / "cache"
    base = os.environ.get("XDG_CACHE_HOME")
    if not base:
        home = os.environ.get("HOME")
        if not home:
            raise ManagementError("HOME or XDG_CACHE_HOME is required")
        base = str(pathlib.Path(home) / ".cache")
    return pathlib.Path(base) / "Clearra" / "cache"


def build_root() -> pathlib.Path:
    if os.name == "nt":
        base = os.environ.get("RUNNER_TEMP") if os.environ.get("GITHUB_ACTIONS") == "true" else os.environ.get("LOCALAPPDATA")
        if not base:
            raise ManagementError("a Windows Clearra build base is required")
        return pathlib.Path(base) / "Clearra" / "build"
    base = os.environ.get("XDG_CACHE_HOME") or str(pathlib.Path(os.environ["HOME"]) / ".cache")
    return pathlib.Path(base) / "Clearra" / "build"


def logs_root() -> pathlib.Path:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise ManagementError("LOCALAPPDATA is required on Windows")
        return pathlib.Path(base) / "Clearra" / "logs"
    return state_root() / "logs"


def browser_root() -> pathlib.Path:
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        if not base:
            raise ManagementError("LOCALAPPDATA is required on Windows")
        return pathlib.Path(base) / "Clearra" / "browser"
    return state_root() / "browser"


def platform_id() -> str:
    architecture = platform.machine().lower()
    suffix = "aarch64" if architecture in {"arm64", "aarch64"} else "x86_64"
    return f"{'windows' if os.name == 'nt' else 'linux'}-{suffix}"


def managed_cargo_tool(policy: dict[str, Any]) -> pathlib.Path:
    version = policy["toolchains"]["wasm_bindgen"]
    name = "wasm-bindgen.exe" if os.name == "nt" else "wasm-bindgen"
    return (
        ROOT
        / "build"
        / "tools"
        / "cargo"
        / "wasm-bindgen-cli"
        / version
        / platform_id()
        / "bin"
        / name
    )


def normalized(path: pathlib.Path) -> str:
    value = str(path.resolve(strict=False)).replace("\\", "/")
    return value.casefold() if os.name == "nt" else value


def is_within(path: pathlib.Path, parent: pathlib.Path) -> bool:
    candidate = normalized(path)
    root = normalized(parent).rstrip("/")
    return candidate == root or candidate.startswith(root + "/")


def is_reparse_point(path: pathlib.Path) -> bool:
    try:
        information = os.lstat(path)
    except OSError:
        return False
    attributes = getattr(information, "st_file_attributes", 0)
    reparse = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    return stat.S_ISLNK(information.st_mode) or bool(attributes & reparse)


def assert_no_link_escape(path: pathlib.Path, stop: pathlib.Path | None = None) -> None:
    """Reject every existing link/reparse component before resolving the path."""
    cursor = pathlib.Path(os.path.abspath(os.fspath(path.expanduser())))
    boundary = pathlib.Path(os.path.abspath(os.fspath(stop.expanduser()))) if stop else None
    while True:
        if is_reparse_point(cursor):
            raise ManagementError(f"path traverses a symbolic link or junction: {cursor}")
        if boundary is not None and normalized(cursor) == normalized(boundary):
            return
        parent = cursor.parent
        if parent == cursor:
            return
        cursor = parent


def assert_no_mount_escape(path: pathlib.Path, root: pathlib.Path) -> None:
    """Reject nested filesystem mount transitions while accepting the root mount itself."""
    if os.name == "nt":
        return
    candidate = path.resolve(strict=False)
    boundary = root.resolve(strict=False)
    cursor = candidate if candidate.exists() and candidate.is_dir() else candidate.parent
    while normalized(cursor) != normalized(boundary):
        if cursor.exists() and os.path.ismount(cursor):
            raise ManagementError(f"path crosses a mount boundary below the allowed root: {cursor}")
        parent = cursor.parent
        if parent == cursor:
            break
        cursor = parent


def is_secret_path(path: pathlib.Path, policy: dict[str, Any]) -> bool:
    parts = [part.casefold() for part in path.parts]
    for part in parts:
        for pattern in policy["secret_path_patterns"]:
            if fnmatch.fnmatch(part, pattern.casefold()):
                return True
    return False


def producer_ids(policy: dict[str, Any]) -> set[str]:
    return {entry["id"] for entry in policy["producers"]}


def allowed_repository_path(path: pathlib.Path, policy: dict[str, Any]) -> bool:
    for entry in policy["repository_roots"]:
        if is_within(path, ROOT / entry["path"]):
            return True
    return False


def matching_allowed_root(path: pathlib.Path, policy: dict[str, Any]) -> pathlib.Path | None:
    roots = [ROOT / entry["path"] for entry in policy["repository_roots"]]
    roots.extend(allowed_external_roots(policy))
    return next((root for root in roots if is_within(path, root)), None)


def external_root_entries(policy: dict[str, Any]) -> list[tuple[dict[str, Any], pathlib.Path]]:
    roots: list[tuple[dict[str, Any], pathlib.Path]] = [
        ({"id": "state", "shared": False}, state_root()),
        ({"id": "cache", "shared": False}, cache_root()),
        ({"id": "build-cache", "shared": False}, build_root()),
        ({"id": "logs", "shared": False}, logs_root()),
        ({"id": "browser", "shared": False}, browser_root()),
    ]
    home = pathlib.Path.home()
    for entry in policy["external_roots"]:
        if entry["id"] in {"state", "cache", "build-cache", "logs", "browser"}:
            continue
        if "environment" in entry:
            roots.append(
                (
                    entry,
                    pathlib.Path(
                        os.environ.get(
                            entry["environment"],
                            home / entry["fallback"].replace("${HOME}/", ""),
                        )
                    ),
                )
            )
        elif "resolver" in entry:
            parts = entry["resolver"].split()
            resolved = run(parts, check=False)
            if resolved.returncode == 0 and resolved.stdout.strip():
                roots.append((entry, pathlib.Path(resolved.stdout.strip())))
        elif os.name == "nt" and "windows" in entry:
            roots.append((entry, pathlib.Path(os.path.expandvars(entry["windows"]))))
        elif os.name != "nt" and "linux" in entry:
            value = entry["linux"]
            # Resolve the two XDG defaults used by the committed policy without
            # accepting a general shell expansion language.
            value = value.replace(
                "${XDG_CACHE_HOME:-${HOME}/.cache}",
                os.environ.get("XDG_CACHE_HOME", str(home / ".cache")),
            ).replace(
                "${XDG_STATE_HOME:-${HOME}/.local/state}",
                os.environ.get("XDG_STATE_HOME", str(home / ".local" / "state")),
            ).replace(
                "${XDG_RUNTIME_DIR:-/tmp}",
                os.environ.get("XDG_RUNTIME_DIR", "/tmp"),
            ).replace("${HOME}", str(home))
            roots.append((entry, pathlib.Path(value)))
    return roots


def allowed_external_roots(policy: dict[str, Any]) -> list[pathlib.Path]:
    return [path for _entry, path in external_root_entries(policy)]


def force_is_available() -> bool:
    forbidden = any(
        os.environ.get(name)
        for name in ("CI", "GITHUB_ACTIONS", "CLEARRA_RELEASE", "CLEARRA_DEPLOYMENT")
    )
    return sys.stdin.isatty() and sys.stderr.isatty() and not forbidden


def assert_output_path(
    path: pathlib.Path,
    policy: dict[str, Any],
    *,
    force: bool = False,
    force_reason: str | None = None,
) -> pathlib.Path:
    lexical = pathlib.Path(os.path.abspath(os.fspath(path.expanduser())))
    if is_secret_path(lexical, policy):
        raise ManagementError("prohibited credential path blocked; contents were not inspected")
    assert_no_link_escape(lexical)
    candidate = lexical.resolve(strict=False)
    allowed_root = matching_allowed_root(candidate, policy)
    if allowed_root is not None:
        assert_no_link_escape(lexical, allowed_root)
        assert_no_mount_escape(candidate, allowed_root)
        return candidate
    if force:
        if not force_is_available():
            raise ManagementError("unmanaged output override is unavailable outside a local interactive run")
        if not force_reason or not force_reason.strip():
            raise ManagementError("--force-reason is required with --force-unmanaged-output")
        return candidate
    raise ManagementError(f"{WARNING}\n거부된 경로: {candidate}")


def repository_head() -> str:
    result = git("rev-parse", "HEAD", check=False)
    return result or "unborn"


def write_receipt(kind: str, payload: dict[str, Any]) -> pathlib.Path:
    directory = state_root() / "receipts" / dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%d")
    directory.mkdir(parents=True, exist_ok=True)
    receipt = {
        "schema_id": "clearra.management-receipt.v1",
        "kind": kind,
        "created_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "repository": str(ROOT),
        "head": repository_head(),
        **payload,
    }
    path = directory / f"{utc_stamp()}-{kind}-{uuid.uuid4().hex[:12]}.json"
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(receipt, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    temporary.replace(path)
    return path


def directory_measurement(
    path: pathlib.Path,
    *,
    max_depth: int | None = None,
    max_entries: int | None = None,
    time_budget_seconds: float | None = None,
) -> dict[str, Any]:
    """Measure without following links, with an explicit bound for large stores.

    An incomplete result reports a byte lower bound rather than pretending that
    a time-bounded sample is the full size. Shared package and toolchain stores
    can contain data owned by many projects, so the audit must not turn into an
    unbounded traversal of another project's cache.
    """
    if not path.exists():
        return {
            "path": str(path),
            "exists": False,
            "files": 0,
            "bytes": 0,
            "complete": True,
            "measurement_kind": "exact",
        }
    files = 0
    size = 0
    sampled_entries = 0
    complete = True
    started = time.monotonic()
    stop = False
    for base, directories, names in os.walk(path, topdown=True, followlinks=False):
        base_path = pathlib.Path(base)
        directories[:] = [
            name for name in directories if not is_reparse_point(base_path / name)
        ]
        depth = len(base_path.relative_to(path).parts)
        if max_depth is not None and depth >= max_depth and directories:
            complete = False
            directories[:] = []
        for name in names:
            if max_entries is not None and sampled_entries >= max_entries:
                complete = False
                stop = True
                break
            if (
                time_budget_seconds is not None
                and time.monotonic() - started >= time_budget_seconds
            ):
                complete = False
                stop = True
                break
            candidate = base_path / name
            try:
                if is_reparse_point(candidate):
                    continue
                size += candidate.stat().st_size
                files += 1
                sampled_entries += 1
            except OSError:
                continue
        if stop:
            break
    return {
        "path": str(path),
        "exists": True,
        "files": files,
        "bytes": size,
        "complete": complete,
        "measurement_kind": "exact" if complete else "bounded-lower-bound",
        "sampled_entries": sampled_entries,
    }


def tree_identity_snapshot(
    path: pathlib.Path, *, max_depth: int | None = None
) -> dict[str, dict[str, int | str]]:
    """Capture metadata only; shared-store file contents are never opened."""
    snapshot: dict[str, dict[str, int | str]] = {}
    if not path.exists():
        return snapshot
    for base, directories, names in os.walk(path, followlinks=False):
        depth = len(pathlib.Path(base).relative_to(path).parts)
        directories[:] = [
            name for name in directories if not is_reparse_point(pathlib.Path(base) / name)
        ]
        entry_names = [*directories, *names]
        for name in entry_names:
            candidate = pathlib.Path(base) / name
            try:
                if is_reparse_point(candidate):
                    continue
                information = candidate.stat()
                relative = candidate.relative_to(path).as_posix()
                snapshot[relative] = {
                    "file_id": f"{information.st_dev}:{information.st_ino}",
                    "bytes": information.st_size,
                    "mtime_ns": information.st_mtime_ns,
                }
            except OSError:
                continue
        if max_depth is not None and depth >= max_depth:
            directories[:] = []
    return snapshot


def tree_identity_delta(
    root: pathlib.Path,
    before: dict[str, dict[str, int | str]],
    after: dict[str, dict[str, int | str]],
) -> dict[str, Any]:
    added = [{"path": name, **after[name]} for name in sorted(after.keys() - before.keys())]
    removed = [{"path": name, **before[name]} for name in sorted(before.keys() - after.keys())]
    changed = [
        {"path": name, "before": before[name], "after": after[name]}
        for name in sorted(before.keys() & after.keys())
        if before[name] != after[name]
    ]
    return {
        "root": str(root),
        "before_files": len(before),
        "after_files": len(after),
        "byte_delta": sum(int(item["bytes"]) for item in after.values())
        - sum(int(item["bytes"]) for item in before.values()),
        "byte_delta_kind": "sampled-entry-metadata",
        "added": added,
        "removed": removed,
        "changed": changed,
    }


def storage_audit(policy: dict[str, Any]) -> dict[str, Any]:
    roots: list[dict[str, Any]] = []
    for entry in policy["repository_roots"]:
        roots.append(
            {
                "id": entry["id"],
                "scope": "repository",
                "classes": entry["classes"],
                "lifecycle": entry["lifecycle"],
                "producer_estimate": entry["owners"],
                **directory_measurement(
                    ROOT / entry["path"],
                    max_entries=50_000,
                    time_budget_seconds=2.0,
                ),
            }
        )
    seen: set[str] = set()
    for entry, root in external_root_entries(policy):
        identity = normalized(root)
        if identity in seen:
            continue
        seen.add(identity)
        roots.append(
            {
                "id": entry["id"],
                "scope": "external-shared" if entry.get("shared") else "external-clearra",
                "lifecycle": entry.get("lifecycle"),
                **directory_measurement(
                    root,
                    max_depth=2 if entry.get("shared") else None,
                    max_entries=20_000 if entry.get("shared") else 50_000,
                    time_budget_seconds=2.0,
                ),
            }
        )
    legacy = []
    for relative in ("target", ".cargo/target", "dist", "dist-server", ".cache", ".pnpm-store"):
        path = ROOT / relative
        if path.exists():
            legacy.append(
                {
                    "producer_estimate": "unknown-legacy",
                    "automatic_cleanup": False,
                    **directory_measurement(
                        path,
                        max_entries=50_000,
                        time_budget_seconds=2.0,
                    ),
                }
            )
    return {"schema_id": "clearra.storage-audit.v1", "head": repository_head(), "roots": roots, "legacy": legacy}


def storage_verify(arguments: argparse.Namespace, policy: dict[str, Any]) -> int:
    verify_static_management_policy(policy)
    if arguments.path:
        path = assert_output_path(
            pathlib.Path(arguments.path),
            policy,
            force=arguments.force_unmanaged_output,
            force_reason=arguments.force_reason,
        )
        print(path)
        if arguments.force_unmanaged_output and matching_allowed_root(path, policy) is None:
            receipt = write_receipt("forced-output", {"path": str(path), "reason": arguments.force_reason})
            print(f"receipt={receipt}")
    else:
        print("storage_policy=accepted")
    return 0


def remove_owned_path(path: pathlib.Path, policy: dict[str, Any]) -> None:
    """Remove one policy-owned path without following links or mount escapes."""
    assert_output_path(path, policy)
    if not path.exists():
        return
    assert_no_link_escape(path)
    if path.is_dir():
        for base, directories, names in os.walk(path, topdown=True, followlinks=False):
            for name in [*directories, *names]:
                candidate = pathlib.Path(base) / name
                if is_reparse_point(candidate):
                    raise ManagementError(
                        f"cleanup refuses a tree containing a link or junction: {candidate}"
                    )
        def remove_readonly(function: Any, value: str, _error: Any) -> None:
            os.chmod(value, stat.S_IWRITE)
            function(value)

        shutil.rmtree(path, onerror=remove_readonly)
    else:
        path.unlink(missing_ok=True)


def validate_managed_command(producer: str, command: Sequence[str]) -> None:
    if producer != "cargo" or pathlib.Path(command[0]).stem.casefold() != "cargo":
        return
    cargo_arguments = command[1 : command.index("--") if "--" in command else len(command)]
    if any(
        re.match(
            r"^(?:--target-dir|--build-dir|--artifact-dir|--out-dir|--config)(?:=|$)",
            value,
        )
        for value in cargo_arguments
    ):
        raise ManagementError("Cargo output and config overrides are owned by Clearra management")


def storage_run(arguments: argparse.Namespace, policy: dict[str, Any]) -> int:
    verify_static_management_policy(policy)
    if arguments.producer not in producer_ids(policy):
        raise ManagementError(f"unknown producer: {arguments.producer}")
    command = list(arguments.command)
    if command and command[0] == "--":
        command.pop(0)
    if not command:
        raise ManagementError("storage run requires a command after --")
    executable = shutil.which(command[0])
    if not executable:
        raise ManagementError(f"required command is unavailable: {command[0]}")
    validate_managed_command(arguments.producer, command)
    producer = next(item for item in policy["producers"] if item["id"] == arguments.producer)
    selected_profile = getattr(arguments, "profile", None) or producer["default_profile"]
    selected_contract = runtime_owner.profile_contract(policy, selected_profile)
    allowed_profiles = {producer["default_profile"]}
    allowed_profiles.update(
        entry["profile"]
        for entry in policy.get("process_registry", [])
        if entry.get("producer") == arguments.producer
    )
    if selected_profile not in allowed_profiles:
        raise ManagementError(
            f"runtime profile is not registered for producer: "
            f"{arguments.producer}/{selected_profile}"
        )
    if (
        selected_contract.get("explicit_timeout_required")
        or selected_contract.get("explicit_lease_required")
    ) and getattr(arguments, "timeout", None) is None:
        raise ManagementError(
            f"runtime profile requires an explicit --timeout: {selected_profile}"
        )
    export_paths = []
    for value in arguments.export_path:
        lexical = pathlib.Path(os.path.abspath(os.fspath(pathlib.Path(value).expanduser())))
        if is_secret_path(lexical, policy):
            raise ManagementError("prohibited credential export path blocked; contents were not inspected")
        assert_no_link_escape(lexical)
        resolved = lexical.resolve(strict=False)
        if is_within(resolved, ROOT) and matching_allowed_root(resolved, policy) is None:
            raise ManagementError("an export capability cannot bypass repository output policy")
        export_paths.append(str(resolved))
    run_id = utc_stamp() + "-" + uuid.uuid4().hex[:12]
    managed_state = state_root() / arguments.producer / run_id
    managed_temp = ROOT / "_local" / "tmp" / arguments.producer / run_id
    managed_state.mkdir(parents=True, exist_ok=False)
    managed_temp.mkdir(parents=True, exist_ok=False)
    output_roots: dict[str, str] = {}
    artifact_classes = {"test", "benchmark", "analysis", "research"}
    for output_class in producer["classes"]:
        if output_class in artifact_classes:
            output_roots[output_class] = str(
                ROOT / "_local" / "artifacts" / output_class / run_id
            )
        elif output_class in {"receipt", "lock", "checkpoint", "state"}:
            output_roots[output_class] = str(managed_state)
        elif output_class == "temporary":
            output_roots[output_class] = str(managed_temp)
        elif output_class == "browser":
            output_roots[output_class] = str(browser_root() / run_id)
        else:
            output_roots[output_class] = str(ROOT / "build" / arguments.producer / "default")
    environment = os.environ.copy()
    environment.update(
        {
            "CLEARRA_MANAGEMENT_POLICY": str(POLICY_PATH),
            "CLEARRA_MANAGED_PRODUCER": arguments.producer,
            "CLEARRA_STATE_ROOT": str(state_root()),
            "CLEARRA_CACHE_ROOT": str(cache_root()),
            "CLEARRA_BUILD_ROOT": str(build_root()),
            "CLEARRA_MANAGED_RUN_ID": run_id,
            "CLEARRA_RUNTIME_SUPERVISED": "1",
            "CLEARRA_MANAGED_OUTPUT_ROOTS": json.dumps(output_roots, separators=(",", ":")),
            "CLEARRA_EXPORT_PATHS": json.dumps(export_paths, separators=(",", ":")),
        }
    )
    if arguments.producer == "cargo":
        environment["CARGO_TARGET_DIR"] = str(ROOT / "build" / "cargo" / "host" / "debug")
        environment["CARGO_INCREMENTAL"] = "0"
        environment["CARGO_BUILD_RUSTC_WRAPPER"] = ""
        environment.pop("RUSTC_WRAPPER", None)
        environment.pop("RUSTC_WORKSPACE_WRAPPER", None)
    environment["CLEARRA_RUNTIME_PROFILE"] = selected_profile
    producer_context = policy.get("runtime_policy", {}).get(
        "producer_hard_containment_contexts", {}
    ).get(arguments.producer)
    if producer_context:
        context_variable = policy["runtime_policy"]["hard_containment_context_env"].get(
            producer_context
        )
        if not context_variable:
            raise ManagementError(
                f"producer has an unknown hard-containment context: {arguments.producer}"
            )
        environment[str(context_variable)] = "1"
    redacted_command = runtime_owner.redact_argv(command)
    started = time.monotonic()
    try:
        try:
            runtime_result = runtime_owner.run_host_process(
                [executable, *command[1:]],
                cwd=ROOT,
                env=environment,
                policy=policy,
                profile=selected_profile,
                minimum_override_mib=getattr(arguments, "minimum_memory_mib", None),
                timeout_override_seconds=getattr(arguments, "timeout", None),
            )
        except runtime_owner.RuntimePolicyError as error:
            denied_receipt = write_receipt(
                "managed-run-denied",
                {
                    "producer": arguments.producer,
                    "profile": selected_profile,
                    "command": redacted_command,
                    "command_sha256": runtime_owner.command_digest(command),
                    "error_type": type(error).__name__,
                    "error": str(error),
                    "output_roots": output_roots,
                    "owned_paths": [str(managed_state)],
                },
            )
            raise ManagementError(f"{error}; receipt={denied_receipt}") from error
    finally:
        remove_owned_path(managed_temp, policy)
    owned_paths = [str(managed_state)]
    for value in output_roots.values():
        candidate = pathlib.Path(value)
        if run_id in candidate.parts and candidate.exists() and candidate != managed_temp:
            owned_paths.append(str(candidate))
    receipt = write_receipt(
        "managed-run",
        {
            "producer": arguments.producer,
            "profile": selected_profile,
            "command": redacted_command,
            "command_sha256": runtime_result.command_sha256,
            "exit_code": runtime_result.returncode,
            "duration_ms": round((time.monotonic() - started) * 1000),
            "runtime": runtime_result.receipt(),
            "output_roots": output_roots,
            "export_capability_count": len(export_paths),
            "owned_paths": owned_paths,
            "cleaned_paths": [str(managed_temp)],
        },
    )
    receipt_stream = sys.stderr if getattr(arguments, "domain", None) == "runtime" else sys.stdout
    print(f"receipt={receipt}", file=receipt_stream)
    if runtime_result.error_code:
        print(
            f"{runtime_result.error_code}: process ended with reason={runtime_result.reason}",
            file=sys.stderr,
        )
    return runtime_result.returncode


def exact_tool_versions(policy: dict[str, Any]) -> dict[str, tuple[list[str], re.Pattern[str]]]:
    versions = policy["toolchains"]
    return {
        "node": (["node", "--version"], re.compile(rf"^v{re.escape(versions['node'])}$")),
        "npm": (["npm", "--version"], re.compile(rf"^{re.escape(versions['npm'])}$")),
        "pnpm": (["pnpm", "--version"], re.compile(rf"^{re.escape(versions['pnpm'])}$")),
        "rust": (["rustc", "--version"], re.compile(rf"^rustc {re.escape(versions['rust'])}\b")),
        "cargo": (["cargo", "--version"], re.compile(rf"^cargo {re.escape(versions['cargo'])}\b")),
        "wasm_bindgen": ([str(managed_cargo_tool(policy)), "--version"], re.compile(rf"^wasm-bindgen {re.escape(versions['wasm_bindgen'])}$")),
    }


def toolchain_check(policy: dict[str, Any]) -> int:
    results = {}
    failed = False
    for name, (command, expected) in exact_tool_versions(policy).items():
        value = run(command, check=False)
        actual = value.stdout.strip() if value.returncode == 0 else None
        accepted = bool(actual and expected.search(actual))
        results[name] = {"actual": actual, "accepted": accepted}
        failed = failed or not accepted
    results["rust_toolchain_file"] = {"path": str(ROOT / "rust-toolchain.toml"), "accepted": (ROOT / "rust-toolchain.toml").is_file()}
    results["pnpm_lock"] = {"path": str(ROOT / "pnpm-lock.yaml"), "accepted": (ROOT / "pnpm-lock.yaml").is_file()}
    failed = failed or not results["rust_toolchain_file"]["accepted"]
    failed = failed or not results["pnpm_lock"]["accepted"]
    rust_version = policy["toolchains"]["rust"]
    components = run(("rustup", "component", "list", "--toolchain", rust_version), check=False)
    targets = run(("rustup", "target", "list", "--toolchain", rust_version), check=False)
    for component in ("rustfmt", "clippy"):
        accepted = components.returncode == 0 and bool(
            re.search(rf"^{re.escape(component)}(?:-[^ ]+)? \(installed\)$", components.stdout, re.M)
        )
        results[f"rust_component_{component}"] = {"accepted": accepted}
        failed = failed or not accepted
    target = "wasm32-unknown-unknown"
    target_accepted = targets.returncode == 0 and bool(
        re.search(rf"^{re.escape(target)} \(installed\)$", targets.stdout, re.M)
    )
    results[f"rust_target_{target}"] = {"accepted": target_accepted}
    failed = failed or not target_accepted
    node_file = (ROOT / ".node-version").read_text(encoding="utf-8").strip() if (ROOT / ".node-version").is_file() else None
    results["node_version_file"] = {
        "actual": node_file,
        "accepted": node_file == policy["toolchains"]["node"],
    }
    failed = failed or not results["node_version_file"]["accepted"]
    print(json.dumps(results, indent=2, ensure_ascii=False))
    return 1 if failed else 0


def constrain_managed_cargo_install_jobs(
    environment: dict[str, str], *, platform_name: str = os.name
) -> None:
    """Bound local Windows tool bootstrap memory without changing workspace builds."""
    if platform_name == "nt" and not environment.get("CI"):
        environment["CARGO_BUILD_JOBS"] = "1"


def toolchain_sync(policy: dict[str, Any]) -> int:
    verify_static_management_policy(policy)
    versions = policy["toolchains"]
    node = run(("node", "--version"), check=False).stdout.strip()
    if node != f"v{versions['node']}":
        raise ManagementError(
            f"Node {versions['node']} must be selected before sync; found {node or 'unavailable'}"
        )
    if run(("pnpm", "--version"), check=False).stdout.strip() != versions["pnpm"]:
        run(("corepack", "enable"))
        run(("corepack", "prepare", f"pnpm@{versions['pnpm']}", "--activate"))
    cargo_home = pathlib.Path(
        os.environ.get("CARGO_HOME", str(pathlib.Path.home() / ".cargo"))
    )
    rustup_home = pathlib.Path(
        os.environ.get("RUSTUP_HOME", str(pathlib.Path.home() / ".rustup"))
    )
    shared_before = {
        "cargo": tree_identity_snapshot(cargo_home, max_depth=2),
        "rustup": tree_identity_snapshot(rustup_home, max_depth=2),
    }
    run(
        (
            "rustup",
            "toolchain",
            "install",
            versions["rust"],
            "--profile",
            "minimal",
            "--component",
            "rustfmt",
            "--component",
            "clippy",
            "--target",
            "wasm32-unknown-unknown",
        )
    )
    tool_root = (
        ROOT
        / "build"
        / "tools"
        / "cargo"
        / "wasm-bindgen-cli"
        / versions["wasm_bindgen"]
        / platform_id()
    )
    tool_root.mkdir(parents=True, exist_ok=True)
    managed_version = run((str(managed_cargo_tool(policy)), "--version"), check=False)
    install_mode = "existing-managed-tool"
    if managed_version.stdout.strip() != f"wasm-bindgen {versions['wasm_bindgen']}":
        temporary = ROOT / "_local" / "tmp" / "management" / (
            "toolchain-" + utc_stamp() + "-" + uuid.uuid4().hex[:12]
        )
        temporary.mkdir(parents=True, exist_ok=False)
        install_environment = os.environ.copy()
        install_environment.update(
            {
                "CARGO_TARGET_DIR": str(
                    ROOT / "build" / "cargo" / "host" / "release" / "tool-install"
                ),
                "CARGO_INCREMENTAL": "0",
                "CARGO_BUILD_RUSTC_WRAPPER": "",
                "TEMP": str(temporary),
                "TMP": str(temporary),
            }
        )
        constrain_managed_cargo_install_jobs(install_environment)
        install_environment.pop("RUSTC_WRAPPER", None)
        install_environment.pop("RUSTC_WORKSPACE_WRAPPER", None)
        try:
            install = run(
                (
                    "cargo",
                    f"+{versions['rust']}",
                    "install",
                    "wasm-bindgen-cli",
                    "--version",
                    versions["wasm_bindgen"],
                    "--locked",
                    "--root",
                    str(tool_root),
                ),
                check=False,
                env=install_environment,
            )
        finally:
            remove_owned_path(temporary, policy)
        install_mode = "cargo-install"
    else:
        install = subprocess.CompletedProcess([], 0, managed_version.stdout, "")
    if install.returncode != 0:
        blocked_by_application_control = (
            os.name == "nt"
            and not os.environ.get("CI")
            and (
                "4551" in install.stderr
                or "application control" in install.stderr.casefold()
                or "애플리케이션 제어 정책" in install.stderr
            )
        )
        source = pathlib.Path(shutil.which("wasm-bindgen") or "")
        source_version = run((str(source), "--version"), check=False) if source.is_file() else None
        if not (
            blocked_by_application_control
            and source_version is not None
            and source_version.stdout.strip() == f"wasm-bindgen {versions['wasm_bindgen']}"
        ):
            detail = install.stderr.strip() or install.stdout.strip()
            raise ManagementError(f"managed wasm-bindgen installation failed\n{detail}")
        destination = tool_root / "bin"
        destination.mkdir(parents=True, exist_ok=True)
        copied: list[dict[str, Any]] = []
        for name in ("wasm-bindgen.exe", "wasm-bindgen-test-runner.exe", "wasm2es6js.exe"):
            candidate = source.parent / name
            if not candidate.is_file():
                raise ManagementError(f"verified shared Cargo install is incomplete: {candidate}")
            target = destination / name
            shutil.copy2(candidate, target)
            copied.append({"name": name, "sha256": sha256_file(target), "bytes": target.stat().st_size})
        provenance = {
            "schema_id": "clearra.managed-cargo-tool-bootstrap.v1",
            "reason": "windows-application-control-4551",
            "version": versions["wasm_bindgen"],
            "source_root": str(source.parent),
            "files": copied,
        }
        (tool_root / "install-provenance.json").write_text(
            json.dumps(provenance, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        install_mode = "verified-local-cargo-copy-after-4551"
    shared_after = {
        "cargo": tree_identity_snapshot(cargo_home, max_depth=2),
        "rustup": tree_identity_snapshot(rustup_home, max_depth=2),
    }
    receipt = write_receipt(
        "toolchain-sync",
        {
            "versions": versions,
            "cargo_tool_root": str(tool_root),
            "cargo_tool_install_mode": install_mode,
            "shared_store_changes": [
                tree_identity_delta(cargo_home, shared_before["cargo"], shared_after["cargo"]),
                tree_identity_delta(rustup_home, shared_before["rustup"], shared_after["rustup"]),
            ],
        },
    )
    print(f"receipt={receipt}")
    return toolchain_check(policy)


def require_pnpm(policy: dict[str, Any]) -> None:
    actual = run(("pnpm", "--version"), check=False).stdout.strip()
    expected = policy["toolchains"]["pnpm"]
    if actual != expected:
        raise ManagementError(f"pnpm {expected} is required; found {actual or 'unavailable'}")


def workspace_node_modules() -> list[pathlib.Path]:
    values = [ROOT / "node_modules"]
    for group in ("apps", "packages"):
        parent = ROOT / group
        if not parent.is_dir():
            continue
        values.extend(
            child / "node_modules" for child in parent.iterdir() if child.is_dir()
        )
    return values


def remove_pnpm_link_tree(path: pathlib.Path) -> None:
    """Remove a known pnpm link view without traversing a reparse point."""
    if not path.exists() and not is_reparse_point(path):
        return
    if is_reparse_point(path):
        try:
            path.unlink()
        except (IsADirectoryError, PermissionError):
            os.rmdir(path)
        return
    if path.is_dir():
        with os.scandir(path) as entries:
            children = [pathlib.Path(entry.path) for entry in entries]
        for child in children:
            remove_pnpm_link_tree(child)
        try:
            path.rmdir()
        except PermissionError:
            os.chmod(path, stat.S_IWRITE)
            path.rmdir()
        return
    try:
        path.unlink()
    except PermissionError:
        os.chmod(path, stat.S_IWRITE)
        path.unlink()


def repository_policy_files() -> list[pathlib.Path]:
    tracked = set(filter(None, git("ls-files", "-z").split("\0")))
    untracked = set(filter(None, git("ls-files", "--others", "--exclude-standard", "-z").split("\0")))
    return [ROOT / relative for relative in sorted(tracked | untracked)]


def dependency_policy_failures(policy: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    permitted_legacy = {".github/workflows/pages-rollback.yml"}
    text_only = {
        "AGENTS.md",
        "README.md",
        "scripts/architecture/validate_release_static_contract.ps1",
        "scripts/architecture/validate_test_policy.ps1",
        "scripts/tools/validate-release-cli-smokes.mjs",
    }
    npm_workspace = re.compile(r"(?:^|[\s;&|])npm\s+(?:ci|install|exec|run|test)(?:\s|$)")
    pnpm_install = re.compile(r"(?:^|[\s;&|])pnpm\s+install(?:\s|$)")
    direct_cargo_install = re.compile(
        r"(?:^|[\s;&|])cargo(?:\s+['\"]?\+[0-9.]+['\"]?)?\s+install(?:\s|$)"
    )
    for path in repository_policy_files():
        relative = path.relative_to(ROOT).as_posix()
        if relative in permitted_legacy or relative in text_only or ".test." in path.name:
            continue
        if (
            path.suffix.lower() not in {".yml", ".yaml", ".json", ".ps1", ".mjs", ".js", ".sh"}
            and not path.name.startswith("Dockerfile")
        ):
            continue
        if is_secret_path(path, policy) or not path.is_file() or path.stat().st_size > 2_000_000:
            continue
        material = path.read_text(encoding="utf-8", errors="replace")
        if npm_workspace.search(material):
            failures.append(f"forbidden npm workspace command: {relative}")
        for line_number, line in enumerate(material.splitlines(), start=1):
            if pnpm_install.search(line) and not (
                "--frozen-lockfile" in line and "--ignore-scripts" in line
            ):
                failures.append(f"unfrozen or lifecycle-enabled pnpm install: {relative}:{line_number}")
            if direct_cargo_install.search(line) and relative != "scripts/tools/install-managed-cargo-tool.ps1":
                failures.append(f"unmanaged cargo install: {relative}:{line_number}")
    return failures


def writer_candidate(path: pathlib.Path) -> bool:
    if path.suffix.lower() not in {".py", ".mjs", ".js", ".ts", ".mts", ".ps1", ".sh", ".rs"}:
        return False
    material = path.read_text(encoding="utf-8", errors="replace")
    patterns = (
        r"\b(?:writeFile|writeFileSync|appendFile|createWriteStream|mkdirSync|rmSync|copyFile|renameSync)\b",
        r"\.(?:write_text|write_bytes)\(",
        r"\b(?:New-Item|Out-File|Set-Content|Add-Content|Copy-Item|Move-Item|Remove-Item)\b",
        r"\b(?:create_dir|create_dir_all|fs::write|File::create|fs::copy|fs::rename|fs::remove_)\b",
        r"\b(?:tarfile\.open|os\.open)\b|\.(?:mkdir|unlink)\(",
        r"(?m)^\s*(?:rm|cp|mv|mkdir|install|tar)\s+",
    )
    return any(re.search(pattern, material) for pattern in patterns)


def process_candidate(path: pathlib.Path) -> bool:
    suffix = path.suffix.lower()
    relative = path.relative_to(ROOT).as_posix()
    if path.name.startswith("Dockerfile"):
        material = path.read_text(encoding="utf-8", errors="replace")
        return bool(re.search(r"(?m)^\s*(?:RUN|CMD|ENTRYPOINT)\b", material))
    if suffix in {".yml", ".yaml"} and relative.startswith(".github/workflows/"):
        material = path.read_text(encoding="utf-8", errors="replace")
        return bool(re.search(r"(?m)^\s*run:\s*", material))
    if suffix not in {".py", ".mjs", ".js", ".ts", ".mts", ".ps1", ".sh", ".rs"}:
        return False
    material = path.read_text(encoding="utf-8", errors="replace")
    return bool(
        (suffix == ".py" and re.search(
            r"(?:subprocess\.(?:run|Popen|call|check_call|check_output)|"
            r"asyncio\.create_subprocess_(?:exec|shell)|"
            r"os\.(?:system|popen|posix_spawn|spawn(?:l|le|lp|lpe|v|ve|vp|vpe)|"
            r"exec(?:l|le|lp|lpe|v|ve|vp|vpe)))\b",
            material,
        ))
        or (suffix == ".ps1" and re.search(
            r"\b(?:Start-Process|System\.Diagnostics\.Process|Diagnostics\.Process|"
            r"ProcessStartInfo)\b",
            material,
        ))
        or (suffix == ".ps1" and re.search(
            r"(?m)(?:^|[;|({=,])\s*@?\(?\s*&\s+"
            r"(?:['\"]|\$[A-Za-z_]|[A-Za-z0-9_.-]+)",
            material,
        ))
        or (
            suffix in {".mjs", ".js", ".ts", ".mts"}
            and re.search(r"(?:node:)?child_process", material)
        )
        or (
            suffix in {".mjs", ".js", ".ts", ".mts"}
            and re.search(r"(?:node:)?worker_threads", material)
            and re.search(r"\bWorker\b", material)
        )
        or (
            suffix == ".rs"
            and
            re.search(r"(?:std|tokio)::process", material)
            and re.search(r"\bCommand::new\s*\(", material)
        )
        or (
            suffix == ".sh"
            and re.search(
                r"(?m)(?:^|[;&|({]|\bthen\b|\belse\b|\bdo\b)\s*(?:exec\s+|systemd-run\s|systemctl\s|runuser\s|curl\s|"
                r"cargo\s|rustc\s|node\s|pnpm\s|python3?\s|bash\s|dash\s|sh\s)",
                material,
            )
        )
    )


def raw_wsl_invocation(path: pathlib.Path) -> bool:
    relative = path.relative_to(ROOT).as_posix()
    if relative == "scripts/management/clearra_runtime.py" or ".test." in path.name:
        return False
    if relative.startswith("scripts/management/tests/") or relative.startswith("tests/"):
        return False
    if path.suffix.lower() not in {".py", ".mjs", ".js", ".ts", ".mts", ".ps1", ".sh", ".rs"}:
        return False
    material = path.read_text(encoding="utf-8", errors="replace")
    patterns = (
        r"(?i)\b(?:spawn|spawnSync|execFile|execFileSync|capture|run)\s*\(\s*(?:\[\s*)?['\"]wsl(?:\.exe)?['\"]",
        r"(?i)\bInvoke-[A-Za-z0-9_-]+\s+['\"]wsl(?:\.exe)?['\"]",
        r"(?i)&\s*wsl(?:\.exe)?\b",
        r"(?mi)\b(?:Get-Command|Start-Process)\s+['\"]?wsl(?:\.exe)?\b",
        r"(?mi)\b(?:ProjectionCommand|SyntaxCommand)\s*=\s*['\"]wsl\.exe['\"]",
    )
    return any(re.search(pattern, material) for pattern in patterns)


def runtime_policy_failures(policy: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    profiles = policy.get("resource_profiles", {})
    required_profile_fields = {
        "minimum_memory_mib",
        "maximum_memory_mib",
        "timeout_seconds",
        "maximum_timeout_seconds",
        "termination_grace_seconds",
        "maximum_descendant_processes",
        "output_limit_bytes",
        "hard_containment_required",
        "oom_policy",
        "admission_basis",
        "concurrency_class",
    }
    for profile, contract in profiles.items():
        missing = required_profile_fields - set(contract)
        if missing:
            failures.append(f"resource profile is incomplete: {profile} ({', '.join(sorted(missing))})")
        elif (
            int(contract["minimum_memory_mib"]) <= 0
            or (
                contract.get("maximum_memory_mib") is not None
                and int(contract["maximum_memory_mib"])
                < int(contract["minimum_memory_mib"])
            )
            or int(contract["timeout_seconds"]) <= 0
            or int(contract["maximum_timeout_seconds"]) < int(contract["timeout_seconds"])
            or int(contract["termination_grace_seconds"]) < 0
            or int(contract["maximum_descendant_processes"]) <= 0
            or int(contract["output_limit_bytes"]) <= 0
        ):
            failures.append(f"resource profile has an invalid bound: {profile}")
    expected_profiles = {
        "control": (256, 512, 300, 300, 32),
        "build-test": (3072, None, 5400, 5400, 512),
        "benchmark-search": (4096, None, 7200, 7200, 256),
        "local-service": (512, 2048, 7200, 7200, 64),
        "cloud-job": (16384, 16384, 840, 900, 64),
    }
    for profile, expected in expected_profiles.items():
        contract = profiles.get(profile)
        actual = None if not isinstance(contract, Mapping) else (
            contract.get("minimum_memory_mib"),
            contract.get("maximum_memory_mib"),
            contract.get("timeout_seconds"),
            contract.get("maximum_timeout_seconds"),
            contract.get("maximum_descendant_processes"),
        )
        if actual != expected:
            failures.append(f"resource profile differs from its bounded contract: {profile}")
    cloud = profiles.get("cloud-job", {})
    if (
        cloud.get("cpu_count") != 8
        or cloud.get("concurrency") != 1
        or cloud.get("maximum_jobs") != 1
        or cloud.get("hard_containment_required") is not True
    ):
        failures.append("cloud-job must preserve the 8-vCPU 16-GiB single-job contract")
    if not profiles.get("benchmark-search", {}).get("explicit_timeout_required"):
        failures.append("benchmark-search must require an explicit timeout")
    if not profiles.get("local-service", {}).get("explicit_lease_required"):
        failures.append("local-service must require an explicit lease")
    required_process_fields = {
        "profile",
        "runtime",
        "tree_ownership",
        "timeout",
        "termination_grace",
        "memory_enforcement",
        "output_limit",
        "oom_policy",
    }
    allowed_tree_ownership = {
        "root-supervisor",
        "runtime-supervisor-owned",
        "supervisor-inherited",
    }
    allowed_memory_enforcement = {
        "host": "profile-hard-limit",
        "host-supervisor": "profile-hard-limit",
        "container-cgroup": "container-cgroup",
        "posix-lease-wrapper": "supervisor-cgroup-hard-limit",
        "wsl-bootstrap": "wsl-cgroup-v2",
        "wsl-guest": "wsl-cgroup-v2",
    }
    process_registry = policy.get("process_registry", [])
    duplicate_process_paths = sorted(
        path
        for path, count in Counter(
            str(entry.get("path")) for entry in process_registry
        ).items()
        if count > 1
    )
    failures.extend(
        f"duplicate process registration: {path}" for path in duplicate_process_paths
    )
    for entry in process_registry:
        missing = required_process_fields - set(entry)
        if missing:
            failures.append(
                f"process registration has no complete resource contract: {entry.get('path')} "
                f"({', '.join(sorted(missing))})"
            )
            continue
        if entry["profile"] not in profiles:
            failures.append(
                f"process registration has unknown resource profile: {entry.get('path')} "
                f"({entry.get('profile')})"
            )
        elif int(entry["timeout"]) > int(profiles[entry["profile"]]["maximum_timeout_seconds"]):
            failures.append(f"process timeout exceeds profile: {entry.get('path')}")
        elif int(entry["timeout"]) <= 0:
            failures.append(f"process timeout is unbounded: {entry.get('path')}")
        if entry["tree_ownership"] not in allowed_tree_ownership:
            failures.append(f"process tree termination is unmanaged: {entry.get('path')}")
        expected_enforcement = allowed_memory_enforcement.get(entry["runtime"])
        if expected_enforcement is None:
            failures.append(f"process runtime is unknown: {entry.get('path')}")
        elif entry["memory_enforcement"] != expected_enforcement:
            failures.append(f"process memory enforcement is unmanaged: {entry.get('path')}")
        if entry["profile"] == "benchmark-search" and not entry.get("explicit_timeout"):
            failures.append(f"benchmark process lacks an explicit timeout: {entry.get('path')}")
        if entry["profile"] == "local-service" and not entry.get("explicit_lease"):
            failures.append(f"local service lacks an explicit lease: {entry.get('path')}")
        if int(entry["termination_grace"]) != int(
            profiles.get(entry["profile"], {}).get("termination_grace_seconds", -1)
        ):
            failures.append(f"process termination grace differs from its profile: {entry.get('path')}")
        if int(entry["output_limit"]) > int(
            profiles.get(entry["profile"], {}).get("output_limit_bytes", -1)
        ) or int(entry["output_limit"]) <= 0:
            failures.append(f"process output limit is invalid: {entry.get('path')}")
        if entry["oom_policy"] != "fail-no-retry":
            failures.append(f"process OOM policy permits retry: {entry.get('path')}")
    for path in repository_policy_files():
        if not path.is_file() or is_secret_path(path, policy):
            continue
        if raw_wsl_invocation(path):
            failures.append(
                "raw WSL invocation outside runtime supervisor: "
                + path.relative_to(ROOT).as_posix()
            )
        if path.suffix.lower() in {".py", ".mjs", ".js", ".ts", ".mts", ".ps1", ".sh"}:
            material = path.read_text(encoding="utf-8", errors="replace")
            if re.search(r"(?i)\bwsl(?:\.exe)?\s+--shutdown\b", material):
                failures.append(
                    "global WSL shutdown is prohibited: " + path.relative_to(ROOT).as_posix()
                )
    runtime_policy = policy.get("runtime_policy", {})
    expected_memory_routing = {
        "control": ("windows-commit-control", "control"),
        "build-test": ("physical", "memory-intensive"),
        "benchmark-search": ("physical", "memory-intensive"),
        "local-service": ("physical", "local-service"),
        "cloud-job": ("physical", "cloud-job"),
    }
    for profile, expected in expected_memory_routing.items():
        contract = profiles.get(profile, {})
        if (
            contract.get("admission_basis"),
            contract.get("concurrency_class"),
        ) != expected:
            failures.append(f"resource profile has invalid memory routing: {profile}")
    if runtime_policy.get("windows_commit_control") != {
        "minimum_physical_reserve_mib": 1024,
        "physical_reserve_fraction": 0.0625,
        "minimum_commit_reserve_mib": 4096,
        "commit_reserve_fraction": 0.125,
    }:
        failures.append("Windows control admission does not preserve RAM and commit reserves")
    if runtime_policy.get("parallel_admission") != {
        "stale_slot_grace_seconds": 30,
        "classes": {
            "control": {"maximum_parallel": 2},
            "memory-intensive": {"maximum_parallel": 1},
            "local-service": {"maximum_parallel": 1},
            "cloud-job": {"maximum_parallel": 1},
        },
    }:
        failures.append("parallel runtime admission classes differ from the bounded contract")
    if runtime_policy.get("management_command_profiles") != {
        "storage": {
            "audit": "control",
            "verify": "control",
            "clean": "control",
        },
        "toolchain": {"check": "control", "sync": "build-test"},
        "deps": {
            "import-lock": "build-test",
            "install": "build-test",
            "update": "build-test",
            "verify": "build-test",
        },
        "package": {"pack": "build-test", "publish": "build-test"},
        "git": {
            "inventory": "build-test",
            "converge": "build-test",
            "review": "control",
            "upload": "control",
            "check": "control",
            "promote": "build-test",
            "protect": "control",
            "finalize": "build-test",
        },
    }:
        failures.append("management subcommands do not have exact runtime profiles")
    if runtime_policy.get("normal_path", {}).get("memory_high"):
        failures.append("MemoryHigh throttling is prohibited on the normal path")
    if runtime_policy.get("normal_path", {}).get("rss_polling"):
        failures.append("production RSS polling is prohibited on the normal path")
    if runtime_policy.get("browser_wasm") != {
        "idle_worker_memory_ceiling_bytes": 536870912,
        "retain_prewarm_within_ceiling": True,
        "discard_over_ceiling_at_idle_transfer_or_next_run": True,
    }:
        failures.append("browser WASM idle-memory lifecycle contract is incomplete")
    if runtime_policy.get("worker_policy", {}).get("allow_silent_reduction"):
        failures.append("silent worker reduction is prohibited")
    if runtime_policy.get("worker_policy", {}).get("allow_oom_retry"):
        failures.append("automatic OOM retry is prohibited")
    if runtime_policy.get("hard_containment_context_env") != {
        "release": "CLEARRA_RELEASE",
        "deployment": "CLEARRA_DEPLOYMENT",
    }:
        failures.append("release and deployment hard-containment contexts are incomplete")
    if runtime_policy.get("producer_hard_containment_contexts") != {
        "release-evidence": "release"
    }:
        failures.append("release producer hard-containment routing is incomplete")
    if runtime_policy.get("management_hard_containment_contexts") != {
        "package.publish.apply": "release",
        "git.promote": "release",
        "git.protect.apply": "release",
    }:
        failures.append("management release hard-containment routing is incomplete")
    tree_contract = runtime_policy.get("process_tree_contract", {})
    if (
        tree_contract.get("windows_job_kill_on_close") is not True
        or tree_contract.get("windows_aggregate_memory_limit") is not True
        or tree_contract.get("windows_active_process_limit") is not True
        or tree_contract.get("posix_parent_lease_wrapper")
        != "scripts/management/clearra_process_wrapper.py"
        or tree_contract.get("linux_cgroup_oom_group") is not True
        or tree_contract.get("wsl_terminate_distribution") != "Clearra-Build"
        or tree_contract.get("wsl_guest_parent_loss_poweroff") is not True
        or tree_contract.get("wsl_global_shutdown_allowed") is not False
    ):
        failures.append("cross-platform process-tree termination contract is incomplete")
    cleanup = runtime_policy.get("cleanup_admission", {})
    control_minimum = profiles.get("control", {}).get("minimum_memory_mib")
    if (
        cleanup.get("profile") != "control"
        or cleanup.get("hard_limit_mib") != control_minimum
        or int(cleanup.get("maximum_timeout_seconds", 0)) > 60
        or set(cleanup.get("allowed_operations", []))
        != {
            "wsl-list-running",
            "wsl-terminate-dedicated",
            "wsl-unregister-owned-partial-import",
        }
    ):
        failures.append("WSL cleanup admission is not narrowly bounded")
    if runtime_policy.get("local_ports") != {
        "4194": {
            "role": "local-product-test",
            "profiles": ["local-service"],
            "persistent": True,
        },
        "4195": {
            "role": "local-benchmark-ab",
            "profiles": ["local-service", "benchmark-search"],
            "persistent": False,
            "strict_port": True,
        },
        "8790": {
            "role": "discord-bot-management",
            "profiles": ["local-service"],
            "persistent": True,
            "transport": "ssh-local-forward",
        },
    }:
        failures.append("local product, benchmark, and Discord management port roles differ")
    wsl = runtime_policy.get("wsl", {})
    if (
        wsl.get("distribution") != "Clearra-Build"
        or wsl.get("global_config_management") != "read-only"
        or wsl.get("allow_shutdown") is not False
        or wsl.get("terminate_only_dedicated_distribution") is not True
    ):
        failures.append("dedicated WSL ownership policy is incomplete")
    bootstrap = wsl.get("bootstrap", {})
    if (
        bootstrap.get("source_must_be_stopped") is not True
        or bootstrap.get("source_storage_metadata_must_remain_unchanged") is not True
        or bootstrap.get("first_boot_systemd") is not False
        or bootstrap.get("archive_metadata_injected_before_import") is not True
        or bootstrap.get("sanitizer") != "scripts/runtime/clearra-wsl-sanitize.sh"
        or bootstrap.get("sanitizer_profile") != "build-test"
        or bootstrap.get("sanitizer_memory_enforcement") != "wsl-cgroup-v2"
        or bootstrap.get("temporary_export_retained") is not False
        or bootstrap.get("source_credentials_copied") is not False
    ):
        failures.append("dedicated WSL bootstrap policy is incomplete")
    for entry, contract in wsl.get("entrypoints", {}).items():
        profile = contract.get("profile")
        if profile not in profiles or not isinstance(contract.get("requires_source"), bool):
            failures.append(f"invalid WSL guest entrypoint contract: {entry}")
        if profiles.get(profile, {}).get("explicit_timeout_required") and not contract.get(
            "timeout_seconds"
        ):
            failures.append(f"WSL benchmark entrypoint lacks an explicit timeout: {entry}")
    direct_owner = runtime_policy.get("direct_wsl_owner")
    if direct_owner != "scripts/management/clearra_runtime.py":
        failures.append("direct WSL owner is not the runtime supervisor")
    else:
        owner_path = ROOT / direct_owner
        if not owner_path.is_file() or not re.search(
            r"shutil\.which\(['\"]wsl\.exe['\"]\)",
            owner_path.read_text(encoding="utf-8", errors="replace") if owner_path.is_file() else "",
        ):
            failures.append("runtime supervisor does not own the concrete WSL launcher")
    return failures


def workflow_job_blocks(material: str) -> list[tuple[str, str]]:
    lines = material.splitlines(keepends=True)
    try:
        start = next(index for index, line in enumerate(lines) if line.strip() == "jobs:") + 1
    except StopIteration:
        return []
    blocks: list[tuple[str, str]] = []
    name: str | None = None
    body: list[str] = []
    for line in lines[start:]:
        match = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
        if match:
            if name is not None:
                blocks.append((name, "".join(body)))
            name = match.group(1)
            body = [line]
        elif name is not None:
            body.append(line)
    if name is not None:
        blocks.append((name, "".join(body)))
    return blocks


def workflow_policy_failures(policy: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    node = policy["toolchains"]["node"]
    pnpm = policy["toolchains"]["pnpm"]
    exact_key = (
        "clearra-pnpm-${{ runner.os }}-${{ runner.arch }}-"
        f"node-{node}-pnpm-{pnpm}-${{{{ hashFiles('pnpm-lock.yaml') }}}}"
    )
    for path in sorted((ROOT / ".github" / "workflows").glob("*.y*ml")):
        relative = path.relative_to(ROOT).as_posix()
        material = path.read_text(encoding="utf-8", errors="replace")
        if "pnpm/action-setup" in material or re.search(r"^\s*cache:\s*pnpm\s*$", material, re.M):
            failures.append(f"workflow bypasses Corepack or uses implicit cache keys: {relative}")
        for found in re.findall(r"^\s*node-version:\s*['\"]?([^'\"\s]+)", material, re.M):
            if found != node:
                failures.append(f"floating or mismatched Node version in {relative}: {found}")
        for job, block in workflow_job_blocks(material):
            if "pnpm install" not in block:
                continue
            activation = block.find(f"corepack prepare pnpm@{pnpm} --activate")
            installation = block.find("pnpm install")
            if activation < 0 or activation > installation:
                failures.append(f"pnpm install precedes exact Corepack activation: {relative}:{job}")
            if "steps.pnpm-store.outputs.path" in block and exact_key not in block:
                failures.append(f"pnpm store cache key is incomplete: {relative}:{job}")
            if re.search(r"^\s*path:.*node_modules", block, re.M):
                failures.append(f"node_modules cache is prohibited: {relative}:{job}")
    return failures


def container_policy_failures(policy: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    allowed = {
        policy["container_images"]["node_22_23_2_bookworm_slim"],
        policy["container_images"]["rust_1_98_1_bookworm"],
    }
    image_pattern = re.compile(
        r"^(?:FROM|\s*(?:name|container):)\s*((?:node|rust):[^\s]+)", re.M
    )
    for path in repository_policy_files():
        relative = path.relative_to(ROOT).as_posix()
        if not (
            path.name.startswith("Dockerfile")
            or path.suffix.lower() in {".yml", ".yaml"}
        ):
            continue
        if not path.is_file() or is_secret_path(path, policy):
            continue
        material = path.read_text(encoding="utf-8", errors="replace")
        for image in image_pattern.findall(material):
            if image not in allowed:
                failures.append(f"unapproved floating Node/Rust container image: {relative} ({image})")
    return failures


def verify_static_management_policy(policy: dict[str, Any]) -> None:
    writers = policy.get("writer_registry", [])
    registered = {entry["path"] for entry in writers}
    producers = {entry["id"]: set(entry["classes"]) for entry in policy["producers"]}
    actual = {
        path.relative_to(ROOT).as_posix()
        for path in repository_policy_files()
        if path.is_file() and not is_secret_path(path, policy) and writer_candidate(path)
    }
    process_registered = {entry["path"] for entry in policy.get("process_registry", [])}
    process_actual = {
        path.relative_to(ROOT).as_posix()
        for path in repository_policy_files()
        if path.is_file() and not is_secret_path(path, policy) and process_candidate(path)
    }
    failures = dependency_policy_failures(policy)
    failures.extend(workflow_policy_failures(policy))
    failures.extend(container_policy_failures(policy))
    failures.extend(runtime_policy_failures(policy))
    failures.extend(f"unregistered writer: {path}" for path in sorted(actual - registered))
    failures.extend(f"stale writer registration: {path}" for path in sorted(registered - actual))
    failures.extend(
        f"unregistered process execution point: {path}"
        for path in sorted(process_actual - process_registered)
    )
    failures.extend(
        f"stale process registration: {path}"
        for path in sorted(process_registered - process_actual)
    )
    for entry in writers:
        producer = entry.get("producer")
        output_class = entry.get("output_class")
        if producer not in producers:
            failures.append(f"writer has unknown producer: {entry.get('path')} ({producer})")
        elif output_class not in producers[producer]:
            failures.append(
                f"writer output class is not owned by producer: {entry.get('path')} "
                f"({producer}/{output_class})"
            )
    for entry in policy.get("process_registry", []):
        if entry.get("producer") not in producers:
            failures.append(
                f"process execution point has unknown producer: {entry.get('path')} "
                f"({entry.get('producer')})"
            )
    if failures:
        raise ManagementError("static management policy verification failed:\n" + "\n".join(failures))


def deps_command(
    action: str,
    policy: dict[str, Any],
    *,
    clean_links: bool = False,
    update_manager: str | None = None,
    update_arguments: Sequence[str] = (),
) -> int:
    verify_static_management_policy(policy)
    require_pnpm(policy)
    if action == "import-lock":
        run(("pnpm", "import"))
        return 0
    if action == "install":
        cleaned: list[str] = []
        if clean_links:
            allowed = {normalized(ROOT / entry["path"]) for entry in policy["repository_roots"]}
            for link_root in workspace_node_modules():
                if normalized(link_root) not in allowed:
                    raise ManagementError(
                        f"pnpm link root is not registered in the management policy: {link_root}"
                    )
                if link_root.exists() or is_reparse_point(link_root):
                    remove_pnpm_link_tree(link_root)
                    cleaned.append(str(link_root))
        store_result = run(("pnpm", "store", "path", "--silent"), check=False)
        if store_result.returncode != 0 or not store_result.stdout.strip():
            raise ManagementError("unable to resolve the pnpm shared store")
        store = pathlib.Path(store_result.stdout.strip())
        before = tree_identity_snapshot(store, max_depth=1)
        completed = run(
            ("pnpm", "install", "--frozen-lockfile", "--ignore-scripts"),
            check=False,
        )
        after = tree_identity_snapshot(store, max_depth=1)
        receipt = write_receipt(
            "dependency-install",
            {
                "package_manager": f"pnpm@{policy['toolchains']['pnpm']}",
                "command": ["pnpm", "install", "--frozen-lockfile", "--ignore-scripts"],
                "exit_code": completed.returncode,
                "shared_store_changes": tree_identity_delta(store, before, after),
                "managed_paths": [str(path) for path in workspace_node_modules()],
                "cleaned_link_roots": cleaned,
            },
        )
        if completed.stdout:
            print(completed.stdout, end="")
        if completed.stderr:
            print(completed.stderr, file=sys.stderr, end="")
        print(f"receipt={receipt}")
        return completed.returncode
    if action == "update":
        if update_manager is None:
            raise ManagementError("dependency update requires an explicit manager")
        return dependency_update(update_manager, update_arguments, policy)
    if action != "verify":
        raise ManagementError(f"unsupported dependency action: {action}")
    failures: list[str] = []
    if (ROOT / "package-lock.json").exists():
        failures.append("package-lock.json remains")
    if not (ROOT / "pnpm-lock.yaml").is_file():
        failures.append("pnpm-lock.yaml is missing")
    parity = run((sys.executable, "-B", "scripts/tools/verify-pnpm-import.py"), check=False)
    if parity.returncode != 0:
        failures.append("pnpm import parity receipt or current lock verification failed")
    failures.extend(dependency_policy_failures(policy))
    if failures:
        raise ManagementError("dependency policy verification failed:\n" + "\n".join(sorted(set(failures))))
    print("dependency_policy=accepted")
    return 0


def require_clean_source(operation: str) -> None:
    status = git("status", "--porcelain=v1", "-z", "--untracked-files=normal")
    if status:
        raise ManagementError(f"{operation} requires a clean source worktree")


def dependency_authority_paths(manager: str) -> list[pathlib.Path]:
    tracked = [pathlib.Path(value) for value in git("ls-files", "-z").split("\0") if value]
    if manager == "pnpm":
        selected = [
            relative
            for relative in tracked
            if relative.as_posix() in {"package.json", "pnpm-lock.yaml", "pnpm-workspace.yaml"}
            or (
                relative.name == "package.json"
                and relative.parts
                and relative.parts[0] in {"apps", "packages"}
            )
        ]
    elif manager == "cargo":
        selected = [
            relative
            for relative in tracked
            if relative.name in {"Cargo.toml", "Cargo.lock"}
        ]
    else:
        raise ManagementError(f"unsupported dependency update manager: {manager}")
    return [ROOT / relative for relative in sorted(selected)]


def dependency_graph_snapshot(manager: str, policy: dict[str, Any]) -> dict[str, Any]:
    if manager == "pnpm":
        command = (
            "pnpm",
            "list",
            "--recursive",
            "--depth",
            "Infinity",
            "--json",
            "--lockfile-only",
        )
    elif manager == "cargo":
        command = (
            "cargo",
            f"+{policy['toolchains']['rust']}",
            "metadata",
            "--format-version",
            "1",
            "--locked",
        )
    else:
        raise ManagementError(f"unsupported dependency graph manager: {manager}")
    result = run(command, check=False)
    material = result.stdout.strip()
    packages: set[str] = set()
    parse_error: str | None = None
    if result.returncode == 0 and material:
        try:
            parsed = json.loads(material)
            if manager == "cargo":
                values = parsed.get("packages", []) if isinstance(parsed, dict) else []
                packages.update(
                    f"{item.get('name')}@{item.get('version')}#{item.get('source') or 'workspace'}"
                    for item in values
                )
            else:
                stack = list(parsed if isinstance(parsed, list) else [parsed])
                while stack:
                    item = stack.pop()
                    if not isinstance(item, dict):
                        continue
                    name = item.get("name")
                    version = item.get("version")
                    if name and version:
                        packages.add(f"{name}@{version}")
                    for key in ("dependencies", "devDependencies", "optionalDependencies"):
                        children = item.get(key, {})
                        if isinstance(children, dict):
                            for child_name, child in children.items():
                                if isinstance(child, dict):
                                    child = {"name": child_name, **child}
                                    stack.append(child)
                                elif child:
                                    packages.add(f"{child_name}@{child}")
        except (TypeError, ValueError) as error:
            parse_error = type(error).__name__
    return {
        "command": list(command),
        "exit_code": result.returncode,
        "stdout_sha256": sha256_bytes(material.encode("utf-8")),
        "package_count": len(packages),
        "packages": sorted(packages),
        "parse_error": parse_error,
    }


def dependency_authority_snapshot(
    manager: str, policy: dict[str, Any]
) -> dict[str, Any]:
    files = []
    authority_paths = dependency_authority_paths(manager)
    for path in authority_paths:
        if path.is_file():
            files.append(
                {
                    "path": path.relative_to(ROOT).as_posix(),
                    "bytes": path.stat().st_size,
                    "sha256": sha256_file(path),
                }
            )
    integrity_values: list[str] = []
    for path in authority_paths:
        if not path.is_file() or path.name not in {"pnpm-lock.yaml", "Cargo.lock"}:
            continue
        material = path.read_text(encoding="utf-8", errors="strict")
        if manager == "pnpm":
            integrity_values.extend(
                match.strip("'\"")
                for match in re.findall(r"\bintegrity:\s*([^\s,}]+)", material)
            )
        else:
            integrity_values.extend(
                match
                for match in re.findall(r'^checksum\s*=\s*"([0-9a-f]+)"\s*$', material, re.M)
            )
    sorted_integrity = sorted(integrity_values)
    return {
        "files": files,
        "graph": dependency_graph_snapshot(manager, policy),
        "integrity": {
            "count": len(sorted_integrity),
            "sha256": sha256_bytes("\n".join(sorted_integrity).encode("utf-8")),
            "values": sorted_integrity,
        },
    }


def validate_dependency_update_arguments(manager: str, arguments: Sequence[str]) -> list[str]:
    values = list(arguments)
    if values and values[0] == "--":
        values = values[1:]
    for value in values:
        lowered = value.casefold()
        if re.search(r"(?:auth|token|secret|password|credential|api[-_]?key|otp)", lowered):
            raise ManagementError("dependency update arguments contain prohibited credential material")
        if re.search(r"https?://[^/\s]+@", value, re.I):
            raise ManagementError("dependency update arguments contain URL credentials")
    if manager == "pnpm":
        prohibited = (
            "--dir",
            "-c",
            "--global",
            "-g",
            "--lockfile-dir",
            "--store-dir",
            "--virtual-store-dir",
            "--global-dir",
            "--state-dir",
            "--config-dir",
            "--config",
            "--registry",
            "--userconfig",
            "--globalconfig",
        )
    elif manager == "cargo":
        prohibited = (
            "--manifest-path",
            "--config",
            "--target-dir",
            "--root",
        )
    else:
        raise ManagementError(f"unsupported dependency update manager: {manager}")
    for value in values:
        lowered = value.casefold()
        if any(lowered == option or lowered.startswith(option + "=") for option in prohibited):
            raise ManagementError(f"dependency update cannot override managed paths: {value}")
    return values


def dependency_changed_paths() -> list[str]:
    changed = {
        value
        for value in git("diff", "--name-only", "-z", "HEAD").split("\0")
        if value
    }
    changed.update(
        value
        for value in git("ls-files", "--others", "--exclude-standard", "-z").split("\0")
        if value
    )
    return sorted(changed)


def dependency_update(
    manager: str, arguments: Sequence[str], policy: dict[str, Any]
) -> int:
    require_clean_source("dependency update")
    values = validate_dependency_update_arguments(manager, arguments)
    before = dependency_authority_snapshot(manager, policy)
    if (
        before["graph"]["exit_code"] != 0
        or before["graph"]["parse_error"] is not None
    ):
        raise ManagementError("cannot capture the pre-update dependency graph")
    if manager == "pnpm":
        require_pnpm(policy)
        store_result = run(("pnpm", "store", "path", "--silent"), check=False)
        if store_result.returncode != 0 or not store_result.stdout.strip():
            raise ManagementError("unable to resolve the pnpm shared store")
        shared_root = pathlib.Path(store_result.stdout.strip())
        command = ("pnpm", "update", "--lockfile-only", "--ignore-scripts", *values)
        environment = None
    else:
        shared_root = pathlib.Path(
            os.environ.get("CARGO_HOME", str(pathlib.Path.home() / ".cargo"))
        )
        command = (
            "cargo",
            f"+{policy['toolchains']['rust']}",
            "update",
            *values,
        )
        environment = os.environ.copy()
        environment["CARGO_TARGET_DIR"] = str(
            ROOT / "build" / "cargo" / "host" / "dependency-update"
        )
    shared_before = tree_identity_snapshot(shared_root, max_depth=1)
    completed = run(command, check=False, env=environment)
    shared_after = tree_identity_snapshot(shared_root, max_depth=1)
    after = dependency_authority_snapshot(manager, policy)
    changed = dependency_changed_paths()
    if manager == "pnpm":
        unexpected = [
            path
            for path in changed
            if path != "pnpm-lock.yaml"
            and not (
                path.endswith("/package.json")
                and path.split("/", 1)[0] in {"apps", "packages"}
            )
            and path != "package.json"
        ]
    else:
        unexpected = [path for path in changed if not path.endswith("Cargo.lock")]
    receipt = write_receipt(
        "dependency-update",
        {
            "manager": manager,
            "command": list(command),
            "exit_code": completed.returncode,
            "before": before,
            "after": after,
            "changed_paths": changed,
            "unexpected_paths": unexpected,
            "shared_store_changes": tree_identity_delta(
                shared_root, shared_before, shared_after
            ),
        },
    )
    print(f"receipt={receipt}")
    if unexpected:
        raise ManagementError(
            "dependency update changed files outside its authority; "
            f"receipt={receipt}"
        )
    if (
        after["graph"]["exit_code"] != 0
        or after["graph"]["parse_error"] is not None
    ):
        raise ManagementError(
            f"updated dependency graph is invalid; receipt={receipt}"
        )
    return completed.returncode


def workspace_package_manifest(package_name: str, policy: dict[str, Any]) -> tuple[pathlib.Path, dict[str, Any]]:
    allowed = set(policy.get("package_policy", {}).get("publishable_packages", []))
    if package_name not in allowed:
        raise ManagementError(f"package is not registered for publication: {package_name}")
    matches: list[tuple[pathlib.Path, dict[str, Any]]] = []
    for path in dependency_authority_paths("pnpm"):
        if path.name != "package.json" or not path.is_file():
            continue
        value = json.loads(path.read_text(encoding="utf-8"))
        if value.get("name") == package_name:
            matches.append((path, value))
    if len(matches) != 1:
        raise ManagementError("publishable package must resolve to exactly one workspace")
    path, manifest = matches[0]
    if manifest.get("private") is True or not manifest.get("version"):
        raise ManagementError("publishable package must be non-private and versioned")
    return path, manifest


def inspect_package_tarball(
    tarball: pathlib.Path, expected_name: str, expected_version: str
) -> dict[str, Any]:
    if not tarball.is_file() or is_reparse_point(tarball):
        raise ManagementError("package tarball must be a regular managed file")
    members: list[dict[str, Any]] = []
    packed_manifest: dict[str, Any] | None = None
    member_names: set[str] = set()
    total_bytes = 0
    try:
        with tarfile.open(tarball, "r:gz") as archive:
            archived_members = archive.getmembers()
            if len(archived_members) > 10_000:
                raise ManagementError("package tarball contains too many members")
            for member in archived_members:
                pure = pathlib.PurePosixPath(member.name)
                if (
                    pure.is_absolute()
                    or ".." in pure.parts
                    or not pure.parts
                    or pure.parts[0] != "package"
                    or member.issym()
                    or member.islnk()
                    or member.isdev()
                ):
                    raise ManagementError(f"package tarball contains an unsafe member: {member.name}")
                normalized_name = pure.as_posix()
                if normalized_name in member_names:
                    raise ManagementError(
                        f"package tarball contains a duplicate member: {normalized_name}"
                    )
                member_names.add(normalized_name)
                total_bytes += member.size
                if total_bytes > 512 * 1024 * 1024:
                    raise ManagementError("package tarball expands beyond the managed size limit")
                entry: dict[str, Any] = {
                    "path": normalized_name,
                    "bytes": member.size,
                    "type": "file" if member.isfile() else "directory",
                }
                if member.isfile():
                    stream = archive.extractfile(member)
                    if stream is None:
                        raise ManagementError(f"package tar member is unreadable: {member.name}")
                    content = stream.read()
                    entry["sha256"] = sha256_bytes(content)
                    if pure.as_posix() == "package/package.json":
                        packed_manifest = json.loads(content.decode("utf-8"))
                elif not member.isdir():
                    raise ManagementError(f"package tarball contains an unsupported member: {member.name}")
                members.append(entry)
    except (tarfile.TarError, UnicodeError, ValueError, json.JSONDecodeError) as error:
        raise ManagementError(f"invalid package tarball: {type(error).__name__}") from error
    if packed_manifest is None:
        raise ManagementError("package tarball has no package/package.json")
    if (
        packed_manifest.get("name") != expected_name
        or packed_manifest.get("version") != expected_version
        or packed_manifest.get("private") is True
    ):
        raise ManagementError("packed package identity does not match the requested workspace")
    return {
        "path": str(tarball),
        "bytes": tarball.stat().st_size,
        "sha256": sha256_file(tarball),
        "name": expected_name,
        "version": expected_version,
        "members": sorted(members, key=lambda item: item["path"]),
    }


def package_pack(package_name: str, policy: dict[str, Any]) -> dict[str, Any]:
    verify_static_management_policy(policy)
    require_pnpm(policy)
    require_clean_source("package pack")
    manifest_path, manifest = workspace_package_manifest(package_name, policy)
    version = str(manifest["version"])
    slug = re.sub(r"[^A-Za-z0-9._-]+", "-", package_name).strip("-")
    run_id = utc_stamp() + "-" + uuid.uuid4().hex[:12]
    stage = ROOT / "build" / "package" / slug / run_id
    assert_output_path(stage, policy)
    stage.mkdir(parents=True, exist_ok=False)
    tarball = stage / f"{slug}-{version}.tgz"
    completed = run(
        (
            "pnpm",
            "--filter",
            package_name,
            "pack",
            "--out",
            str(tarball),
            "--json",
        ),
        check=False,
    )
    if completed.returncode != 0:
        receipt = write_receipt(
            "package-pack-failed",
            {
                "package": package_name,
                "version": version,
                "manifest": manifest_path.relative_to(ROOT).as_posix(),
                "stage": str(stage),
                "exit_code": completed.returncode,
            },
        )
        raise ManagementError(f"pnpm pack failed; receipt={receipt}")
    require_clean_source("package pack lifecycle")
    inspection = inspect_package_tarball(tarball, package_name, version)
    payload = {
        "source_sha": git("rev-parse", "HEAD"),
        "source_tree": git("rev-parse", "HEAD^{tree}"),
        "package": package_name,
        "version": version,
        "manifest": manifest_path.relative_to(ROOT).as_posix(),
        "tarball": inspection,
        "owned_paths": [str(stage)],
    }
    receipt = write_receipt("package-pack", payload)
    return {**payload, "receipt": str(receipt)}


def load_package_pack_receipt(
    value: str, policy: dict[str, Any]
) -> tuple[pathlib.Path, dict[str, Any], dict[str, Any]]:
    path = pathlib.Path(value).resolve(strict=True)
    root = state_root() / "receipts"
    if not is_within(path, root) or is_secret_path(path, policy):
        raise ManagementError("package publication requires a managed pack receipt")
    assert_no_link_escape(path, root)
    receipt = json.loads(path.read_text(encoding="utf-8"))
    if receipt.get("schema_id") != "clearra.management-receipt.v1" or receipt.get("kind") != "package-pack":
        raise ManagementError("unsupported package pack receipt")
    tarball_record = receipt.get("tarball", {})
    tarball = pathlib.Path(str(tarball_record.get("path") or "")).resolve(strict=True)
    build_root = (ROOT / "build").resolve(strict=False)
    if not is_within(tarball, build_root):
        raise ManagementError("package tarball escaped the managed build root")
    assert_no_link_escape(tarball, build_root)
    current = inspect_package_tarball(
        tarball, str(receipt.get("package") or ""), str(receipt.get("version") or "")
    )
    if current != tarball_record:
        raise ManagementError("package tarball no longer matches its pack receipt")
    return path, receipt, current


def package_publish(
    receipt_value: str,
    policy: dict[str, Any],
    *,
    tag: str,
    access: str,
    apply: bool,
) -> dict[str, Any]:
    verify_static_management_policy(policy)
    require_pnpm(policy)
    receipt_path, receipt, tarball = load_package_pack_receipt(receipt_value, policy)
    require_clean_source("package publication")
    current_sha = git("rev-parse", "HEAD")
    current_tree = git("rev-parse", "HEAD^{tree}")
    if current_sha != receipt.get("source_sha") or current_tree != receipt.get("source_tree"):
        raise ManagementError("package source no longer matches the packed source identity")
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,127}", tag):
        raise ManagementError("invalid npm distribution tag")
    if access not in {"public", "restricted"}:
        raise ManagementError("invalid npm package access")
    command = [
        "npm",
        "publish",
        tarball["path"],
        "--provenance",
        "--ignore-scripts",
        "--tag",
        tag,
        "--access",
        access,
    ]
    plan = {
        "package": receipt["package"],
        "version": receipt["version"],
        "source_sha": current_sha,
        "source_tree": current_tree,
        "pack_receipt": str(receipt_path),
        "tarball_sha256": tarball["sha256"],
        "tarball_bytes": tarball["bytes"],
        "command": command,
        "apply": apply,
    }
    if not apply:
        return plan
    npm_version = run(("npm", "--version"), check=False).stdout.strip()
    if npm_version != policy["toolchains"]["npm"]:
        raise ManagementError(
            f"npm {policy['toolchains']['npm']} is required; found {npm_version or 'unavailable'}"
        )
    identity = run(("npm", "whoami"), check=False)
    if identity.returncode != 0 or not identity.stdout.strip():
        raise ManagementError("npm registry authentication is unavailable")
    completed = run(command, check=False)
    publication_receipt = write_receipt(
        "package-publication",
        {
            **plan,
            "npm_identity": identity.stdout.strip(),
            "exit_code": completed.returncode,
        },
    )
    result = {**plan, "exit_code": completed.returncode, "receipt": str(publication_receipt)}
    if completed.returncode != 0:
        raise ManagementError(f"npm publication failed; receipt={publication_receipt}")
    return result


@dataclass
class Worktree:
    path: pathlib.Path
    head: str
    branch: str | None
    detached: bool


def parse_worktrees(material: str) -> list[Worktree]:
    records = []
    for block in material.strip().split("\n\n"):
        values: dict[str, str] = {}
        flags: set[str] = set()
        for line in block.splitlines():
            if " " in line:
                key, value = line.split(" ", 1)
                values[key] = value
            elif line:
                flags.add(line)
        if "worktree" in values:
            records.append(
                Worktree(
                    pathlib.Path(values["worktree"]),
                    values.get("HEAD", ""),
                    values.get("branch"),
                    "detached" in flags,
                )
            )
    return records


def worktree_status(worktree: Worktree, policy: dict[str, Any]) -> dict[str, Any]:
    result = run(("git", "status", "--porcelain=v1", "-z", "--untracked-files=normal"), cwd=worktree.path)
    entries = [entry for entry in result.stdout.split("\0") if entry]
    secret_blocker_count = 0
    for entry in entries:
        candidate = entry[3:] if len(entry) > 3 else entry
        if is_secret_path(pathlib.Path(candidate), policy):
            secret_blocker_count += 1
    return {
        "path": str(worktree.path),
        "head": worktree.head,
        "branch": worktree.branch,
        "detached": worktree.detached,
        "dirty_entries": len(entries),
        "credential_path_blocker_count": secret_blocker_count,
    }


def classify_ref(ref: str, main: str) -> str:
    commit = run(("git", "rev-parse", f"{ref}^{{commit}}"), check=False)
    if commit.returncode != 0:
        return "non-commit"
    sha = commit.stdout.strip()
    if run(("git", "merge-base", "--is-ancestor", sha, main), check=False).returncode == 0:
        return "ancestor"
    if git("rev-parse", f"{sha}^{{tree}}") == git("rev-parse", f"{main}^{{tree}}"):
        return "tree-identical"
    cherry = run(("git", "cherry", main, sha), check=False)
    lines = [line for line in cherry.stdout.splitlines() if line.strip()]
    if cherry.returncode == 0 and lines and all(line.startswith("-") for line in lines):
        return "patch-equivalent"
    return "unique"


def patch_ids_for_range(
    main: str, sha: str, policy: dict[str, Any]
) -> list[dict[str, Any]]:
    commits = git("rev-list", "--reverse", f"{main}..{sha}").splitlines()
    values: list[dict[str, Any]] = []
    for commit in commits:
        names = git(
            "diff-tree", "--no-commit-id", "--name-only", "-r", "--root", commit
        ).splitlines()
        blocked = sum(1 for name in names if is_secret_path(pathlib.Path(name), policy))
        if blocked:
            values.append(
                {"commit": commit, "patch_id": None, "credential_path_blocker_count": blocked}
            )
            continue
        patch = run(("git", "show", "--pretty=format:", "--binary", commit))
        patch_id = run(("git", "patch-id", "--stable"), input_text=patch.stdout, check=False)
        digest = patch_id.stdout.split()[0] if patch_id.returncode == 0 and patch_id.stdout.split() else "empty"
        values.append({"commit": commit, "patch_id": digest})
    return values


def related_check_runs_once(sha: str) -> dict[str, Any]:
    response = run(
        (
            "gh",
            "api",
            f"repos/{repository_name()}/commits/{sha}/check-runs?per_page=100",
        ),
        check=False,
    )
    if response.returncode != 0:
        return {"available": False, "checks": []}
    try:
        material = json.loads(response.stdout)
    except json.JSONDecodeError:
        return {"available": False, "checks": []}
    checks = [
        {
            "name": item.get("name"),
            "status": item.get("status"),
            "conclusion": item.get("conclusion"),
            "url": item.get("html_url"),
            "head_sha": item.get("head_sha"),
        }
        for item in material.get("check_runs", [])
    ]
    return {"available": True, "checks": checks}


def git_inventory(policy: dict[str, Any], *, fetch: bool = False) -> dict[str, Any]:
    remote = policy["git_policy"]["remote"]
    main_ref = f"refs/remotes/{remote}/{policy['git_policy']['canonical_branch']}"
    shallow = git("rev-parse", "--is-shallow-repository") == "true"
    if fetch:
        if shallow:
            git("fetch", "--unshallow", "--tags", remote, f"+refs/heads/*:refs/remotes/{remote}/*")
        else:
            git("fetch", "--tags", remote, f"+refs/heads/*:refs/remotes/{remote}/*")
        shallow = git("rev-parse", "--is-shallow-repository") == "true"
    if shallow:
        raise ManagementError("full history is required before Git classification")
    fsck = run(("git", "fsck", "--full", "--no-reflogs"), check=False)
    if fsck.returncode != 0:
        raise ManagementError("Git object closure is invalid; convergence is blocked")
    refs: list[dict[str, Any]] = []
    evidence_cache: dict[str, dict[str, Any]] = {}
    check_cache: dict[str, dict[str, Any]] = {}
    material = git(
        "for-each-ref",
        "--format=%(refname)%00%(objectname)",
        "refs/heads",
        "refs/remotes",
        "refs/tags",
    )
    for line in material.splitlines():
        if "\0" not in line:
            continue
        ref, object_sha = line.split("\0", 1)
        commit_result = run(("git", "rev-parse", f"{ref}^{{commit}}"), check=False)
        commit_sha = commit_result.stdout.strip() if commit_result.returncode == 0 else None
        classification = "canonical" if ref == main_ref else classify_ref(ref, main_ref)
        entry: dict[str, Any] = {
            "ref": ref,
            "object_sha": object_sha,
            "sha": commit_sha or object_sha,
            "classification": classification,
        }
        if commit_sha:
            if commit_sha not in evidence_cache:
                ahead_behind = git("rev-list", "--left-right", "--count", f"{main_ref}...{commit_sha}").split()
                evidence: dict[str, Any] = {
                    "tree": git("rev-parse", f"{commit_sha}^{{tree}}"),
                    "behind": int(ahead_behind[0]),
                    "ahead": int(ahead_behind[1]),
                }
                if classification == "unique":
                    changed_files = git(
                        "diff", "--name-only", f"{main_ref}...{commit_sha}"
                    ).splitlines()
                    evidence["credential_path_blocker_count"] = sum(
                        1
                        for name in changed_files
                        if is_secret_path(pathlib.Path(name), policy)
                    )
                    evidence["changed_files"] = [
                        name
                        for name in changed_files
                        if not is_secret_path(pathlib.Path(name), policy)
                    ]
                    evidence["patch_ids"] = patch_ids_for_range(main_ref, commit_sha, policy)
                evidence_cache[commit_sha] = evidence
            entry.update(evidence_cache[commit_sha])
            if classification == "unique" and ref.startswith(f"refs/remotes/{remote}/"):
                if commit_sha not in check_cache:
                    check_cache[commit_sha] = related_check_runs_once(commit_sha)
                entry["related_ci"] = check_cache[commit_sha]
        refs.append(entry)
    worktrees = parse_worktrees(git("worktree", "list", "--porcelain"))
    remote_material = run(("git", "ls-remote", "--heads", "--tags", remote), check=False)
    remote_refs = []
    if remote_material.returncode == 0:
        for line in remote_material.stdout.splitlines():
            fields = line.split()
            if len(fields) == 2:
                remote_refs.append({"sha": fields[0], "ref": fields[1]})
    submodule = run(("git", "submodule", "status", "--recursive"), check=False)
    return {
        "schema_id": "clearra.git-inventory.v1",
        "created_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "repository": str(ROOT),
        "common_git_directory": git("rev-parse", "--git-common-dir"),
        "shallow": shallow,
        "canonical_ref": main_ref,
        "canonical_sha": git("rev-parse", main_ref),
        "remote_ls_refs": remote_refs,
        "refs": refs,
        "worktrees": [worktree_status(item, policy) for item in worktrees],
        "submodules": {
            "accepted": submodule.returncode == 0,
            "status": submodule.stdout.splitlines(),
        },
        "fsck_dangling_count": sum(1 for line in (fsck.stdout + fsck.stderr).splitlines() if "dangling " in line),
        "fsck_unreachable_count": sum(1 for line in (fsck.stdout + fsck.stderr).splitlines() if "unreachable " in line),
    }


def process_is_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    if pid == os.getpid():
        return True
    if os.name == "nt":
        import ctypes

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
        handle = kernel32.OpenProcess(0x1000, False, pid)
        if handle:
            kernel32.CloseHandle(handle)
            return True
        return ctypes.get_last_error() == 5
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def repository_lock_owner(path: pathlib.Path) -> dict[str, Any] | None:
    try:
        material = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    pid = material.get("pid") if isinstance(material, dict) else None
    created = material.get("created_utc") if isinstance(material, dict) else None
    if not isinstance(pid, int) or pid <= 0 or not isinstance(created, str) or not created:
        return None
    return {"pid": pid, "created_utc": created}


@contextlib.contextmanager
def repository_lock() -> Iterator[pathlib.Path]:
    identity = hashlib.sha256(normalized(ROOT).encode()).hexdigest()[:24]
    directory = state_root() / "git-locks"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / f"{identity}.lock"
    recovered: dict[str, Any] | None = None
    while True:
        try:
            descriptor = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            break
        except FileExistsError as error:
            owner = repository_lock_owner(path)
            if owner is None or process_is_alive(owner["pid"]):
                raise ManagementError(
                    f"another Clearra Git management operation owns {path}"
                ) from error
            stale = path.with_name(path.name + f".stale-{uuid.uuid4().hex[:12]}")
            try:
                os.replace(path, stale)
            except FileNotFoundError:
                continue
            recovered = owner
            stale.unlink(missing_ok=True)
    try:
        payload: dict[str, Any] = {
            "pid": os.getpid(),
            "created_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        }
        if recovered is not None:
            payload["recovered_stale_owner"] = recovered
        try:
            os.write(descriptor, json.dumps(payload).encode())
        finally:
            os.close(descriptor)
        if recovered is not None:
            write_receipt(
                "git-stale-lock-recovered",
                {
                    "lock": str(path),
                    "stale_owner": recovered,
                    "new_owner_pid": os.getpid(),
                },
            )
        yield path
    finally:
        path.unlink(missing_ok=True)


def safe_ref_component(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", value).strip(".-")[:80] or "ref"


def untracked_paths(worktree: pathlib.Path) -> list[pathlib.Path]:
    result = run(("git", "ls-files", "--others", "--exclude-standard", "-z"), cwd=worktree)
    return [worktree / item for item in result.stdout.split("\0") if item]


def generated_untracked(path: pathlib.Path, worktree: pathlib.Path) -> bool:
    relative = path.relative_to(worktree).as_posix()
    prefixes = (
        "build/", "coverage/", "node_modules/", "dist/", "dist-server/", ".cache/",
        "_local/artifacts/", "_local/state/", "_local/tmp/", "target/",
    )
    return relative.startswith(prefixes)


def prepare_git_safety(policy: dict[str, Any]) -> dict[str, Any]:
    transaction = utc_stamp() + "-" + uuid.uuid4().hex[:12]
    directory = state_root() / "git-safety" / hashlib.sha256(normalized(ROOT).encode()).hexdigest()[:24] / transaction
    directory.mkdir(parents=True, exist_ok=False)
    inventory = git_inventory(policy, fetch=True)
    mapping: list[dict[str, str]] = []
    candidates: set[str] = set()
    for ref in inventory["refs"]:
        candidates.add(ref["sha"])
    for item in inventory["worktrees"]:
        if item["head"]:
            candidates.add(item["head"])
    reflog = run(("git", "reflog", "--all", "--format=%H"), check=False)
    candidates.update(line for line in reflog.stdout.splitlines() if re.fullmatch(r"[0-9a-f]{40}", line))
    fsck = run(("git", "fsck", "--full", "--unreachable", "--no-reflogs"), check=False)
    for line in (fsck.stdout + fsck.stderr).splitlines():
        match = re.search(r"(?:dangling|unreachable) commit ([0-9a-f]{40})", line)
        if match:
            candidates.add(match.group(1))
    prefix = policy["git_policy"]["safety_ref_prefix"].rstrip("/") + "/" + transaction
    for index, sha in enumerate(sorted(candidates)):
        if run(("git", "cat-file", "-e", f"{sha}^{{commit}}"), check=False).returncode != 0:
            continue
        ref = f"{prefix}/{index:05d}-{safe_ref_component(sha[:12])}"
        git("update-ref", ref, sha)
        mapping.append({"ref": ref, "sha": sha})
    (directory / "safety-refs.json").write_text(json.dumps(mapping, indent=2) + "\n", encoding="utf-8")
    bundle = directory / "repository.bundle"
    git("bundle", "create", str(bundle), "--all")
    git("bundle", "verify", str(bundle))
    verification = directory / "independent-clone"
    run(("git", "clone", "--no-checkout", str(bundle), str(verification)), cwd=directory)
    run(("git", "fsck", "--full"), cwd=verification)
    remove_owned_path(verification, policy)
    archive = directory / "worktree-uncommitted.zip"
    archive_manifest: list[dict[str, Any]] = []
    blockers: list[dict[str, Any]] = []
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as output:
        for index, item in enumerate(parse_worktrees(git("worktree", "list", "--porcelain"))):
            prefix_name = f"worktree-{index:03d}"
            for cached, name in ((False, "unstaged.patch"), (True, "staged.patch")):
                name_command = ["git", "diff", "--name-only", "-z"]
                if cached:
                    name_command.append("--cached")
                names = [
                    value
                    for value in run(name_command, cwd=item.path).stdout.split("\0")
                    if value
                ]
                blocked_names = [
                    value for value in names if is_secret_path(pathlib.Path(value), policy)
                ]
                if blocked_names:
                    blockers.append(
                        {
                            "worktree": str(item.path),
                            "classification": "credential-path",
                            "count": len(blocked_names),
                            "area": "staged" if cached else "unstaged",
                        }
                    )
                patch_parts: list[str] = []
                for value in names:
                    if is_secret_path(pathlib.Path(value), policy):
                        continue
                    command = ["git", "diff", "--binary"]
                    if cached:
                        command.append("--cached")
                    command.extend(("--", value))
                    patch_parts.append(run(command, cwd=item.path).stdout)
                material = "".join(patch_parts).encode("utf-8", errors="strict")
                if material:
                    output.writestr(f"{prefix_name}/{name}", material)
            for path in untracked_paths(item.path):
                relative = path.relative_to(item.path)
                if is_secret_path(relative, policy):
                    blockers.append(
                        {
                            "worktree": str(item.path),
                            "classification": "credential-path",
                            "count": 1,
                            "area": "untracked",
                        }
                    )
                    continue
                if generated_untracked(path, item.path):
                    archive_manifest.append({"worktree": str(item.path), "path": relative.as_posix(), "classification": "reproducible-generated"})
                    continue
                try:
                    assert_no_link_escape(path, item.path)
                except ManagementError:
                    blockers.append(
                        {
                            "worktree": str(item.path),
                            "classification": "link-or-reparse",
                            "path": relative.as_posix(),
                        }
                    )
                    continue
                if path.is_file():
                    digest = sha256_file(path)
                    output.write(path, f"{prefix_name}/untracked/{relative.as_posix()}")
                    archive_manifest.append({"worktree": str(item.path), "path": relative.as_posix(), "sha256": digest, "bytes": path.stat().st_size})
    (directory / "untracked-manifest.json").write_text(json.dumps(archive_manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    (directory / "blocked-path-summary.json").write_text(json.dumps(blockers, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    inventory_path = directory / "inventory.json"
    inventory_path.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    review_items: list[dict[str, Any]] = []
    for ref in inventory["refs"]:
        if ref["classification"] == "unique":
            review_items.append(
                {
                    "kind": "ref",
                    "ref": ref["ref"],
                    "sha": ref["sha"],
                    "tree": ref.get("tree"),
                    "decision": "pending",
                    "reason": "",
                }
            )
    for worktree in inventory["worktrees"]:
        if worktree["dirty_entries"]:
            review_items.append(
                {
                    "kind": "dirty-worktree",
                    "worktree": worktree["path"],
                    "head": worktree["head"],
                    "dirty_entries": worktree["dirty_entries"],
                    "decision": "pending",
                    "reason": "",
                }
            )
    review_path = directory / "review-decisions.json"
    review_path.write_text(
        json.dumps(
            {
                "schema_id": "clearra.git-convergence-review.v1",
                "transaction": transaction,
                "allowed_decisions": ["selected", "excluded"],
                "items": review_items,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )
    commit_blockers = sum(
        int(ref.get("credential_path_blocker_count", 0)) for ref in inventory["refs"]
    )
    receipt = {
        "schema_id": "clearra.git-safety.v1",
        "transaction": transaction,
        "bundle": str(bundle),
        "bundle_sha256": sha256_file(bundle),
        "safety_refs": len(mapping),
        "untracked_entries": len(archive_manifest),
        "credential_path_blockers": len(blockers) + commit_blockers,
        "inventory": str(inventory_path),
        "review_decisions": str(review_path),
        "pending_review_items": len(review_items),
        "ready_for_review": len(blockers) + commit_blockers == 0,
    }
    receipt_path = directory / "receipt.json"
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return {**receipt, "receipt": str(receipt_path)}


def repository_name() -> str:
    url = git("remote", "get-url", "origin")
    match = re.search(r"github\.com[/:]([^/]+/[^/.]+)(?:\.git)?$", url)
    if not match:
        raise ManagementError(f"unsupported GitHub origin URL: {url}")
    return match.group(1)


def github_https_clone_url(remote_url: str) -> str:
    """Return a credential-free HTTPS URL for an exact GitHub repository."""
    value = remote_url.strip()
    match = re.fullmatch(
        r"(?:git@github\.com:|ssh://git@github\.com/|https://github\.com/)"
        r"([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(?:\.git)?",
        value,
        flags=re.IGNORECASE,
    )
    if not match or any(part in {".", ".."} for part in match.groups()):
        raise ManagementError("independent checkout requires a credential-free GitHub origin URL")
    owner, repository = match.groups()
    return f"https://github.com/{owner}/{repository}.git"


def candidate_check_status_once(
    candidate: str, policy: dict[str, Any]
) -> tuple[str, list[dict[str, Any]]]:
    pattern = policy["git_policy"]["candidate_pattern"]
    if not fnmatch.fnmatch(candidate, pattern) or run(
        ("git", "check-ref-format", f"refs/heads/{candidate}"), check=False
    ).returncode:
        raise ManagementError(f"candidate branch must match {pattern}")
    remote_ref = f"refs/remotes/origin/{candidate}"
    git("fetch", "origin", f"refs/heads/{candidate}:{remote_ref}")
    sha = git("rev-parse", remote_ref)
    response = run(("gh", "api", f"repos/{repository_name()}/commits/{sha}/check-runs"))
    checks = json.loads(response.stdout).get("check_runs", [])
    required = policy["git_policy"]["required_checks"]
    selected = []
    for name in required:
        matches = [
            check
            for check in checks
            if check.get("name") == name and check.get("head_sha") == sha
        ]
        if not matches:
            selected.append(
                {
                    "id": None,
                    "name": name,
                    "head_sha": sha,
                    "status": "missing",
                    "conclusion": None,
                    "url": None,
                }
            )
            continue
        check = max(matches, key=lambda item: int(item.get("id") or 0))
        selected.append(
            {
                "id": check.get("id"),
                "name": name,
                "head_sha": check.get("head_sha"),
                "status": check.get("status"),
                "conclusion": check.get("conclusion"),
                "url": check.get("html_url"),
            }
        )
    return sha, selected


def check_candidate_once(candidate: str, policy: dict[str, Any]) -> tuple[str, list[dict[str, Any]]]:
    sha, selected = candidate_check_status_once(candidate, policy)
    for check in selected:
        if check["status"] == "missing":
            raise ManagementError(
                f"required exact-SHA check is missing for {sha}: {check['name']}"
            )
        if check["status"] != "completed" or check["conclusion"] != "success":
            raise ManagementError(
                f"required check is not successful for {sha}: {check['name']} "
                f"({check['status']}/{check['conclusion']})"
            )
    return sha, selected


def load_safety_transaction(
    receipt_value: str, policy: dict[str, Any]
) -> tuple[pathlib.Path, dict[str, Any], pathlib.Path, dict[str, Any]]:
    receipt_path = pathlib.Path(receipt_value).resolve(strict=True)
    safety_root = state_root() / "git-safety"
    if not is_within(receipt_path, safety_root) or is_secret_path(receipt_path, policy):
        raise ManagementError("operation requires a managed Git safety receipt")
    assert_no_link_escape(receipt_path, safety_root)
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if receipt.get("schema_id") != "clearra.git-safety.v1":
        raise ManagementError("unsupported Git safety receipt")
    if not receipt.get("ready_for_review") or receipt.get("credential_path_blockers"):
        raise ManagementError("Git safety receipt has unresolved prohibited-path blockers")
    bundle = pathlib.Path(receipt.get("bundle", "")).resolve(strict=True)
    if not is_within(bundle, receipt_path.parent):
        raise ManagementError("Git safety bundle escaped its transaction")
    if sha256_file(bundle) != receipt.get("bundle_sha256"):
        raise ManagementError("Git safety bundle is missing or its digest changed")
    run(("git", "bundle", "verify", str(bundle)))
    review_path = pathlib.Path(receipt.get("review_decisions", "")).resolve(strict=True)
    if not is_within(review_path, receipt_path.parent):
        raise ManagementError("convergence review escaped its safety transaction")
    assert_no_link_escape(review_path, receipt_path.parent)
    review = json.loads(review_path.read_text(encoding="utf-8"))
    if (
        review.get("schema_id") != "clearra.git-convergence-review.v1"
        or review.get("transaction") != receipt.get("transaction")
    ):
        raise ManagementError("convergence review does not match the safety transaction")
    return receipt_path, receipt, review_path, review


def safe_archive_relative(value: str) -> pathlib.PurePosixPath:
    relative = pathlib.PurePosixPath(value)
    if (
        relative.is_absolute()
        or not relative.parts
        or any(part in {"", ".", ".."} for part in relative.parts)
        or "\\" in value
    ):
        raise ManagementError("safety archive contains an unsafe relative path")
    return relative


def validate_dirty_candidate_evidence(
    item: dict[str, Any],
    evidence_reference: Any,
    candidate_sha: str,
    receipt_path: pathlib.Path,
    receipt: dict[str, Any],
    policy: dict[str, Any],
) -> dict[str, Any]:
    if not isinstance(evidence_reference, dict):
        raise ManagementError("selected dirty worktree evidence must be a structured reference")
    if evidence_reference.get("schema_id") != "clearra.git-dirty-evidence-reference.v1":
        raise ManagementError("selected dirty worktree evidence has an unsupported schema")
    evidence_path = pathlib.Path(str(evidence_reference.get("receipt", ""))).resolve(strict=True)
    if not is_within(evidence_path, receipt_path.parent / "evidence"):
        raise ManagementError("dirty worktree evidence escaped its safety transaction")
    assert_no_link_escape(evidence_path, receipt_path.parent)
    if sha256_file(evidence_path) != evidence_reference.get("sha256"):
        raise ManagementError("dirty worktree evidence digest changed")
    evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
    if (
        evidence.get("schema_id") != "clearra.git-dirty-candidate-evidence.v1"
        or evidence.get("transaction") != receipt.get("transaction")
        or evidence.get("source_head") != item.get("head")
        or normalized(pathlib.Path(evidence.get("worktree", "")))
        != normalized(pathlib.Path(item.get("worktree", "")))
    ):
        raise ManagementError("dirty worktree evidence does not match its review item")
    evidence_sha = str(evidence.get("candidate_sha") or "")
    if not re.fullmatch(r"[0-9a-f]{40}", evidence_sha):
        raise ManagementError("dirty worktree evidence has an invalid candidate SHA")
    if run(
        ("git", "merge-base", "--is-ancestor", evidence_sha, candidate_sha),
        check=False,
    ).returncode != 0:
        raise ManagementError("dirty worktree evidence is not contained in the candidate")
    actual_tree = git("rev-parse", f"{evidence_sha}^{{tree}}")
    if (
        evidence.get("candidate_tree") != actual_tree
        or evidence.get("reconstructed_tree") != actual_tree
        or not evidence.get("verified_exact_tree")
    ):
        raise ManagementError("dirty worktree evidence no longer proves exact tree equality")
    archive = receipt_path.parent / "worktree-uncommitted.zip"
    manifest = receipt_path.parent / "untracked-manifest.json"
    if (
        sha256_file(archive) != evidence.get("archive_sha256")
        or sha256_file(manifest) != evidence.get("untracked_manifest_sha256")
    ):
        raise ManagementError("dirty worktree evidence inputs changed")
    return {
        "receipt": str(evidence_path),
        "sha256": evidence_reference["sha256"],
        "candidate_sha": evidence_sha,
        "candidate_tree": actual_tree,
    }


def validate_ref_candidate_evidence(
    item: dict[str, Any],
    evidence_reference: Any,
    candidate_sha: str,
    receipt_path: pathlib.Path,
) -> dict[str, Any]:
    if not isinstance(evidence_reference, dict):
        raise ManagementError("selected ref is neither contained nor backed by replay evidence")
    if evidence_reference.get("schema_id") != "clearra.git-ref-evidence-reference.v1":
        raise ManagementError("selected ref replay evidence has an unsupported schema")
    evidence_path = pathlib.Path(str(evidence_reference.get("receipt", ""))).resolve(strict=True)
    if not is_within(evidence_path, receipt_path.parent / "evidence"):
        raise ManagementError("selected ref replay evidence escaped its safety transaction")
    assert_no_link_escape(evidence_path, receipt_path.parent)
    if sha256_file(evidence_path) != evidence_reference.get("sha256"):
        raise ManagementError("selected ref replay evidence digest changed")
    evidence = json.loads(evidence_path.read_text(encoding="utf-8"))
    source_sha = str(item.get("sha") or "")
    if (
        evidence.get("schema_id") != "clearra.git-ref-replay-evidence.v1"
        or source_sha not in evidence.get("selected_tips", [])
        or evidence_reference.get("source_tip") != source_sha
    ):
        raise ManagementError("selected ref replay evidence does not match its review item")
    initial_sha = str(evidence.get("initial_sha") or "")
    final_sha = str(evidence.get("final_sha") or "")
    if run(
        ("git", "merge-base", "--is-ancestor", final_sha, candidate_sha), check=False
    ).returncode != 0:
        raise ManagementError("selected ref replay result is not contained in the candidate")
    expected = set(
        git("rev-list", source_sha, "--not", initial_sha).splitlines()
    )
    covered = {
        str(entry.get("source"))
        for entry in evidence.get("applied", [])
        if entry.get("classification")
        in {"replayed", "already-contained", "patch-equivalent-empty"}
    }
    if not expected.issubset(covered):
        raise ManagementError("selected ref replay evidence does not cover its source history")
    if evidence.get("final_tree") != git("rev-parse", f"{final_sha}^{{tree}}"):
        raise ManagementError("selected ref replay evidence final tree changed")
    return {
        "receipt": str(evidence_path),
        "sha256": evidence_reference["sha256"],
        "source_tip": source_sha,
        "final_sha": final_sha,
    }


def record_dirty_candidate_evidence(
    receipt_value: str,
    worktree_value: str,
    evidence_candidate: str,
    policy: dict[str, Any],
) -> dict[str, Any]:
    receipt_path, receipt, review_path, review = load_safety_transaction(
        receipt_value, policy
    )
    worktree_path = pathlib.Path(worktree_value).resolve(strict=True)
    matches = [
        item
        for item in review.get("items", [])
        if item.get("kind") == "dirty-worktree"
        and normalized(pathlib.Path(item.get("worktree", ""))) == normalized(worktree_path)
    ]
    if len(matches) != 1:
        raise ManagementError("exactly one dirty worktree review item is required")
    item = matches[0]
    candidate_sha = git("rev-parse", f"{evidence_candidate}^{{commit}}")
    candidate_tree = git("rev-parse", f"{candidate_sha}^{{tree}}")
    inventory_path = receipt_path.parent / "inventory.json"
    archive_path = receipt_path.parent / "worktree-uncommitted.zip"
    manifest_path = receipt_path.parent / "untracked-manifest.json"
    inventory = json.loads(inventory_path.read_text(encoding="utf-8"))
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    indexes = [
        index
        for index, entry in enumerate(inventory.get("worktrees", []))
        if normalized(pathlib.Path(entry.get("path", ""))) == normalized(worktree_path)
    ]
    if len(indexes) != 1:
        raise ManagementError("dirty worktree is missing from the safety inventory")
    index = indexes[0]
    prefix = f"worktree-{index:03d}"
    evidence_directory = receipt_path.parent / "evidence"
    evidence_directory.mkdir(parents=True, exist_ok=True)
    clone_directory = evidence_directory / (
        f"materialize-{index:03d}-{candidate_sha[:12]}-{uuid.uuid4().hex[:8]}"
    )
    patch_digests: dict[str, str] = {}
    archived_untracked: list[dict[str, Any]] = []
    generated_entries = 0
    try:
        run(("git", "clone", "--no-checkout", str(receipt["bundle"]), str(clone_directory)))
        git("config", "core.autocrlf", "false", cwd=clone_directory)
        git("checkout", "--detach", str(item["head"]), cwd=clone_directory)
        with zipfile.ZipFile(archive_path) as archive:
            names = set(archive.namelist())
            for patch_name, indexed in (("staged.patch", True), ("unstaged.patch", False)):
                member = f"{prefix}/{patch_name}"
                if member not in names:
                    continue
                material = archive.read(member)
                patch_digests[patch_name] = sha256_bytes(material)
                command = ["git", "apply", "--binary", "--whitespace=nowarn"]
                if indexed:
                    command.append("--index")
                patch_file = clone_directory / f".clearra-evidence-{patch_name}"
                patch_file.write_bytes(material)
                try:
                    applied = run(
                        [*command, str(patch_file)],
                        cwd=clone_directory,
                        check=False,
                    )
                finally:
                    patch_file.unlink(missing_ok=True)
                if applied.returncode != 0:
                    raise ManagementError(
                        f"cannot reconstruct dirty worktree {patch_name}: "
                        f"{applied.stderr.strip() or applied.stdout.strip()}"
                    )
            for entry in manifest:
                if normalized(pathlib.Path(entry.get("worktree", ""))) != normalized(
                    worktree_path
                ):
                    continue
                if entry.get("classification") == "reproducible-generated":
                    generated_entries += 1
                    continue
                relative = safe_archive_relative(str(entry.get("path", "")))
                if is_secret_path(pathlib.Path(*relative.parts), policy):
                    raise ManagementError("dirty evidence encountered a prohibited path")
                member = f"{prefix}/untracked/{relative.as_posix()}"
                if member not in names:
                    raise ManagementError("dirty evidence archive is missing an untracked file")
                material = archive.read(member)
                if (
                    sha256_bytes(material) != entry.get("sha256")
                    or len(material) != int(entry.get("bytes", -1))
                ):
                    raise ManagementError("dirty evidence untracked file digest changed")
                destination = clone_directory.joinpath(*relative.parts)
                if not is_within(destination, clone_directory):
                    raise ManagementError("dirty evidence path escaped its materialization root")
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(material)
                archived_untracked.append(
                    {
                        "path": relative.as_posix(),
                        "sha256": entry["sha256"],
                        "bytes": entry["bytes"],
                    }
                )
        git("add", "-A", cwd=clone_directory)
        reconstructed_tree = git("write-tree", cwd=clone_directory)
        if reconstructed_tree != candidate_tree:
            failure = {
                "schema_id": "clearra.git-dirty-candidate-evidence-failure.v1",
                "transaction": receipt["transaction"],
                "worktree": str(worktree_path),
                "source_head": item["head"],
                "candidate_sha": candidate_sha,
                "candidate_tree": candidate_tree,
                "reconstructed_tree": reconstructed_tree,
                "materialization": str(clone_directory),
            }
            failure_path = evidence_directory / (
                f"failed-{index:03d}-{candidate_sha[:12]}-{utc_stamp()}.json"
            )
            write_json_atomic(failure_path, failure)
            raise ManagementError(
                f"dirty worktree does not reconstruct the candidate tree; evidence={failure_path}"
            )
        evidence = {
            "schema_id": "clearra.git-dirty-candidate-evidence.v1",
            "transaction": receipt["transaction"],
            "created_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
            "worktree": str(worktree_path),
            "worktree_index": index,
            "source_head": item["head"],
            "candidate_sha": candidate_sha,
            "candidate_tree": candidate_tree,
            "reconstructed_tree": reconstructed_tree,
            "verified_exact_tree": True,
            "archive_sha256": sha256_file(archive_path),
            "untracked_manifest_sha256": sha256_file(manifest_path),
            "patch_sha256": patch_digests,
            "untracked": archived_untracked,
            "generated_entries_omitted": generated_entries,
        }
        evidence_path = evidence_directory / (
            f"dirty-{index:03d}-{candidate_sha[:12]}.json"
        )
        write_json_atomic(evidence_path, evidence)
        evidence_digest = sha256_file(evidence_path)
        item["candidate_evidence"] = {
            "schema_id": "clearra.git-dirty-evidence-reference.v1",
            "receipt": str(evidence_path),
            "sha256": evidence_digest,
            "candidate_sha": candidate_sha,
            "candidate_tree": candidate_tree,
        }
        write_json_atomic(review_path, review)
        decision = record_review_decision(
            receipt_value,
            policy,
            decision="selected",
            reason=(
                f"safety archive reconstructs candidate {candidate_sha} "
                "with exact tree equality"
            ),
            worktree=str(worktree_path),
        )
        return {
            "worktree": str(worktree_path),
            "candidate_sha": candidate_sha,
            "candidate_tree": candidate_tree,
            "evidence": str(evidence_path),
            "evidence_sha256": evidence_digest,
            "review": str(review_path),
            "decision_receipt": decision["receipt"],
        }
    finally:
        if clone_directory.exists():
            remove_owned_path(clone_directory, policy)


def git_review_summary(
    receipt_value: str, candidate: str, policy: dict[str, Any]
) -> dict[str, Any]:
    receipt_path, receipt, _review_path, review = load_safety_transaction(
        receipt_value, policy
    )
    candidate_sha = git("rev-parse", f"{candidate}^{{commit}}")
    groups: dict[str, dict[str, Any]] = {}
    dirty: list[dict[str, Any]] = []
    for item in review.get("items", []):
        if item.get("kind") == "ref":
            sha = str(item.get("sha"))
            group = groups.setdefault(
                sha,
                {
                    "sha": sha,
                    "tree": item.get("tree"),
                    "refs": [],
                    "decisions": [],
                    "contained_in_candidate": run(
                        ("git", "merge-base", "--is-ancestor", sha, candidate_sha),
                        check=False,
                    ).returncode
                    == 0,
                },
            )
            group["refs"].append(item.get("ref"))
            group["decisions"].append(item.get("decision"))
        elif item.get("kind") == "dirty-worktree":
            dirty.append(
                {
                    "worktree": item.get("worktree"),
                    "head": item.get("head"),
                    "dirty_entries": item.get("dirty_entries"),
                    "decision": item.get("decision"),
                    "has_candidate_evidence": isinstance(
                        item.get("candidate_evidence"), dict
                    ),
                }
            )
    summary = {
        "schema_id": "clearra.git-convergence-summary.v1",
        "transaction": receipt["transaction"],
        "candidate_sha": candidate_sha,
        "ref_groups": sorted(groups.values(), key=lambda item: item["sha"]),
        "dirty_worktrees": dirty,
        "counts": {
            "ref_items": sum(len(item["refs"]) for item in groups.values()),
            "unique_ref_shas": len(groups),
            "dirty_worktrees": len(dirty),
            "pending": sum(
                1 for item in review.get("items", []) if item.get("decision") == "pending"
            ),
            "selected": sum(
                1 for item in review.get("items", []) if item.get("decision") == "selected"
            ),
            "excluded": sum(
                1 for item in review.get("items", []) if item.get("decision") == "excluded"
            ),
        },
    }
    summary_path = receipt_path.parent / "review-summary.json"
    write_json_atomic(summary_path, summary)
    return {**summary, "summary": str(summary_path)}


def record_review_decision(
    receipt_value: str,
    policy: dict[str, Any],
    *,
    decision: str,
    reason: str,
    ref: str | None = None,
    worktree: str | None = None,
) -> dict[str, Any]:
    if decision not in {"selected", "excluded"}:
        raise ManagementError("review decision must be selected or excluded")
    if not reason.strip():
        raise ManagementError("review decision requires a non-empty reason")
    if bool(ref) == bool(worktree):
        raise ManagementError("review decision requires exactly one ref or worktree target")
    receipt_path, receipt, review_path, review = load_safety_transaction(
        receipt_value, policy
    )
    matches: list[dict[str, Any]] = []
    for item in review.get("items", []):
        if ref is not None and item.get("kind") == "ref" and item.get("ref") == ref:
            matches.append(item)
        if worktree is not None and item.get("kind") == "dirty-worktree":
            requested = pathlib.Path(worktree).resolve(strict=False)
            recorded = pathlib.Path(str(item.get("worktree") or "")).resolve(strict=False)
            if normalized(requested) == normalized(recorded):
                matches.append(item)
    if len(matches) != 1:
        raise ManagementError("review target must match exactly one pending inventory item")
    item = matches[0]
    if (
        decision == "selected"
        and item.get("kind") == "dirty-worktree"
        and not isinstance(item.get("candidate_evidence"), dict)
    ):
        raise ManagementError(
            "dirty worktree selection requires exact candidate evidence first"
        )
    previous = {"decision": item.get("decision"), "reason": item.get("reason")}
    event = {
        "schema_id": "clearra.git-convergence-review-decision.v1",
        "transaction": receipt["transaction"],
        "target": {
            key: item.get(key)
            for key in ("kind", "ref", "sha", "worktree", "head")
            if item.get(key) is not None
        },
        "previous": previous,
        "decision": decision,
        "reason": reason.strip(),
        "repository_head": repository_head(),
    }
    event_path = receipt_path.parent / (
        f"review-decision-{utc_stamp()}-{uuid.uuid4().hex[:12]}.json"
    )
    write_json_atomic(event_path, event)
    event_reference = {
        "receipt": str(event_path),
        "sha256": sha256_file(event_path),
    }
    item["decision"] = decision
    item["reason"] = reason.strip()
    item.setdefault("decision_receipts", []).append(event_reference)
    write_json_atomic(review_path, review)
    return {
        **event,
        "receipt": str(event_path),
        "receipt_sha256": event_reference["sha256"],
    }


def validate_review_decision_receipt(
    item: dict[str, Any],
    receipt_path: pathlib.Path,
    receipt: dict[str, Any],
    policy: dict[str, Any],
) -> dict[str, Any]:
    references = item.get("decision_receipts")
    if not isinstance(references, list) or not references:
        raise ManagementError("review decision has no immutable decision receipt")
    reference = references[-1]
    path = pathlib.Path(str(reference.get("receipt") or "")).resolve(strict=True)
    if not is_within(path, receipt_path.parent) or is_secret_path(path, policy):
        raise ManagementError("review decision receipt escaped its safety transaction")
    assert_no_link_escape(path, receipt_path.parent)
    if sha256_file(path) != reference.get("sha256"):
        raise ManagementError("review decision receipt digest changed")
    event = json.loads(path.read_text(encoding="utf-8"))
    if (
        event.get("schema_id") != "clearra.git-convergence-review-decision.v1"
        or event.get("transaction") != receipt.get("transaction")
        or event.get("decision") != item.get("decision")
        or event.get("reason") != item.get("reason")
    ):
        raise ManagementError("review decision receipt does not match the active decision")
    target = event.get("target", {})
    for key in ("kind", "ref", "sha", "head"):
        if item.get(key) is not None and target.get(key) != item.get(key):
            raise ManagementError("review decision receipt target changed")
    if item.get("worktree") is not None and normalized(
        pathlib.Path(str(target.get("worktree") or ""))
    ) != normalized(pathlib.Path(str(item["worktree"]))):
        raise ManagementError("review decision receipt worktree target changed")
    return {
        "receipt": str(path),
        "sha256": reference["sha256"],
        "decision": event["decision"],
    }


def convergence_replay_selection(
    receipt_value: str, candidate_sha: str, policy: dict[str, Any]
) -> tuple[pathlib.Path, dict[str, Any], pathlib.Path, dict[str, Any], list[str]]:
    receipt_path, receipt, review_path, review = load_safety_transaction(
        receipt_value, policy
    )
    selected_tips: list[str] = []
    for item in review.get("items", []):
        decision = item.get("decision")
        reason = str(item.get("reason") or "").strip()
        if decision not in {"selected", "excluded"}:
            raise ManagementError("convergence review still contains pending items")
        if not reason:
            raise ManagementError("every selected or excluded convergence item needs a reason")
        validate_review_decision_receipt(item, receipt_path, receipt, policy)
        if decision == "excluded":
            continue
        if item.get("kind") == "ref":
            source_sha = str(item.get("sha") or "")
            if not re.fullmatch(r"[0-9a-f]{40}", source_sha):
                raise ManagementError("selected ref has an invalid commit SHA")
            selected_tips.append(source_sha)
        elif item.get("kind") == "dirty-worktree":
            validate_dirty_candidate_evidence(
                item,
                item.get("candidate_evidence"),
                candidate_sha,
                receipt_path,
                receipt,
                policy,
            )
        else:
            raise ManagementError("convergence review contains an unknown item kind")
    return receipt_path, receipt, review_path, review, sorted(set(selected_tips))


def replay_conflict_evidence(policy: dict[str, Any]) -> dict[str, Any]:
    conflicts: dict[str, dict[str, Any]] = {}
    unmerged = run(("git", "ls-files", "-u", "-z"), check=False).stdout
    for record in [value for value in unmerged.split("\0") if value]:
        metadata, _, path_value = record.partition("\t")
        fields = metadata.split()
        if len(fields) != 3 or not path_value:
            continue
        mode, blob, stage = fields
        if is_secret_path(pathlib.Path(path_value), policy):
            key = "prohibited-path-redacted"
            entry = conflicts.setdefault(key, {"credential_path_blocker_count": 0, "stages": []})
            entry["credential_path_blocker_count"] += 1
            continue
        entry = conflicts.setdefault(path_value, {"stages": []})
        entry["stages"].append({"stage": int(stage), "mode": mode, "blob": blob})
        path = ROOT / path_value
        if path.is_file() and not is_reparse_point(path):
            entry["worktree_sha256"] = sha256_file(path)
            entry["worktree_bytes"] = path.stat().st_size
    return {"files": conflicts, "count": len(conflicts)}


def apply_convergence_review(
    receipt_value: str, candidate: str, policy: dict[str, Any]
) -> dict[str, Any]:
    pattern = policy["git_policy"]["candidate_pattern"]
    branch = git("branch", "--show-current")
    if not branch or not fnmatch.fnmatch(branch, pattern):
        raise ManagementError(f"current branch must match {pattern}")
    if candidate != branch:
        raise ManagementError("convergence replay must target the current candidate branch")
    status = git("status", "--porcelain=v1", "-z", "--untracked-files=all")
    if status:
        raise ManagementError("candidate worktree must be clean before convergence replay")
    initial_sha = git("rev-parse", f"{candidate}^{{commit}}")
    if git("rev-parse", "HEAD") != initial_sha:
        raise ManagementError("current HEAD does not equal the requested candidate")
    origin_main = git("rev-parse", "refs/remotes/origin/main")
    if run(
        ("git", "merge-base", "--is-ancestor", origin_main, initial_sha),
        check=False,
    ).returncode != 0:
        raise ManagementError("candidate is not based on the current origin/main")
    receipt_path, receipt, review_path, review, selected_tips = (
        convergence_replay_selection(receipt_value, initial_sha, policy)
    )
    original_review = json.loads(json.dumps(review))
    review_updated = False
    commits: list[str] = []
    if selected_tips:
        commits = git(
            "rev-list",
            "--reverse",
            "--topo-order",
            *selected_tips,
            "--not",
            initial_sha,
        ).splitlines()
    merge_commits = [
        commit
        for commit in commits
        if len(git("rev-list", "--parents", "-n", "1", commit).split()) > 2
    ]
    if merge_commits:
        blocker = {
            "schema_id": "clearra.git-convergence-replay-blocked.v1",
            "transaction": receipt["transaction"],
            "candidate": branch,
            "candidate_sha": initial_sha,
            "reason": "selected history contains merge commits requiring an explicit mainline",
            "merge_commits": merge_commits,
        }
        blocker_path = receipt_path.parent / f"replay-blocked-{utc_stamp()}.json"
        write_json_atomic(blocker_path, blocker)
        raise ManagementError(f"selected history contains merge commits; receipt={blocker_path}")
    applied: list[dict[str, Any]] = []
    replay_transaction = utc_stamp() + "-" + uuid.uuid4().hex[:12]
    rollback_ref = (
        policy["git_policy"]["safety_ref_prefix"].rstrip("/")
        + f"/{receipt['transaction']}/replay-{replay_transaction}"
    )
    try:
        for source_sha in commits:
            before = git("rev-parse", "HEAD")
            if run(
                ("git", "merge-base", "--is-ancestor", source_sha, before),
                check=False,
            ).returncode == 0:
                applied.append(
                    {"source": source_sha, "result": before, "classification": "already-contained"}
                )
                continue
            names = git(
                "diff-tree", "--no-commit-id", "--name-only", "-r", "--root", source_sha
            ).splitlines()
            if any(is_secret_path(pathlib.Path(name), policy) for name in names):
                raise ManagementError("selected commit changes a prohibited credential path")
            picked = run(("git", "cherry-pick", "-x", source_sha), check=False)
            if picked.returncode != 0:
                conflict = replay_conflict_evidence(policy)
                cherry_head = run(
                    ("git", "rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"),
                    check=False,
                )
                if (
                    conflict["count"] == 0
                    and cherry_head.returncode == 0
                    and run(("git", "diff", "--quiet"), check=False).returncode == 0
                    and run(("git", "diff", "--cached", "--quiet"), check=False).returncode
                    == 0
                ):
                    git("cherry-pick", "--skip")
                    applied.append(
                        {
                            "source": source_sha,
                            "result": git("rev-parse", "HEAD"),
                            "classification": "patch-equivalent-empty",
                        }
                    )
                    continue
                partial = git("rev-parse", "HEAD")
                git("update-ref", rollback_ref, partial)
                abort = run(("git", "cherry-pick", "--abort"), check=False)
                if abort.returncode != 0:
                    raise ManagementError(
                        "convergence replay failed and cherry-pick abort also failed; "
                        f"preserved at {rollback_ref}"
                    )
                git("reset", "--hard", initial_sha)
                receipt_material = {
                    "schema_id": "clearra.git-convergence-replay-conflict.v1",
                    "transaction": receipt["transaction"],
                    "replay_transaction": replay_transaction,
                    "candidate": branch,
                    "initial_sha": initial_sha,
                    "source_commit": source_sha,
                    "partial_head": partial,
                    "rollback_ref": rollback_ref,
                    "applied_before_conflict": applied,
                    "conflict": conflict,
                    "stderr": picked.stderr.strip(),
                    "restored_head": git("rev-parse", "HEAD"),
                }
                conflict_path = receipt_path.parent / (
                    f"replay-conflict-{replay_transaction}.json"
                )
                write_json_atomic(conflict_path, receipt_material)
                raise ManagementError(
                    f"convergence replay conflicted and was rolled back; receipt={conflict_path}"
                )
            result_sha = git("rev-parse", "HEAD")
            applied.append(
                {
                    "source": source_sha,
                    "source_tree": git("rev-parse", f"{source_sha}^{{tree}}"),
                    "before": before,
                    "result": result_sha,
                    "result_tree": git("rev-parse", f"{result_sha}^{{tree}}"),
                    "changed_files": names,
                    "classification": "replayed",
                }
            )
        final_sha = git("rev-parse", "HEAD")
        final_tree = git("rev-parse", f"{final_sha}^{{tree}}")
        replay_evidence = {
            "schema_id": "clearra.git-ref-replay-evidence.v1",
            "transaction": receipt["transaction"],
            "replay_transaction": replay_transaction,
            "candidate": branch,
            "initial_sha": initial_sha,
            "final_sha": final_sha,
            "final_tree": final_tree,
            "selected_tips": selected_tips,
            "applied": applied,
        }
        evidence_directory = receipt_path.parent / "evidence"
        evidence_path = evidence_directory / (
            f"replay-{replay_transaction}.json"
        )
        write_json_atomic(evidence_path, replay_evidence)
        evidence_digest = sha256_file(evidence_path)
        for item in review.get("items", []):
            if item.get("kind") != "ref" or item.get("decision") != "selected":
                continue
            source_sha = str(item.get("sha") or "")
            if run(
                ("git", "merge-base", "--is-ancestor", source_sha, final_sha),
                check=False,
            ).returncode == 0:
                continue
            item["candidate_evidence"] = {
                "schema_id": "clearra.git-ref-evidence-reference.v1",
                "receipt": str(evidence_path),
                "sha256": evidence_digest,
                "source_tip": source_sha,
                "final_sha": final_sha,
            }
        write_json_atomic(review_path, review)
        review_updated = True
        convergence = validate_convergence_review(receipt_value, final_sha, policy)
        material = {
            "schema_id": "clearra.git-convergence-replay.v1",
            "transaction": receipt["transaction"],
            "replay_transaction": replay_transaction,
            "candidate": branch,
            "initial_sha": initial_sha,
            "final_sha": final_sha,
            "final_tree": final_tree,
            "selected_tips": selected_tips,
            "applied": applied,
            "replay_evidence": str(evidence_path),
            "replay_evidence_sha256": evidence_digest,
            "convergence": convergence,
        }
        replay_path = receipt_path.parent / f"replay-{replay_transaction}.json"
        write_json_atomic(replay_path, material)
        return {**material, "receipt": str(replay_path)}
    except Exception:
        if review_updated:
            write_json_atomic(review_path, original_review)
        if git("rev-parse", "HEAD") != initial_sha:
            current = git("rev-parse", "HEAD")
            git("update-ref", rollback_ref, current)
            run(("git", "cherry-pick", "--abort"), check=False)
            git("reset", "--hard", initial_sha)
        raise


def validate_convergence_review(
    receipt_value: str, candidate_sha: str, policy: dict[str, Any]
) -> dict[str, Any]:
    receipt_path, receipt, review_path, review = load_safety_transaction(
        receipt_value, policy
    )
    selected = 0
    excluded = 0
    dirty_evidence: list[dict[str, Any]] = []
    ref_evidence: list[dict[str, Any]] = []
    for item in review.get("items", []):
        decision = item.get("decision")
        reason = str(item.get("reason") or "").strip()
        if decision not in {"selected", "excluded"}:
            raise ManagementError("convergence review still contains pending items")
        if not reason:
            raise ManagementError("every selected or excluded convergence item needs a reason")
        validate_review_decision_receipt(item, receipt_path, receipt, policy)
        if decision == "excluded":
            excluded += 1
            continue
        selected += 1
        if item.get("kind") == "ref":
            source_sha = item.get("sha")
            contained = bool(source_sha) and run(
                ("git", "merge-base", "--is-ancestor", source_sha, candidate_sha),
                check=False,
            ).returncode == 0
            if not contained:
                ref_evidence.append(
                    validate_ref_candidate_evidence(
                        item,
                        item.get("candidate_evidence"),
                        candidate_sha,
                        receipt_path,
                    )
                )
        else:
            dirty_evidence.append(
                validate_dirty_candidate_evidence(
                    item,
                    item.get("candidate_evidence"),
                    candidate_sha,
                    receipt_path,
                    receipt,
                    policy,
                )
            )
    return {
        "receipt": str(receipt_path),
        "transaction": receipt["transaction"],
        "bundle_sha256": receipt["bundle_sha256"],
        "review_sha256": sha256_file(review_path),
        "selected": selected,
        "excluded": excluded,
        "dirty_evidence": dirty_evidence,
        "ref_evidence": ref_evidence,
    }


def default_main_preflight(candidate_sha: str) -> tuple[pathlib.Path, str]:
    main_worktrees = [
        item.path
        for item in parse_worktrees(git("worktree", "list", "--porcelain"))
        if item.branch == "refs/heads/main"
    ]
    if len(main_worktrees) != 1:
        raise ManagementError(
            f"exactly one default checkout of local main is required; found {len(main_worktrees)}"
        )
    main_path = main_worktrees[0]
    status = run(
        ("git", "status", "--porcelain=v1", "-z", "--untracked-files=normal"),
        cwd=main_path,
    ).stdout
    if status:
        raise ManagementError(
            "default checkout main is dirty; preserve and review it before remote promotion"
        )
    local_sha = run(("git", "rev-parse", "main"), cwd=main_path).stdout.strip()
    if run(
        ("git", "merge-base", "--is-ancestor", local_sha, candidate_sha),
        cwd=main_path,
        check=False,
    ).returncode != 0:
        raise ManagementError(
            "local main has changes that are not contained in the candidate; promotion is blocked"
        )
    return main_path, local_sha


def verify_independent_main_checkout(
    expected_sha: str, policy: dict[str, Any]
) -> tuple[dict[str, Any], pathlib.Path]:
    transaction = utc_stamp() + "-" + uuid.uuid4().hex[:12]
    repo_id = hashlib.sha256(normalized(ROOT).encode()).hexdigest()[:24]
    directory = state_root() / "git-verification" / repo_id / transaction
    directory.parent.mkdir(parents=True, exist_ok=True)
    remote_url = github_https_clone_url(
        git("remote", "get-url", policy["git_policy"]["remote"])
    )
    failure_phase = "clone"
    try:
        run(("git", "clone", "--no-local", "--no-checkout", remote_url, str(directory)))
        failure_phase = "checkout"
        run(("git", "checkout", "--detach", expected_sha), cwd=directory)
        failure_phase = "identity"
        actual_sha = run(("git", "rev-parse", "HEAD"), cwd=directory).stdout.strip()
        actual_tree = run(("git", "rev-parse", "HEAD^{tree}"), cwd=directory).stdout.strip()
        expected_tree = git("rev-parse", f"{expected_sha}^{{tree}}")
        if actual_sha != expected_sha or actual_tree != expected_tree:
            raise ManagementError("independent checkout does not match promoted main")
        required_files = [
            "config/clearra-management.v1.json",
            "pnpm-lock.yaml",
            "rust-toolchain.toml",
        ]
        digests: dict[str, str] = {}
        for relative in required_files:
            path = directory / relative
            if not path.is_file():
                raise ManagementError(f"independent checkout is missing {relative}")
            digests[relative] = sha256_file(path)
        for arguments in (
            (sys.executable, "-B", "scripts/management/clearra_manage.py", "deps", "verify"),
            (sys.executable, "-B", "scripts/management/clearra_manage.py", "storage", "verify"),
        ):
            failure_phase = "policy-" + "-".join(arguments[3:])
            result = run(arguments, cwd=directory, check=False)
            if result.returncode != 0:
                detail = result.stderr.strip() or result.stdout.strip()
                raise ManagementError(
                    f"independent checkout policy verification failed: {' '.join(arguments[3:])}\n{detail}"
                )
        validation_commands: list[tuple[str, tuple[str, ...], pathlib.Path]] = [
            (
                "toolchain-sync",
                (sys.executable, "-B", "scripts/management/clearra_manage.py", "toolchain", "sync"),
                directory,
            ),
            (
                "deps-install",
                (sys.executable, "-B", "scripts/management/clearra_manage.py", "deps", "install"),
                directory,
            ),
            ("ctk3-test", ("pnpm", "--filter", "ctk3", "run", "test"), directory),
            ("ui-test", ("pnpm", "--filter", "@clearra/ui", "run", "test"), directory),
            (
                "discord-production-deploy",
                (
                    "pnpm",
                    "deploy",
                    "--filter",
                    "@clearra/discord-bot",
                    "--prod",
                    "build/discord-container/independent-main",
                ),
                directory,
            ),
            (
                "discord-runtime-smoke",
                (
                    "node",
                    "--input-type=module",
                    "-e",
                    "await import('./src/clearra/command.mjs'); "
                    "await import('./src/job-service/server.mjs'); await import('ctk3')",
                ),
                directory / "build" / "discord-container" / "independent-main",
            ),
            (
                "cargo-fmt",
                (
                    sys.executable,
                    "-B",
                    "scripts/management/clearra_manage.py",
                    "storage",
                    "run",
                    "--producer",
                    "cargo",
                    "--",
                    "cargo",
                    "fmt",
                    "--all",
                    "--check",
                ),
                directory,
            ),
            (
                "cargo-check",
                (
                    sys.executable,
                    "-B",
                    "scripts/management/clearra_manage.py",
                    "storage",
                    "run",
                    "--producer",
                    "cargo",
                    "--",
                    "cargo",
                    "check",
                    "--workspace",
                    "--locked",
                ),
                directory,
            ),
        ]
        executed: list[dict[str, Any]] = []
        for validation_id, command, command_cwd in validation_commands:
            failure_phase = "core-" + validation_id
            started = time.monotonic()
            result = run(command, cwd=command_cwd, check=False)
            executed.append(
                {
                    "command": list(command),
                    "cwd": str(command_cwd.relative_to(directory))
                    if command_cwd != directory
                    else ".",
                    "exit_code": result.returncode,
                    "elapsed_seconds": round(time.monotonic() - started, 3),
                }
            )
            if result.returncode != 0:
                detail = result.stderr.strip() or result.stdout.strip()
                raise ManagementError(
                    "independent checkout core validation failed: "
                    f"{' '.join(command)}\n{detail}"
                )
        evidence = {
            "path": str(directory),
            "sha": actual_sha,
            "tree": actual_tree,
            "file_sha256": digests,
            "policy_verification": "accepted",
            "core_validation": executed,
        }
        return evidence, directory
    except Exception as error:
        # Keep a checkout that was actually created and bind its location and
        # closed failure phase to an external receipt. Never persist child
        # output because it may contain environment-specific sensitive text.
        checkout_retained = directory.exists()
        failure_receipt = write_receipt(
            "git-independent-verification-failed",
            {
                "expected_sha": expected_sha,
                "checkout": str(directory),
                "checkout_retained": checkout_retained,
                "failure_phase": failure_phase,
                "error_type": type(error).__name__,
                "owned_paths": [str(directory)] if checkout_retained else [],
            },
        )
        disposition = "the checkout was retained" if checkout_retained else "no checkout was created"
        raise ManagementError(
            f"independent main verification failed during {failure_phase}; {disposition}; "
            f"receipt={failure_receipt}"
        ) from error


def promote_candidate(
    candidate: str, safety_receipt: str, policy: dict[str, Any]
) -> dict[str, Any]:
    pattern = policy["git_policy"]["candidate_pattern"]
    if not fnmatch.fnmatch(candidate, pattern):
        raise ManagementError(f"candidate branch must match {pattern}")
    sha, checks = check_candidate_once(candidate, policy)
    convergence = validate_convergence_review(safety_receipt, sha, policy)
    git("fetch", "origin", "refs/heads/main:refs/remotes/origin/main")
    before = git("rev-parse", "refs/remotes/origin/main")
    if run(("git", "merge-base", "--is-ancestor", before, sha), check=False).returncode != 0:
        raise ManagementError("candidate is not a fast-forward of current origin/main")
    candidate_files = git("diff", "--name-only", before, sha).splitlines()
    if any(is_secret_path(pathlib.Path(name), policy) for name in candidate_files):
        raise ManagementError(
            "candidate changes a prohibited credential path; contents were not inspected"
        )
    diff_digest = hashlib.sha256(
        run(("git", "diff", "--binary", before, sha)).stdout.encode("utf-8")
    ).hexdigest()
    main_path, local_before = default_main_preflight(sha)
    ruleset = apply_ruleset(policy)
    remote = push_main_fast_forward(sha, before)
    run(("git", "merge", "--ff-only", sha), cwd=main_path)
    local_update = run(("git", "rev-parse", "HEAD"), cwd=main_path).stdout.strip()
    if local_update != sha:
        raise ManagementError("local main readback does not equal the promoted candidate")
    independent, verification_path = verify_independent_main_checkout(sha, policy)
    if independent["sha"] != sha or independent["tree"] != git("rev-parse", f"{sha}^{{tree}}"):
        raise ManagementError("independent main verification did not converge")
    receipt = {
        "candidate": candidate,
        "sha": sha,
        "parent": git("rev-parse", f"{sha}^"),
        "tree": git("rev-parse", f"{sha}^{{tree}}"),
        "origin_main_before": before,
        "origin_main_after": remote,
        "checks": checks,
        "ci_run_ids": [item["id"] for item in checks],
        "convergence": convergence,
        "ruleset": ruleset,
        "policy_sha256": sha256_file(ROOT / "config" / "clearra-management.v1.json"),
        "lockfile_sha256": sha256_file(ROOT / "pnpm-lock.yaml"),
        "toolchain_sha256": sha256_file(ROOT / "rust-toolchain.toml"),
        "diff_sha256": diff_digest,
        "local_main_before": local_before,
        "local_main_update": local_update,
        "independent_checkout": independent,
    }
    receipt["receipt"] = str(write_receipt("git-promotion", receipt))
    return receipt


def upload_candidate(
    candidate: str,
    safety_receipt: str,
    policy: dict[str, Any],
    *,
    exact_commit: str | None = None,
) -> dict[str, Any]:
    """Publish a reviewed candidate with a normal fast-forward push and readback."""
    pattern = policy["git_policy"]["candidate_pattern"]
    if not fnmatch.fnmatch(candidate, pattern):
        raise ManagementError(f"candidate branch must match {pattern}")
    if run(("git", "check-ref-format", f"refs/heads/{candidate}"), check=False).returncode:
        raise ManagementError("candidate is not a valid Git branch name")
    if git("status", "--porcelain=v1", "-z", "--untracked-files=all"):
        raise ManagementError("candidate worktree must be clean before upload")
    branch = git("branch", "--show-current")
    if exact_commit is None:
        if branch != candidate:
            raise ManagementError("candidate upload must run from the candidate branch")
        sha = git("rev-parse", f"refs/heads/{candidate}^{{commit}}")
        source_mode = "candidate-branch"
    else:
        if not re.fullmatch(r"[0-9a-f]{40}", exact_commit):
            raise ManagementError("detached candidate upload requires an exact commit SHA")
        if branch:
            raise ManagementError(
                "exact commit upload is allowed only from a detached clean worktree"
            )
        sha = git("rev-parse", f"{exact_commit}^{{commit}}")
        if sha != exact_commit or git("rev-parse", "HEAD") != sha:
            raise ManagementError(
                "detached candidate upload requires HEAD to equal the exact commit SHA"
            )
        source_mode = "detached-exact-sha"
    local_candidate_lookup = run(
        ("git", "rev-parse", "--verify", f"refs/heads/{candidate}^{{commit}}"),
        check=False,
    )
    local_candidate_sha = (
        local_candidate_lookup.stdout.strip()
        if local_candidate_lookup.returncode == 0
        else None
    )
    local_candidate_worktrees: list[dict[str, Any]] = []
    for worktree in parse_worktrees(git("worktree", "list", "--porcelain")):
        if worktree.branch != f"refs/heads/{candidate}":
            continue
        status = run(
            ("git", "status", "--porcelain=v1", "-z", "--untracked-files=all"),
            cwd=worktree.path,
        ).stdout
        local_candidate_worktrees.append(
            {
                "path": str(worktree.path),
                "head": worktree.head,
                "dirty_entries": len([entry for entry in status.split("\0") if entry]),
                "preserved": True,
            }
        )
    convergence = validate_convergence_review(safety_receipt, sha, policy)
    git("fetch", "origin", "refs/heads/main:refs/remotes/origin/main")
    origin_main = git("rev-parse", "refs/remotes/origin/main")
    if run(
        ("git", "merge-base", "--is-ancestor", origin_main, sha), check=False
    ).returncode:
        raise ManagementError("candidate is not a fast-forward of current origin/main")
    remote_main = git("ls-remote", "origin", "refs/heads/main").split()
    if not remote_main or remote_main[0] != origin_main:
        raise ManagementError("origin/main changed during candidate upload preflight")

    changed = git("diff", "--name-only", origin_main, sha).splitlines()
    if any(is_secret_path(pathlib.Path(name), policy) for name in changed):
        raise ManagementError(
            "candidate changes a prohibited credential path; contents were not inspected"
        )
    authorized_pushers = authorized_github_maintainers(policy)

    remote_ref = f"refs/heads/{candidate}"
    remote_line = git("ls-remote", "--heads", "origin", remote_ref).split()
    remote_before = remote_line[0] if remote_line else None
    if remote_before and remote_before != sha:
        tracking_ref = f"refs/remotes/origin/{candidate}"
        git("fetch", "origin", f"{remote_ref}:{tracking_ref}")
        if run(
            ("git", "merge-base", "--is-ancestor", remote_before, sha), check=False
        ).returncode:
            raise ManagementError("candidate upload is not a fast-forward of its remote branch")

    uploaded = remote_before != sha
    if uploaded:
        pushed = run(
            ("git", "push", "origin", f"{sha}:{remote_ref}"),
            check=False,
        )
        if pushed.returncode != 0:
            detail = pushed.stderr.strip() or pushed.stdout.strip()
            raise ManagementError(f"normal candidate push was rejected\n{detail}")
    readback = git("ls-remote", "--heads", "origin", remote_ref).split()
    if not readback or readback[0] != sha:
        raise ManagementError("remote candidate readback does not equal the reviewed candidate")

    receipt = {
        "schema_id": "clearra.git-candidate-upload.v1",
        "candidate": candidate,
        "sha": sha,
        "tree": git("rev-parse", f"{sha}^{{tree}}"),
        "parents": git("rev-list", "--parents", "-n", "1", sha).split()[1:],
        "source_mode": source_mode,
        "local_candidate_ref_sha": local_candidate_sha,
        "local_candidate_worktrees": local_candidate_worktrees,
        "origin_main": origin_main,
        "remote_candidate_before": remote_before,
        "remote_candidate_after": readback[0],
        "uploaded": uploaded,
        "diff_sha256": hashlib.sha256(
            git("diff", "--binary", origin_main, sha).encode("utf-8")
        ).hexdigest(),
        "policy_sha256": sha256_file(ROOT / "config" / "clearra-management.v1.json"),
        "lockfile_sha256": sha256_file(ROOT / "pnpm-lock.yaml"),
        "toolchain_sha256": sha256_file(ROOT / "rust-toolchain.toml"),
        "convergence": convergence,
        "authorized_pushers": authorized_pushers,
    }
    receipt["receipt"] = str(write_receipt("git-candidate-upload", receipt))
    return receipt


def push_main_fast_forward(candidate_sha: str, expected_base: str) -> str:
    """Publish main once, with an immediate base check and exact readback."""
    current = git("ls-remote", "origin", "refs/heads/main").split()
    if not current or current[0] != expected_base:
        raise ManagementError("origin/main changed after candidate preflight")
    pushed = run(
        ("git", "push", "origin", f"{candidate_sha}:refs/heads/main"),
        check=False,
    )
    if pushed.returncode != 0:
        detail = pushed.stderr.strip() or pushed.stdout.strip()
        raise ManagementError(f"normal fast-forward push was rejected\n{detail}")
    readback = git("ls-remote", "origin", "refs/heads/main").split()
    if not readback or readback[0] != candidate_sha:
        raise ManagementError("origin/main readback does not equal the promoted candidate")
    return readback[0]


def authorized_github_maintainers(policy: dict[str, Any]) -> list[dict[str, Any]]:
    configured = policy["git_policy"].get("authorized_maintainers", [])
    if not configured:
        raise ManagementError("Git policy has no authorized maintainers")
    expected = {
        (str(item.get("login") or "").casefold(), int(item.get("github_user_id") or 0))
        for item in configured
    }
    if any(not login or identifier <= 0 for login, identifier in expected):
        raise ManagementError("Git policy contains an invalid authorized maintainer")
    repo = repository_name()
    current = json.loads(run(("gh", "api", "user")).stdout)
    current_identity = (str(current.get("login") or "").casefold(), int(current.get("id") or 0))
    if current_identity not in expected:
        raise ManagementError("authenticated GitHub user is not an authorized maintainer")
    collaborators = json.loads(
        run(("gh", "api", f"repos/{repo}/collaborators?affiliation=all&per_page=100")).stdout
    )
    pushers = [
        {
            "login": item.get("login"),
            "github_user_id": item.get("id"),
            "permissions": item.get("permissions", {}),
        }
        for item in collaborators
        if item.get("permissions", {}).get("push")
        or item.get("permissions", {}).get("maintain")
        or item.get("permissions", {}).get("admin")
    ]
    actual = {
        (str(item.get("login") or "").casefold(), int(item.get("github_user_id") or 0))
        for item in pushers
    }
    if actual != expected:
        raise ManagementError(
            "repository push collaborators do not exactly match authorized_maintainers"
        )
    return pushers


def verify_ruleset_readback(
    value: dict[str, Any], policy: dict[str, Any]
) -> dict[str, Any]:
    if (
        value.get("name") != "Clearra main fast-forward gate"
        or value.get("target") != "branch"
        or value.get("enforcement") != "active"
    ):
        raise ManagementError("GitHub ruleset readback identity does not match policy")
    includes = value.get("conditions", {}).get("ref_name", {}).get("include", [])
    excludes = value.get("conditions", {}).get("ref_name", {}).get("exclude", [])
    if includes != ["~DEFAULT_BRANCH"] or excludes != []:
        raise ManagementError("GitHub ruleset does not target only the default branch")
    if value.get("bypass_actors"):
        raise ManagementError("GitHub ruleset unexpectedly grants a bypass actor")
    rules = value.get("rules", [])
    types = [item.get("type") for item in rules]
    expected_types = {
        "deletion",
        "non_fast_forward",
        "required_linear_history",
        "required_status_checks",
    }
    if set(types) != expected_types or len(types) != len(expected_types):
        raise ManagementError("GitHub ruleset rule types do not exactly match policy")
    required = next(item for item in rules if item.get("type") == "required_status_checks")
    parameters = required.get("parameters", {})
    status_checks = parameters.get("required_status_checks", [])
    contexts = [item.get("context") for item in status_checks]
    expected_contexts = policy["git_policy"]["required_checks"]
    if sorted(contexts) != sorted(expected_contexts) or len(contexts) != len(
        expected_contexts
    ):
        raise ManagementError("GitHub ruleset required checks do not match policy")
    if (
        parameters.get("strict_required_status_checks_policy") is not True
        or parameters.get("do_not_enforce_on_create") is not True
    ):
        raise ManagementError("GitHub ruleset status-check parameters do not match policy")
    return {
        "id": value.get("id"),
        "name": value.get("name"),
        "target": value.get("target"),
        "enforcement": value.get("enforcement"),
        "ref_include": includes,
        "rule_types": sorted(types),
        "required_checks": sorted(contexts),
        "bypass_actors": [],
    }


def read_ruleset(ruleset_id: int | str) -> dict[str, Any]:
    """Read one repository ruleset through a narrow, fixture-friendly boundary."""
    return json.loads(
        run(("gh", "api", f"repos/{repository_name()}/rulesets/{ruleset_id}")).stdout
    )


def apply_ruleset(policy: dict[str, Any]) -> dict[str, Any]:
    repo = repository_name()
    name = "Clearra main fast-forward gate"
    maintainers = authorized_github_maintainers(policy)
    payload = {
        "name": name,
        "target": "branch",
        "enforcement": "active",
        "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}},
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
                        {"context": name} for name in policy["git_policy"]["required_checks"]
                    ],
                },
            },
        ],
        "bypass_actors": [],
    }
    existing = json.loads(run(("gh", "api", f"repos/{repo}/rulesets")).stdout)
    match = next((item for item in existing if item.get("name") == name), None)
    if match:
        response = run(("gh", "api", "--method", "PUT", f"repos/{repo}/rulesets/{match['id']}", "--input", "-"), input_text=json.dumps(payload))
    else:
        response = run(("gh", "api", "--method", "POST", f"repos/{repo}/rulesets", "--input", "-"), input_text=json.dumps(payload))
    changed = json.loads(response.stdout)
    ruleset_id = changed.get("id")
    if not ruleset_id:
        raise ManagementError("GitHub ruleset mutation did not return an id")
    readback = read_ruleset(ruleset_id)
    verified = verify_ruleset_readback(readback, policy)
    receipt = {
        "repository": repo,
        "ruleset": verified,
        "authorized_maintainers": maintainers,
    }
    receipt["receipt"] = str(write_receipt("github-ruleset", receipt))
    return receipt


def load_promotion_receipt(
    value: str, policy: dict[str, Any]
) -> tuple[pathlib.Path, dict[str, Any]]:
    path = pathlib.Path(value).resolve(strict=True)
    root = state_root() / "receipts"
    if not is_within(path, root) or is_secret_path(path, policy):
        raise ManagementError("finalization requires a managed promotion receipt")
    assert_no_link_escape(path, root)
    receipt = json.loads(path.read_text(encoding="utf-8"))
    if (
        receipt.get("schema_id") != "clearra.management-receipt.v1"
        or receipt.get("kind") != "git-promotion"
    ):
        raise ManagementError("unsupported Git promotion receipt")
    return path, receipt


def ref_is_reviewed_or_equivalent(
    ref: str,
    sha: str,
    main_sha: str,
    review_by_ref: dict[str, dict[str, Any]],
    reviewed_shas: set[str],
) -> tuple[bool, str]:
    if ref in review_by_ref:
        reviewed = review_by_ref[ref]
        decision = str(reviewed.get("decision"))
        if reviewed.get("sha") == sha and decision in {"selected", "excluded"}:
            return True, f"review-{decision}"
    if sha in reviewed_shas:
        return True, "reviewed-sha"
    classification = classify_ref(sha, main_sha)
    if classification in {"ancestor", "tree-identical", "patch-equivalent"}:
        return True, classification
    for reviewed_sha in sorted(reviewed_shas):
        if run(
            ("git", "merge-base", "--is-ancestor", sha, reviewed_sha),
            check=False,
        ).returncode == 0:
            return True, "ancestor-of-reviewed-sha"
    return False, classification


def finalize_candidate(
    candidate: str,
    safety_receipt_value: str,
    promotion_receipt_value: str,
    policy: dict[str, Any],
    *,
    apply: bool,
) -> dict[str, Any]:
    promotion_path, promotion = load_promotion_receipt(promotion_receipt_value, policy)
    sha = str(promotion.get("sha") or "")
    if promotion.get("candidate") != candidate or not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ManagementError("promotion receipt does not match the requested candidate")
    safety_path, safety, _review_path, review = load_safety_transaction(
        safety_receipt_value, policy
    )
    convergence = validate_convergence_review(safety_receipt_value, sha, policy)
    remote_main_material = git("ls-remote", "origin", "refs/heads/main").split()
    if not remote_main_material or remote_main_material[0] != sha:
        raise ManagementError("origin/main does not equal the promoted candidate")
    main_worktrees = [
        item
        for item in parse_worktrees(git("worktree", "list", "--porcelain"))
        if item.branch == "refs/heads/main"
    ]
    if len(main_worktrees) != 1:
        raise ManagementError("finalization requires exactly one local main worktree")
    main_worktree = main_worktrees[0]
    if main_worktree.head != sha:
        raise ManagementError("local main does not equal the promoted candidate")
    if run(
        ("git", "status", "--porcelain=v1", "-z", "--untracked-files=all"),
        cwd=main_worktree.path,
    ).stdout:
        raise ManagementError("local main must be clean before finalization")
    if normalized(ROOT) != normalized(main_worktree.path):
        raise ManagementError("git finalize must run from the default main checkout")

    independent = promotion.get("independent_checkout", {})
    independent_path = pathlib.Path(str(independent.get("path") or "")).resolve(strict=True)
    verification_root = state_root() / "git-verification"
    if not is_within(independent_path, verification_root):
        raise ManagementError("independent verification checkout escaped its managed root")
    assert_no_link_escape(independent_path, verification_root)
    independent_sha = git("rev-parse", "HEAD", cwd=independent_path)
    independent_tree = git("rev-parse", "HEAD^{tree}", cwd=independent_path)
    if independent_sha != sha or independent_tree != promotion.get("tree"):
        raise ManagementError("independent verification checkout no longer matches promotion")

    ruleset_id = promotion.get("ruleset", {}).get("ruleset", {}).get("id")
    if not ruleset_id:
        raise ManagementError("promotion receipt has no verified GitHub ruleset")
    ruleset = read_ruleset(ruleset_id)
    ruleset_readback = verify_ruleset_readback(ruleset, policy)

    review_by_ref = {
        str(item.get("ref")): item
        for item in review.get("items", [])
        if item.get("kind") == "ref"
    }
    reviewed_shas = {
        str(item.get("sha"))
        for item in review_by_ref.values()
        if item.get("decision") in {"selected", "excluded"}
    }
    worktrees = parse_worktrees(git("worktree", "list", "--porcelain"))
    removable_worktrees: list[dict[str, Any]] = []
    blocked_worktrees: list[dict[str, Any]] = []
    for worktree in worktrees:
        if normalized(worktree.path) == normalized(main_worktree.path):
            continue
        dirty = run(
            ("git", "status", "--porcelain=v1", "-z", "--untracked-files=all"),
            cwd=worktree.path,
        ).stdout
        if dirty:
            blocked_worktrees.append(
                {"path": str(worktree.path), "head": worktree.head, "reason": "dirty"}
            )
            continue
        branch_ref = worktree.branch or ""
        safe, reason = ref_is_reviewed_or_equivalent(
            branch_ref, worktree.head, sha, review_by_ref, reviewed_shas
        )
        if (
            not safe
            and candidate == branch_ref.removeprefix("refs/heads/")
            and worktree.head == sha
        ):
            safe, reason = True, "promoted-candidate"
        target = {"path": str(worktree.path), "head": worktree.head, "reason": reason}
        (removable_worktrees if safe else blocked_worktrees).append(target)
    if blocked_worktrees:
        blocker_receipt = write_receipt(
            "git-finalization-blocked",
            {
                "candidate": candidate,
                "sha": sha,
                "safety_receipt": str(safety_path),
                "promotion_receipt": str(promotion_path),
                "blocked_worktrees": blocked_worktrees,
            },
        )
        raise ManagementError(
            f"worktree finalization remains blocked for {len(blocked_worktrees)} worktrees; "
            f"receipt={blocker_receipt}"
        )

    local_refs: list[dict[str, str]] = []
    for line in git(
        "for-each-ref", "--format=%(refname)%00%(objectname)", "refs/heads"
    ).splitlines():
        ref, object_sha = line.split("\0", 1)
        if ref == "refs/heads/main":
            continue
        safe, reason = ref_is_reviewed_or_equivalent(
            ref, object_sha, sha, review_by_ref, reviewed_shas
        )
        if ref == f"refs/heads/{candidate}" and object_sha == sha:
            safe, reason = True, "promoted-candidate"
        if not safe:
            raise ManagementError(f"local ref remains unreviewed: {ref}")
        local_refs.append({"ref": ref, "sha": object_sha, "reason": reason})

    remote_refs: list[dict[str, str]] = []
    remote_lines = git(
        "for-each-ref", "--format=%(refname)%00%(objectname)", "refs/remotes/origin"
    ).splitlines()
    for line in remote_lines:
        ref, object_sha = line.split("\0", 1)
        if ref in {"refs/remotes/origin/main", "refs/remotes/origin/HEAD"}:
            continue
        safe, reason = ref_is_reviewed_or_equivalent(
            ref, object_sha, sha, review_by_ref, reviewed_shas
        )
        branch_name = ref.removeprefix("refs/remotes/origin/")
        if branch_name == candidate and object_sha == sha:
            safe, reason = True, "promoted-candidate"
        if not safe:
            raise ManagementError(f"remote ref remains unreviewed: {ref}")
        remote_refs.append(
            {"ref": ref, "branch": branch_name, "sha": object_sha, "reason": reason}
        )

    prefix = policy["git_policy"]["safety_ref_prefix"].rstrip("/") + "/" + safety["transaction"]
    safety_refs = [
        {"ref": line.split("\0", 1)[0], "sha": line.split("\0", 1)[1]}
        for line in git(
            "for-each-ref", "--format=%(refname)%00%(objectname)", prefix
        ).splitlines()
        if "\0" in line
    ]
    plan = {
        "schema_id": "clearra.git-finalization-plan.v1",
        "candidate": candidate,
        "sha": sha,
        "tree": promotion.get("tree"),
        "promotion_receipt": str(promotion_path),
        "safety_receipt": str(safety_path),
        "convergence": convergence,
        "ruleset": ruleset_readback,
        "main_worktree": str(main_worktree.path),
        "independent_checkout": str(independent_path),
        "worktrees": removable_worktrees,
        "local_refs": local_refs,
        "remote_refs": remote_refs,
        "safety_refs": safety_refs,
        "apply": apply,
    }
    if not apply:
        return plan

    for worktree in removable_worktrees:
        git("worktree", "remove", worktree["path"])
    if remote_refs:
        git(
            "push",
            "--atomic",
            "origin",
            "--delete",
            *[entry["branch"] for entry in remote_refs],
        )
    if local_refs:
        delete_refs_atomically(
            [(entry["ref"], entry["sha"]) for entry in local_refs]
        )
    remaining_remote = {
        line.split()[1]
        for line in git("ls-remote", "--heads", "origin").splitlines()
        if len(line.split()) == 2
    }
    if remaining_remote != {"refs/heads/main"}:
        raise ManagementError("remote branch cleanup did not converge to main only")
    remaining_local = git("for-each-ref", "--format=%(refname)", "refs/heads").splitlines()
    if remaining_local != ["refs/heads/main"]:
        raise ManagementError("local branch cleanup did not converge to main only")
    remaining_worktrees = parse_worktrees(git("worktree", "list", "--porcelain"))
    if len(remaining_worktrees) != 1 or normalized(remaining_worktrees[0].path) != normalized(
        main_worktree.path
    ):
        raise ManagementError("worktree cleanup did not converge to local main only")
    remove_owned_path(independent_path, policy)
    if safety_refs:
        delete_refs_atomically(
            [(entry["ref"], entry["sha"]) for entry in safety_refs]
        )
    safety_directory = safety_path.parent
    remove_owned_path(safety_directory, policy)
    final = {
        **plan,
        "apply": True,
        "remote_main_readback": git("ls-remote", "origin", "refs/heads/main").split()[0],
        "local_main_readback": git("rev-parse", "HEAD", cwd=main_worktree.path),
        "local_tree_readback": git("rev-parse", "HEAD^{tree}", cwd=main_worktree.path),
        "independent_checkout_removed": not independent_path.exists(),
        "safety_transaction_removed": not safety_directory.exists(),
    }
    final["receipt"] = str(write_receipt("git-finalization", final))
    return final


def print_json(value: Any) -> None:
    print(json.dumps(value, indent=2, ensure_ascii=False))


def wsl_supervisor(policy: dict[str, Any]) -> runtime_owner.WslSupervisor:
    temporary_value = policy["runtime_policy"]["wsl"]["temporary_root_windows"]
    temporary_root = pathlib.Path(os.path.expandvars(temporary_value))
    assert_output_path(temporary_root, policy)

    def validate_output(path: pathlib.Path) -> pathlib.Path:
        try:
            return assert_output_path(path, policy)
        except ManagementError as error:
            raise runtime_owner.RuntimePolicyError(str(error)) from error

    return runtime_owner.WslSupervisor(
        root=ROOT,
        policy=policy,
        state_root=state_root(),
        temporary_root=temporary_root,
        secret_predicate=lambda path: is_secret_path(path, policy),
        output_validator=validate_output,
    )


def runtime_audit_command(policy: dict[str, Any]) -> int:
    audit = runtime_owner.runtime_audit(root=ROOT, policy=policy)
    try:
        verify_static_management_policy(policy)
        audit["static_policy"] = {"accepted": True, "failures": []}
    except ManagementError as error:
        audit["static_policy"] = {"accepted": False, "error": str(error)}
    receipt = write_receipt("runtime-audit", audit)
    print_json({**audit, "receipt": str(receipt)})
    return 0 if audit["static_policy"]["accepted"] else 1


def runtime_wsl_command(arguments: argparse.Namespace, policy: dict[str, Any]) -> int:
    verify_static_management_policy(policy)
    supervisor = wsl_supervisor(policy)
    try:
        if arguments.wsl_action == "provision":
            result = supervisor.provision(arguments.source_distro)
            kind = "runtime-wsl-provision"
            exit_code = 0
        elif arguments.wsl_action == "verify":
            result = supervisor.verify()
            kind = "runtime-wsl-verify"
            exit_code = int(result["runtime"]["returncode"])
        elif arguments.wsl_action == "run":
            command_arguments = list(arguments.arguments)
            if command_arguments and command_arguments[0] == "--":
                command_arguments.pop(0)
            result = supervisor.run_entry(
                arguments.entry,
                command_arguments,
                minimum_override_mib=arguments.minimum_memory_mib,
            )
            kind = "runtime-wsl-run"
            exit_code = int(result["runtime"]["returncode"])
        else:
            raise ManagementError("unsupported runtime WSL action")
    except runtime_owner.RuntimePolicyError as error:
        receipt = write_receipt(
            "runtime-wsl-failed",
            {
                "action": arguments.wsl_action,
                "entry": getattr(arguments, "entry", None),
                "error_type": type(error).__name__,
                "error": str(error),
                "details": getattr(error, "details", {}),
            },
        )
        raise ManagementError(f"{error}; receipt={receipt}") from error
    durable = dict(result)
    durable.pop("runtime_stdout", None)
    durable.pop("runtime_stderr", None)
    receipt = write_receipt(kind, durable)
    print_json({**durable, "receipt": str(receipt)})
    return exit_code


def management_command_profile(
    arguments: argparse.Namespace, policy: dict[str, Any]
) -> str | None:
    domain = str(getattr(arguments, "domain", ""))
    action = str(getattr(arguments, "action", ""))
    domains = policy.get("runtime_policy", {}).get("management_command_profiles", {})
    actions = domains.get(domain, {}) if isinstance(domains, Mapping) else {}
    profile = actions.get(action) if isinstance(actions, Mapping) else None
    return str(profile) if profile else None


def run_management_command_supervised(
    requested_argv: Sequence[str],
    arguments: argparse.Namespace,
    policy: dict[str, Any],
) -> int:
    profile = management_command_profile(arguments, policy)
    if profile is None:
        raise ManagementError("management command has no registered runtime profile")
    contract = runtime_owner.profile_contract(policy, profile)
    environment = os.environ.copy()
    environment.update(
        {
            "CLEARRA_MANAGED_PRODUCER": "management",
            "CLEARRA_RUNTIME_PROFILE": profile,
            "CLEARRA_RUNTIME_SUPERVISED": "1",
        }
    )
    context_key = f"{arguments.domain}.{arguments.action}"
    if bool(getattr(arguments, "apply", False)):
        context_key += ".apply"
    context = policy.get("runtime_policy", {}).get(
        "management_hard_containment_contexts", {}
    ).get(context_key)
    if context:
        context_variable = policy["runtime_policy"]["hard_containment_context_env"].get(
            context
        )
        if not context_variable:
            raise ManagementError(
                f"management command has an unknown hard-containment context: {context_key}"
            )
        environment[str(context_variable)] = "1"
    command = [sys.executable, "-B", str(pathlib.Path(__file__).resolve()), *requested_argv]
    try:
        result = runtime_owner.run_host_process(
            command,
            cwd=ROOT,
            env=environment,
            policy=policy,
            profile=profile,
            timeout_override_seconds=int(contract["maximum_timeout_seconds"]),
        )
    except runtime_owner.RuntimePolicyError as error:
        receipt = write_receipt(
            "management-command-denied",
            {
                "domain": arguments.domain,
                "action": arguments.action,
                "profile": profile,
                "command": runtime_owner.redact_argv(command),
                "command_sha256": runtime_owner.command_digest(command),
                "error_type": type(error).__name__,
                "error": str(error),
            },
        )
        raise ManagementError(f"{error}; receipt={receipt}") from error
    receipt = write_receipt(
        "management-command-runtime",
        {
            "domain": arguments.domain,
            "action": arguments.action,
            "profile": profile,
            "runtime": result.receipt(),
        },
    )
    print(f"management_runtime_receipt={receipt}", file=sys.stderr)
    return result.returncode


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    domains = root.add_subparsers(dest="domain", required=True)

    storage = domains.add_parser("storage")
    storage_actions = storage.add_subparsers(dest="action", required=True)
    storage_actions.add_parser("audit")
    verify = storage_actions.add_parser("verify")
    verify.add_argument("--path")
    verify.add_argument("--force-unmanaged-output", action="store_true")
    verify.add_argument("--force-reason")
    clean = storage_actions.add_parser("clean")
    clean.add_argument("--receipt", required=True)
    clean.add_argument("--apply", action="store_true")
    managed_run = storage_actions.add_parser("run")
    managed_run.add_argument("--producer", required=True)
    managed_run.add_argument("--profile")
    managed_run.add_argument("--minimum-memory-mib", type=int)
    managed_run.add_argument("--timeout", type=int)
    managed_run.add_argument("--export-path", action="append", default=[])
    managed_run.add_argument("command", nargs=argparse.REMAINDER)

    runtime_parser = domains.add_parser("runtime")
    runtime_actions = runtime_parser.add_subparsers(dest="action", required=True)
    runtime_actions.add_parser("audit")
    runtime_run = runtime_actions.add_parser("run")
    runtime_run.add_argument("--producer", required=True)
    runtime_run.add_argument("--profile", required=True)
    runtime_run.add_argument("--minimum-memory-mib", type=int)
    runtime_run.add_argument("--timeout", type=int)
    runtime_run.add_argument("--export-path", action="append", default=[])
    runtime_run.add_argument("command", nargs=argparse.REMAINDER)
    runtime_wsl = runtime_actions.add_parser("wsl")
    runtime_wsl_actions = runtime_wsl.add_subparsers(dest="wsl_action", required=True)
    runtime_wsl_provision = runtime_wsl_actions.add_parser("provision")
    runtime_wsl_provision.add_argument("--source-distro", default="Ubuntu")
    runtime_wsl_actions.add_parser("verify")
    runtime_wsl_run = runtime_wsl_actions.add_parser("run")
    runtime_wsl_run.add_argument("--entry", required=True)
    runtime_wsl_run.add_argument("--minimum-memory-mib", type=int)
    runtime_wsl_run.add_argument("arguments", nargs=argparse.REMAINDER)

    toolchain = domains.add_parser("toolchain")
    toolchain_actions = toolchain.add_subparsers(dest="action", required=True)
    toolchain_actions.add_parser("check")
    toolchain_actions.add_parser("sync")

    deps = domains.add_parser("deps")
    deps_actions = deps.add_subparsers(dest="action", required=True)
    deps_actions.add_parser("import-lock")
    install = deps_actions.add_parser("install")
    install.add_argument("--clean-links", action="store_true")
    update = deps_actions.add_parser("update")
    update.add_argument("--manager", choices=("pnpm", "cargo"), required=True)
    update.add_argument("arguments", nargs=argparse.REMAINDER)
    deps_actions.add_parser("verify")

    package = domains.add_parser("package")
    package_actions = package.add_subparsers(dest="action", required=True)
    pack = package_actions.add_parser("pack")
    pack.add_argument("--package", required=True)
    publish = package_actions.add_parser("publish")
    publish.add_argument("--receipt", required=True)
    publish.add_argument("--tag", default="latest")
    publish.add_argument("--access", choices=("public", "restricted"), default="public")
    publish.add_argument("--apply", action="store_true")

    git_parser = domains.add_parser("git")
    git_actions = git_parser.add_subparsers(dest="action", required=True)
    inventory = git_actions.add_parser("inventory")
    inventory.add_argument("--fetch", action="store_true")
    converge = git_actions.add_parser("converge")
    converge.add_argument("--apply", action="store_true")
    converge.add_argument("--safety-receipt")
    converge.add_argument("--candidate")
    review = git_actions.add_parser("review")
    review.add_argument("--safety-receipt", required=True)
    review.add_argument("--candidate", required=True)
    review.add_argument("--record-worktree")
    review.add_argument("--evidence-commit")
    decision_target = review.add_mutually_exclusive_group()
    decision_target.add_argument("--decide-ref")
    decision_target.add_argument("--decide-worktree")
    review.add_argument("--decision", choices=("selected", "excluded"))
    review.add_argument("--reason")
    upload = git_actions.add_parser("upload")
    upload.add_argument("--candidate", required=True)
    upload.add_argument("--safety-receipt", required=True)
    upload.add_argument(
        "--commit",
        help=(
            "exact detached HEAD SHA to upload when the local candidate branch is "
            "checked out by a protected worktree"
        ),
    )
    check = git_actions.add_parser("check")
    check.add_argument("--candidate", required=True)
    promote = git_actions.add_parser("promote")
    promote.add_argument("--candidate", required=True)
    promote.add_argument("--safety-receipt", required=True)
    protect = git_actions.add_parser("protect")
    protect.add_argument("--candidate", required=True)
    protect.add_argument("--safety-receipt", required=True)
    finalize = git_actions.add_parser("finalize")
    finalize.add_argument("--candidate", required=True)
    finalize.add_argument("--safety-receipt", required=True)
    finalize.add_argument("--promotion-receipt", required=True)
    finalize.add_argument("--apply", action="store_true")
    return root


def storage_clean(arguments: argparse.Namespace, policy: dict[str, Any]) -> int:
    receipt_path = pathlib.Path(arguments.receipt).resolve(strict=True)
    if not is_within(receipt_path, state_root() / "receipts"):
        raise ManagementError("cleanup accepts only a managed receipt")
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    owned = [pathlib.Path(item) for item in receipt.get("owned_paths", [])]
    for path in owned:
        assert_output_path(path, policy)
    if not arguments.apply:
        print_json({"action": "dry-run", "owned_paths": [str(path) for path in owned]})
        return 0
    for path in owned:
        remove_owned_path(path, policy)
    print_json({"action": "removed", "owned_paths": [str(path) for path in owned]})
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    requested_argv = list(argv if argv is not None else sys.argv[1:])
    arguments = parser().parse_args(requested_argv)
    policy = load_policy()
    if (
        management_command_profile(arguments, policy) is not None
        and os.environ.get("CLEARRA_RUNTIME_SUPERVISED") != "1"
    ):
        return run_management_command_supervised(requested_argv, arguments, policy)
    if arguments.domain == "storage":
        if arguments.action == "audit":
            audit = storage_audit(policy)
            audit["receipt"] = str(write_receipt("storage-audit", audit))
            print_json(audit)
            return 0
        if arguments.action == "verify":
            return storage_verify(arguments, policy)
        if arguments.action == "run":
            return storage_run(arguments, policy)
        if arguments.action == "clean":
            return storage_clean(arguments, policy)
    if arguments.domain == "runtime":
        if arguments.action == "audit":
            return runtime_audit_command(policy)
        if arguments.action == "run":
            return storage_run(arguments, policy)
        if arguments.action == "wsl":
            return runtime_wsl_command(arguments, policy)
    if arguments.domain == "toolchain":
        return toolchain_check(policy) if arguments.action == "check" else toolchain_sync(policy)
    if arguments.domain == "deps":
        return deps_command(
            arguments.action,
            policy,
            clean_links=getattr(arguments, "clean_links", False),
            update_manager=getattr(arguments, "manager", None),
            update_arguments=getattr(arguments, "arguments", ()),
        )
    if arguments.domain == "package":
        if arguments.action == "pack":
            print_json(package_pack(arguments.package, policy))
            return 0
        if arguments.action == "publish":
            print_json(
                package_publish(
                    arguments.receipt,
                    policy,
                    tag=arguments.tag,
                    access=arguments.access,
                    apply=arguments.apply,
                )
            )
            return 0
    if arguments.domain == "git":
        with repository_lock():
            if arguments.action == "inventory":
                inventory = git_inventory(policy, fetch=arguments.fetch)
                path = write_receipt("git-inventory", inventory)
                print_json({**inventory, "receipt": str(path)})
                return 0
            if arguments.action == "converge":
                if arguments.apply:
                    if not arguments.safety_receipt or not arguments.candidate:
                        raise ManagementError(
                            "git converge --apply requires --safety-receipt and --candidate"
                        )
                    print_json(
                        apply_convergence_review(
                            arguments.safety_receipt, arguments.candidate, policy
                        )
                    )
                else:
                    if arguments.safety_receipt or arguments.candidate:
                        raise ManagementError(
                            "--safety-receipt and --candidate require --apply"
                        )
                    print_json(prepare_git_safety(policy))
                return 0
            if arguments.action == "review":
                if bool(arguments.record_worktree) != bool(arguments.evidence_commit):
                    raise ManagementError(
                        "--record-worktree and --evidence-commit must be supplied together"
                    )
                deciding = bool(arguments.decide_ref or arguments.decide_worktree)
                if deciding != bool(arguments.decision and arguments.reason):
                    raise ManagementError(
                        "a review decision requires one target, --decision, and --reason"
                    )
                evidence = None
                if arguments.record_worktree:
                    evidence = record_dirty_candidate_evidence(
                        arguments.safety_receipt,
                        arguments.record_worktree,
                        arguments.evidence_commit,
                        policy,
                    )
                decision = None
                if deciding:
                    decision = record_review_decision(
                        arguments.safety_receipt,
                        policy,
                        decision=arguments.decision,
                        reason=arguments.reason,
                        ref=arguments.decide_ref,
                        worktree=arguments.decide_worktree,
                    )
                summary = git_review_summary(
                    arguments.safety_receipt, arguments.candidate, policy
                )
                print_json({"evidence": evidence, "decision": decision, **summary})
                return 0
            if arguments.action == "upload":
                print_json(
                    upload_candidate(
                        arguments.candidate,
                        arguments.safety_receipt,
                        policy,
                        exact_commit=arguments.commit,
                    )
                )
                return 0
            if arguments.action == "check":
                sha, checks = candidate_check_status_once(arguments.candidate, policy)
                print_json(
                    {
                        "schema_id": "clearra.git-candidate-check-once.v1",
                        "candidate": arguments.candidate,
                        "sha": sha,
                        "checks": checks,
                        "queried_once": True,
                    }
                )
                return 0
            if arguments.action == "promote":
                print_json(
                    promote_candidate(arguments.candidate, arguments.safety_receipt, policy)
                )
                return 0
            if arguments.action == "protect":
                sha, checks = check_candidate_once(arguments.candidate, policy)
                convergence = validate_convergence_review(
                    arguments.safety_receipt, sha, policy
                )
                default_main_preflight(sha)
                print_json(
                    {
                        "sha": sha,
                        "checks": checks,
                        "convergence": convergence,
                        "ruleset": apply_ruleset(policy),
                    }
                )
                return 0
            if arguments.action == "finalize":
                print_json(
                    finalize_candidate(
                        arguments.candidate,
                        arguments.safety_receipt,
                        arguments.promotion_receipt,
                        policy,
                        apply=arguments.apply,
                    )
                )
                return 0
    raise ManagementError("unreachable management command")


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ManagementError, runtime_owner.RuntimePolicyError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(2)
