#!/usr/bin/env python3

import hashlib
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent
TARGETS = (
    ROOT / "clearra-oracle-freeze-v080",
    ROOT / "clearra-oracle-inactive-stage-v080.template",
)
FRAGMENT = re.compile(
    r"/usr/bin/python3 - \"\$[^\"]+\" \"\$[^\"]+\" \"\$[^\"]+\" <<'PY'\n"
    r"(?P<body>import hashlib\n.*?\n)PY\n",
    re.DOTALL,
)
CLEANUP_FUNCTION = re.compile(
    r"(cleanup_runtime_root\(\) \{\n.*?\n\})\n",
    re.DOTALL,
)
GOOD = b"private-overlay-fixture\n"
GOOD_SHA = hashlib.sha256(GOOD).hexdigest()


def extract(target: Path, output: Path) -> None:
    match = FRAGMENT.search(target.read_text(encoding="utf-8"))
    if not match:
        raise AssertionError(f"sealed copy fragment missing: {target.name}")
    output.write_text(match.group("body"), encoding="utf-8", newline="\n")


def fixture(base: Path):
    sealed = base / "sealed"
    destination_root = base / "destination"
    sealed.mkdir(mode=0o700, parents=True)
    destination_root.mkdir(mode=0o700)
    source = sealed / f"private-overlay-no-config-{GOOD_SHA}.tar"
    source.write_bytes(GOOD)
    source.chmod(0o600)
    return sealed, source, destination_root / "overlay.tar"


def invoke(program: Path, sealed: Path, source: Path, destination: Path, **extra):
    environment = os.environ.copy()
    environment["CLEARRA_REMOTE_OVERLAY_TEST_ROOT"] = str(sealed)
    environment.update(extra)
    return subprocess.run(
        [sys.executable, str(program), str(source), GOOD_SHA, str(destination)],
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def rejected(result, label):
    if result.returncode == 0:
        raise AssertionError(f"accepted {label}")


def run_target(target: Path, program: Path, base: Path) -> None:
    sealed, source, destination = fixture(base / "success")
    result = invoke(program, sealed, source, destination)
    if result.returncode != 0 or destination.read_bytes() != GOOD:
        raise AssertionError(f"valid copy failed for {target.name}: {result.stderr!r}")
    metadata = destination.stat()
    if stat.S_IMODE(metadata.st_mode) != 0o600 or metadata.st_nlink != 1:
        raise AssertionError("copied metadata drifted")

    sealed, source, destination = fixture(base / "symlink-source")
    real = source.with_suffix(".real")
    source.rename(real)
    source.symlink_to(real.name)
    rejected(invoke(program, sealed, source, destination), "source symlink")

    sealed, source, destination = fixture(base / "hardlink")
    os.link(source, source.with_suffix(".hardlink"))
    rejected(invoke(program, sealed, source, destination), "source hardlink")

    sealed, source, destination = fixture(base / "mode")
    source.chmod(0o640)
    rejected(invoke(program, sealed, source, destination), "wrong source mode")

    sealed, source, destination = fixture(base / "hash")
    source.write_bytes(b"wrong")
    rejected(invoke(program, sealed, source, destination), "wrong source hash")

    sealed, source, destination = fixture(base / "path")
    drifted = sealed / f"private-overlay-no-config-{'0' * 64}.tar"
    source.rename(drifted)
    rejected(invoke(program, sealed, drifted, destination), "canonical filename drift")

    sealed, source, destination = fixture(base / "writable-parent")
    sealed.chmod(0o770)
    rejected(invoke(program, sealed, source, destination), "writable parent")

    real_parent = base / "real-parent"
    sealed, source, destination = fixture(real_parent)
    linked_parent = base / "linked-parent"
    linked_parent.symlink_to(sealed, target_is_directory=True)
    linked_source = linked_parent / source.name
    rejected(invoke(program, linked_parent, linked_source, destination), "symlink parent")

    sealed, source, destination = fixture(base / "exclusive")
    destination.write_bytes(b"occupied")
    rejected(invoke(program, sealed, source, destination), "occupied O_EXCL destination")
    if destination.read_bytes() != b"occupied":
        raise AssertionError("O_EXCL failure modified existing destination")

    sealed, source, destination = fixture(base / "swap")
    (Path(str(source) + ".swap")).write_bytes(b"replacement")
    Path(str(source) + ".swap").chmod(0o600)
    result = invoke(
        program, sealed, source, destination,
        CLEARRA_REMOTE_OVERLAY_TEST_SWAP="1",
    )
    if result.returncode != 0 or destination.read_bytes() != GOOD or source.read_bytes() == GOOD:
        raise AssertionError("source path swap was not bound to the opened fd")

    sealed, source, destination = fixture(base / "cleanup")
    result = invoke(
        program, sealed, source, destination,
        CLEARRA_REMOTE_OVERLAY_TEST_CORRUPT_COPY="1",
    )
    rejected(result, "post-fsync destination drift")
    if destination.exists() or destination.is_symlink():
        raise AssertionError("failed copy left secret-bearing destination")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="clearra-overlay-copy-test-") as temporary:
        temporary_root = Path(temporary)
        temporary_root.chmod(0o700)
        for index, target in enumerate(TARGETS):
            program = temporary_root / f"fragment-{index}.py"
            extract(target, program)
            run_target(target, program, temporary_root / f"target-{index}")
        template = TARGETS[1].read_text(encoding="utf-8")
        cleanup_match = CLEANUP_FUNCTION.search(template)
        if not cleanup_match:
            raise AssertionError("inactive-stage cleanup function missing")
        cleanup_base = temporary_root / "post-copy-cleanup"
        roots = [cleanup_base / name for name in ("input", "stage", "upload")]
        for root in roots:
            root.mkdir(parents=True, mode=0o700)
        (roots[0] / "private-overlay-no-config.tar").write_bytes(GOOD)
        cleanup_script = temporary_root / "cleanup.sh"
        calls = "\n".join(
            f"cleanup_runtime_root '{root}' '{root}'" for root in roots
        )
        cleanup_script.write_text(
            "#!/bin/sh\nset -eu\n" + cleanup_match.group(1) + "\n" + calls + "\n",
            encoding="utf-8",
            newline="\n",
        )
        result = subprocess.run(
            ["/usr/bin/dash", str(cleanup_script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0 or any(root.exists() for root in roots):
            raise AssertionError("post-copy failure cleanup left runtime residue")
    print("oracle_remote_overlay_copy_test=pass")


if __name__ == "__main__":
    main()
