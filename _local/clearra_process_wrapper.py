#!/usr/bin/env python3
"""POSIX lease owner for one Clearra command tree.

The host supervisor keeps this process's stdin open for the lifetime of the
managed command.  If that lease disappears, or the supervisor itself dies,
the wrapper terminates every descendant before it exits.  The wrapper is not
used on Windows, where a kill-on-close Job Object owns the same lifetime.
"""

from __future__ import annotations

import argparse
import contextlib
import ctypes
import os
import pathlib
import signal
import subprocess
import sys
import threading
import time
from collections.abc import Sequence


PR_SET_PDEATHSIG = 1
PR_SET_CHILD_SUBREAPER = 36


def _set_prctl(option: int, value: int) -> None:
    libc = ctypes.CDLL(None, use_errno=True)
    prctl = libc.prctl
    prctl.argtypes = [ctypes.c_int, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
    prctl.restype = ctypes.c_int
    if prctl(option, value, 0, 0, 0) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))


def _pid_alive(pid: int) -> bool:
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _read_exact_fd(file_descriptor: int, size: int) -> bytes:
    chunks: list[bytes] = []
    remaining = size
    while remaining:
        try:
            chunk = os.read(file_descriptor, remaining)
        except InterruptedError:
            continue
        if not chunk:
            raise EOFError("runtime lease ended before stdin payload was complete")
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def _proc_children() -> dict[int, list[int]]:
    children: dict[int, list[int]] = {}
    proc = pathlib.Path("/proc")
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            material = (entry / "stat").read_text(encoding="ascii")
            close = material.rfind(")")
            if close < 0:
                continue
            pid = int(material[: material.find(" ")])
            remainder = material[close + 2 :].split()
            parent = int(remainder[1])
        except (OSError, ValueError, IndexError):
            continue
        children.setdefault(parent, []).append(pid)
    return children


def _descendants(root: int) -> set[int]:
    by_parent = _proc_children()
    pending = list(by_parent.get(root, ()))
    found: set[int] = set()
    while pending:
        pid = pending.pop()
        if pid in found:
            continue
        found.add(pid)
        pending.extend(by_parent.get(pid, ()))
    return found


def _signal_pids(pids: set[int], requested_signal: int) -> None:
    for pid in sorted(pids, reverse=True):
        with contextlib.suppress(ProcessLookupError, PermissionError):
            os.kill(pid, requested_signal)


def _terminate_descendants(command: subprocess.Popen[bytes], grace: float) -> str:
    with contextlib.suppress(ProcessLookupError):
        os.killpg(command.pid, signal.SIGTERM)
    _signal_pids(_descendants(os.getpid()), signal.SIGTERM)
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline:
        if not _descendants(os.getpid()):
            return "term"
        time.sleep(0.05)
    with contextlib.suppress(ProcessLookupError):
        os.killpg(command.pid, signal.SIGKILL)
    _signal_pids(_descendants(os.getpid()), signal.SIGKILL)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try:
            waited, _ = os.waitpid(-1, os.WNOHANG)
        except ChildProcessError:
            break
        if waited == 0:
            if not _descendants(os.getpid()):
                break
            time.sleep(0.05)
    return "kill"


def _move_and_destroy_owned_cgroup(
    owned_cgroup: pathlib.Path | None,
    parent_cgroup: pathlib.Path | None,
) -> bool:
    """Move the lease owner out, then destroy only its dedicated cgroup."""
    if owned_cgroup is None or parent_cgroup is None:
        return False
    try:
        (parent_cgroup / "cgroup.procs").write_text(str(os.getpid()), encoding="ascii")
    except OSError:
        return False
    kill_file = owned_cgroup / "cgroup.kill"
    if kill_file.is_file():
        with contextlib.suppress(OSError):
            kill_file.write_text("1", encoding="ascii")
    else:
        try:
            pids = {
                int(value)
                for value in (owned_cgroup / "cgroup.procs").read_text(encoding="ascii").split()
            }
        except (OSError, ValueError):
            pids = set()
        _signal_pids(pids, signal.SIGKILL)
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try:
            if not (owned_cgroup / "cgroup.procs").read_text(encoding="ascii").strip():
                break
        except OSError:
            break
        time.sleep(0.05)
    with contextlib.suppress(OSError):
        owned_cgroup.rmdir()
    return not owned_cgroup.exists()


def _parse(arguments: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument("--parent-pid", required=True, type=int)
    parser.add_argument("--grace-seconds", required=True, type=float)
    parser.add_argument("--input-size", required=True, type=int)
    parser.add_argument("--parent-cgroup")
    parser.add_argument("--owned-cgroup")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    result = parser.parse_args(arguments)
    if result.command and result.command[0] == "--":
        result.command = result.command[1:]
    if not result.command:
        parser.error("a command is required after --")
    if result.input_size < 0:
        parser.error("--input-size cannot be negative")
    if result.grace_seconds < 0:
        parser.error("--grace-seconds cannot be negative")
    return result


def main(arguments: Sequence[str] | None = None) -> int:
    options = _parse(list(arguments if arguments is not None else sys.argv[1:]))
    parent_pid = int(options.parent_pid)
    lease_lost = threading.Event()

    def mark_lease_lost(_signal: int, _frame: object) -> None:
        lease_lost.set()

    signal.signal(signal.SIGTERM, mark_lease_lost)
    signal.signal(signal.SIGINT, mark_lease_lost)
    signal.signal(signal.SIGHUP, mark_lease_lost)
    _set_prctl(PR_SET_CHILD_SUBREAPER, 1)
    _set_prctl(PR_SET_PDEATHSIG, signal.SIGTERM)
    if os.getppid() != parent_pid or not _pid_alive(parent_pid):
        lease_lost.set()

    stdin_file_descriptor = sys.stdin.fileno()
    try:
        input_payload = _read_exact_fd(stdin_file_descriptor, int(options.input_size))
    except EOFError:
        return 125
    if lease_lost.is_set() or os.getppid() != parent_pid or not _pid_alive(parent_pid):
        return 125

    command = subprocess.Popen(
        list(options.command),
        stdin=subprocess.PIPE if input_payload else subprocess.DEVNULL,
        stdout=None,
        stderr=None,
        start_new_session=True,
        shell=False,
    )
    if input_payload:
        assert command.stdin is not None
        try:
            command.stdin.write(input_payload)
            command.stdin.close()
        except BrokenPipeError:
            pass

    os.set_blocking(stdin_file_descriptor, False)
    while command.poll() is None and not lease_lost.is_set():
        try:
            if os.read(stdin_file_descriptor, 4096) == b"":
                lease_lost.set()
                break
        except BlockingIOError:
            pass
        except InterruptedError:
            continue
        except OSError:
            lease_lost.set()
            break
        lease_lost.wait(0.1)

    if lease_lost.is_set() and command.poll() is None:
        parent_gone = os.getppid() != parent_pid or not _pid_alive(parent_pid)
        owned = pathlib.Path(options.owned_cgroup) if options.owned_cgroup else None
        parent = pathlib.Path(options.parent_cgroup) if options.parent_cgroup else None
        if parent_gone and _move_and_destroy_owned_cgroup(owned, parent):
            return 125
        _terminate_descendants(command, float(options.grace_seconds))
        return 125

    returncode = command.wait()
    # A command may daemonize before its direct parent exits.  No descendant
    # is allowed to outlive the bounded operation.
    if _descendants(os.getpid()):
        _terminate_descendants(command, float(options.grace_seconds))
    return returncode if returncode >= 0 else 128 + abs(returncode)


if __name__ == "__main__":
    raise SystemExit(main())
