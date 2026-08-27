#!/usr/bin/python3

"""Trusted Clearra immutable-release tree digest.

This helper intentionally has no imports from the candidate release.  It
implements the clearra-release-tree-v1 byte contract used by the tracked Node
validator and rejects dangling, escaping, or unsupported filesystem entries.
"""

from __future__ import annotations

import hashlib
import os
import stat
import sys


def fail() -> "NoReturn":
    print("release_tree_digest=failed", file=sys.stderr)
    raise SystemExit(2)


def release_tree_sha256(root_argument: str) -> str:
    root = os.path.realpath(os.path.abspath(root_argument))
    root_stat = os.lstat(root)
    if not stat.S_ISDIR(root_stat.st_mode):
        raise ValueError("root is not a directory")

    entries: list[tuple[str, str, str | None]] = []

    def collect(directory: str) -> None:
        with os.scandir(directory) as children:
            ordered = sorted(children, key=lambda child: child.name)
        for child in ordered:
            absolute_path = os.path.join(directory, child.name)
            relative_path = os.path.relpath(absolute_path, root).replace(os.sep, "/")
            metadata = os.lstat(absolute_path)
            if stat.S_ISLNK(metadata.st_mode):
                if not os.path.exists(absolute_path):
                    raise ValueError("dangling symlink")
                resolved_target = os.path.realpath(absolute_path)
                if os.path.commonpath((root, resolved_target)) != root:
                    raise ValueError("escaping symlink")
                entries.append(("symlink", relative_path, os.readlink(absolute_path)))
            elif stat.S_ISDIR(metadata.st_mode):
                entries.append(("directory", relative_path, None))
                collect(absolute_path)
            elif stat.S_ISREG(metadata.st_mode):
                entries.append(("file", relative_path, absolute_path))
            else:
                raise ValueError("unsupported entry")

    collect(root)
    entries.sort(key=lambda entry: entry[1])
    digest = hashlib.sha256()
    digest.update(b"clearra-release-tree-v1\0")
    for entry_type, relative_path, value in entries:
        digest.update(entry_type.encode("utf-8"))
        digest.update(b"\0")
        digest.update(relative_path.encode("utf-8"))
        digest.update(b"\0")
        if entry_type == "file":
            assert value is not None
            with open(value, "rb") as source:
                contents = source.read()
            digest.update(str(len(contents)).encode("ascii"))
            digest.update(b"\0")
            digest.update(contents)
        elif entry_type == "symlink":
            assert value is not None
            digest.update(value.encode("utf-8"))
        digest.update(b"\0")
    return digest.hexdigest()


def main() -> None:
    if len(sys.argv) != 2:
        fail()
    try:
        print(release_tree_sha256(sys.argv[1]))
    except (OSError, UnicodeError, ValueError):
        fail()


if __name__ == "__main__":
    main()
