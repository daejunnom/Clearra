#!/usr/bin/env python3
"""Prove that the npm-to-pnpm lock migration preserved resolved packages."""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parents[2]
NPM_LOCK = ROOT / "package-lock.json"
PNPM_LOCK = ROOT / "pnpm-lock.yaml"
RECEIPT = ROOT / "config" / "pnpm-import-baseline.v1.json"


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def npm_graph() -> set[tuple[str, str, str]]:
    lock = json.loads(NPM_LOCK.read_text(encoding="utf-8"))
    result: set[tuple[str, str, str]] = set()
    for path, package in lock["packages"].items():
        if "node_modules/" not in path or not package.get("version") or not package.get("integrity"):
            continue
        name = path.rsplit("node_modules/", 1)[1]
        result.add((name, package["version"], package["integrity"]))
    return result


def pnpm_graph() -> set[tuple[str, str, str]]:
    text = PNPM_LOCK.read_text(encoding="utf-8")
    try:
        packages = text.split("\npackages:\n", 1)[1].split("\nsnapshots:\n", 1)[0]
    except IndexError as error:
        raise RuntimeError("unsupported pnpm lock structure") from error
    result: set[tuple[str, str, str]] = set()
    current: str | None = None
    for line in packages.splitlines():
        key = re.match(r"^  ['\"]?(.+?)['\"]?:$", line)
        if key:
            current = key.group(1)
            continue
        resolution = re.match(r"^    resolution: \{integrity: (.+)\}$", line)
        if resolution and current:
            split = current.rfind("@")
            if split <= 0:
                raise RuntimeError(f"invalid pnpm package identity: {current}")
            result.add((current[:split], current[split + 1 :], resolution.group(1)))
            current = None
    return result


def graph_digest(graph: set[tuple[str, str, str]]) -> str:
    material = "\n".join("\0".join(item) for item in sorted(graph)).encode()
    return hashlib.sha256(material).hexdigest()


def main() -> int:
    if not PNPM_LOCK.is_file():
        raise RuntimeError("pnpm-lock.yaml is required for import proof")
    if not NPM_LOCK.is_file():
        baseline = json.loads(RECEIPT.read_text(encoding="utf-8"))
        pnpm = pnpm_graph()
        verification = {
            "schema_id": "clearra.pnpm-import-verification.v1",
            "package_lock_sha256": baseline["package_lock_sha256"],
            "pnpm_lock_sha256": digest(PNPM_LOCK),
            "pnpm_resolved_count": len(pnpm),
            "resolved_graph_sha256": graph_digest(pnpm),
        }
        verification["accepted"] = (
            baseline.get("accepted") is True
            and verification["pnpm_lock_sha256"]
            == baseline.get("validated_pnpm_lock_sha256", baseline["pnpm_lock_sha256"])
            and verification["pnpm_resolved_count"] == baseline["pnpm_resolved_count"]
            and verification["resolved_graph_sha256"] == baseline["resolved_graph_sha256"]
        )
        print(json.dumps(verification, indent=2))
        return 0 if verification["accepted"] else 1
    npm = npm_graph()
    pnpm = pnpm_graph()
    missing = sorted(npm - pnpm)
    added = sorted(pnpm - npm)
    receipt = {
        "schema_id": "clearra.pnpm-import-baseline.v1",
        "package_lock_sha256": digest(NPM_LOCK),
        "pnpm_lock_sha256": digest(PNPM_LOCK),
        "npm_resolved_count": len(npm),
        "pnpm_resolved_count": len(pnpm),
        "resolved_graph_sha256": graph_digest(npm),
        "missing": [{"name": n, "version": v, "integrity": i} for n, v, i in missing],
        "added": [{"name": n, "version": v, "integrity": i} for n, v, i in added],
        "accepted": not missing and not added,
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, indent=2))
    return 0 if receipt["accepted"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(2)
