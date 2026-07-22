import argparse
import base64
import fnmatch
import hashlib
import json
import os
import re
import shutil
import subprocess
from datetime import datetime
from pathlib import Path, PurePosixPath

PROJECT_NAME = "Clearra"
BUNDLE_SCHEMA_VERSION = 2
OUTPUT_FILE_NAME = "project_bundle.txt"
MAX_FILE_SIZE = 2_000_000

CMAKE_SOURCE_MANIFEST = "core-c/cmake/source_manifest.cmake"
CMAKE_TEST_MANIFEST = "core-c/cmake/test_targets.cmake"
CMAKE_ROOT_FILE = "core-c/CMakeLists.txt"
REQUIRED_COVERAGE_SOURCES = {
    "src/coverage/pattern_bitset_c.c",
    "src/coverage/coverage_row_builder.c",
    "src/coverage/coverage_union.c",
    "src/coverage/coverage_overlap.c",
}
RUST_SOURCE_ROOTS = (
    "crates",
    "apps/clearra-desktop/src-tauri",
)

CMAKE_FILE_REFERENCE = re.compile(
    r"(?<![A-Za-z0-9_./+-])"
    r"((?:src|tests)/[A-Za-z0-9_./+-]+\.(?:c|cc|cpp|cxx|h|hpp))"
)
CMAKE_LOCAL_INCLUDE = re.compile(r"\binclude\s*\(\s*([^\s\)]+)")
RUST_PATH_MODULE = re.compile(
    r"#\s*\[\s*path\s*=\s*\"([^\"]+)\"\s*\]\s*"
    r"(?:#\s*\[[^\]]+\]\s*)*"
    r"(?:pub(?:\s*\([^\)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
    re.MULTILINE,
)
RUST_PLAIN_MODULE = re.compile(
    r"^\s*(?:pub(?:\s*\([^\)]*\))?\s+)?"
    r"mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;",
    re.MULTILINE,
)
RUST_LITERAL_INCLUDE = re.compile(
    r"\binclude(?:_str|_bytes)?!\s*\(\s*\"([^\"]+)\"\s*\)",
    re.MULTILINE,
)
RUST_ANY_INCLUDE = re.compile(r"\binclude(?:_str|_bytes)?!\s*\(")

# Useful review metadata that is safe to include even though it starts with a dot.
ALWAYS_INCLUDE = {
    ".dockerignore",
    ".gitignore",
}

# These are local tooling, dependency snapshots, or planning handoff material rather
# than product source. They stay in the repository but not in a review bundle.
BUNDLE_EXCLUDE_ONLY = {
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Clearra 파일 구조.md",
    "Clearra 프로그래밍 원칙.md",
    "Clearra 핸드오프.md",
    "Veya_implementation_audit_summary.md",
}

BUNDLE_EXCLUDE_PATTERNS = {
    "*.log",
    "*.temp",
    "*.tmp",
    "*.tsbuildinfo",
    "*.pyc",
    "*.pyo",
    ".DS_Store",
    "Thumbs.db",
    "npm-debug.log*",
    "pnpm-debug.log*",
    "yarn-debug.log*",
    "yarn-error.log*",
    "project_bundle.txt",
    "package-bundle.txt",
}

BINARY_EXTENSIONS = {
    ".7z",
    ".a",
    ".avif",
    ".class",
    ".dll",
    ".dylib",
    ".exe",
    ".flac",
    ".fumen",
    ".gif",
    ".gz",
    ".ico",
    ".icns",
    ".jar",
    ".jpeg",
    ".jpg",
    ".lib",
    ".m4a",
    ".mp3",
    ".o",
    ".obj",
    ".opus",
    ".pdb",
    ".pdf",
    ".png",
    ".rar",
    ".rlib",
    ".rmeta",
    ".so",
    ".svg",
    ".tar",
    ".tgz",
    ".wasm",
    ".wav",
    ".webp",
    ".woff",
    ".woff2",
    ".zip",
}

# A matching directory sequence is excluded wherever it appears in the tree.
SKIP_DIRS = {
    ".agents",
    ".cache",
    ".firebase",
    ".git",
    ".github/actions-cache",
    ".mypy_cache",
    ".next",
    ".nuxt",
    ".output",
    ".pnpm-store",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".turbo",
    ".venv",
    ".vite",
    "__pycache__",
    "blob-report",
    "build",
    "checkpoints",
    "coverage",
    "dist",
    "dist-server",
    "docs/history",
    "logs",
    "models",
    "node_modules",
    "package",
    "playwright-report",
    "research-data",
    "storybook-static",
    "target",
    "test-results",
    "vendor",
    "venv",
}

SECRET_FILE_NAMES = {
    ".netrc",
    ".npmrc",
    ".pypirc",
    "credentials.json",
    "id_dsa",
    "id_ed25519",
    "id_ecdsa",
    "id_rsa",
    "service-account.json",
    "service_account.json",
}

SECRET_FILE_PATTERNS = {
    "*credential*.json",
    "*service-account*.json",
    "*service_account*.json",
    "*.key",
    "*.p12",
    "*.pem",
    "*.pfx",
}


def find_git_executable() -> str:
    env_git = os.environ.get("GIT_EXE")
    if env_git:
        return env_git

    path_git = shutil.which("git")
    if path_git:
        return path_git

    bundled_git = (
        Path.home()
        / ".cache"
        / "codex-runtimes"
        / "codex-primary-runtime"
        / "dependencies"
        / "native"
        / "git"
        / "cmd"
        / "git.exe"
    )
    if bundled_git.exists():
        return str(bundled_git)

    return "git"


GIT_EXE = find_git_executable()
CARGO_EXE = shutil.which("cargo") or "cargo"


def find_repo_root() -> Path:
    try:
        raw = subprocess.check_output(
            [GIT_EXE, "rev-parse", "--show-toplevel"],
            stderr=subprocess.DEVNULL,
        ).decode("utf-8", errors="replace").strip()
        return Path(raw).resolve()
    except (FileNotFoundError, subprocess.CalledProcessError):
        return Path(__file__).resolve().parents[1]


ROOT = find_repo_root()


class SourceTreeIntegrityError(RuntimeError):
    pass


def repository_relative_path(path: Path) -> str:
    try:
        relative = path.resolve().relative_to(ROOT.resolve())
    except ValueError as error:
        raise SourceTreeIntegrityError(
            f"Referenced path escapes the repository: {path}"
        ) from error
    return PurePosixPath(relative).as_posix()


def require_repository_file(
    path: Path,
    purpose: str,
    required_paths: set[str],
    errors: list[str],
) -> str | None:
    try:
        relative = repository_relative_path(path)
    except SourceTreeIntegrityError as error:
        errors.append(f"{purpose}: {error}")
        return None
    if not path.exists() or not path.is_file():
        errors.append(f"{purpose} is missing: {relative}")
        return None
    if path.is_symlink():
        errors.append(f"{purpose} must not be a symlink: {relative}")
        return None
    if is_secret_path(path):
        errors.append(f"{purpose} resolves to a forbidden secret path: {relative}")
        return None
    required_paths.add(relative)
    return relative


def read_integrity_text(path: Path, purpose: str, errors: list[str]) -> str | None:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        errors.append(f"{purpose} could not be read as UTF-8: {path}: {error}")
        return None


def default_output_file() -> Path:
    configured = os.environ.get("CLEARRA_BUNDLE_OUTPUT")
    if configured:
        return Path(configured).expanduser().resolve()
    return (ROOT / "_local" / OUTPUT_FILE_NAME).resolve()


def run_git_bytes(args: list[str], input_bytes: bytes | None = None) -> bytes:
    return subprocess.check_output(
        [GIT_EXE, *args],
        cwd=ROOT,
        input=input_bytes,
        stderr=subprocess.STDOUT,
    )


def run_git_text(args: list[str]) -> str:
    return run_git_bytes(args).decode("utf-8", errors="replace")


def run_git_tracked_files() -> list[str]:
    raw = run_git_bytes(["ls-files", "--cached", "-z"])
    return unique_paths(
        part.decode("utf-8", errors="replace")
        for part in raw.split(b"\0")
        if part
    )


def run_git_status_short() -> str:
    return run_git_text(["status", "--short"])


def run_git_head() -> str:
    try:
        return run_git_text(["rev-parse", "--short=12", "HEAD"]).strip()
    except subprocess.CalledProcessError:
        return "unknown"


def git_status_summary(status_short: str) -> str:
    if not status_short.strip():
        return "clean"
    counts: dict[str, int] = {}
    for line in status_short.splitlines():
        if len(line) < 2:
            continue
        code = line[:2]
        counts[code] = counts.get(code, 0) + 1
    return ", ".join(f"{code}:{count}" for code, count in sorted(counts.items()))


def unique_paths(paths) -> list[str]:
    return list(dict.fromkeys(path for path in paths if path))


def contains_skipped_dir(path_parts: tuple[str, ...]) -> bool:
    for skip_dir in SKIP_DIRS:
        skip_parts = PurePosixPath(skip_dir).parts
        width = len(skip_parts)
        if width == 0:
            continue
        if any(
            path_parts[index : index + width] == skip_parts
            for index in range(len(path_parts) - width + 1)
        ):
            return True
    return False


def should_skip_dir(relative_dir: Path) -> bool:
    return contains_skipped_dir(PurePosixPath(relative_dir.as_posix()).parts)


def should_skip_candidate_path(file_path: str) -> bool:
    normalized = PurePosixPath(file_path.replace("\\", "/"))
    return contains_skipped_dir(normalized.parent.parts)


def iter_repository_files(relative_roots: tuple[str, ...], suffix: str):
    for relative_root in relative_roots:
        root = ROOT / relative_root
        if not root.exists() or not root.is_dir():
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            current = Path(dirpath)
            relative_dir = current.relative_to(ROOT)
            dirnames[:] = [
                dirname
                for dirname in dirnames
                if not should_skip_dir(relative_dir / dirname)
            ]
            for filename in filenames:
                if filename.endswith(suffix):
                    yield current / filename


def validate_cmake_references(
    required_paths: set[str], errors: list[str]
) -> dict[str, set[str]]:
    source_manifest = ROOT / CMAKE_SOURCE_MANIFEST
    test_manifest = ROOT / CMAKE_TEST_MANIFEST
    cmake_root = ROOT / CMAKE_ROOT_FILE
    for path, purpose in (
        (cmake_root, "CMake root file"),
        (source_manifest, "CMake source manifest"),
        (test_manifest, "CMake test manifest"),
    ):
        require_repository_file(path, purpose, required_paths, errors)

    source_text = read_integrity_text(
        source_manifest, "CMake source manifest", errors
    )
    test_text = read_integrity_text(test_manifest, "CMake test manifest", errors)
    root_text = read_integrity_text(cmake_root, "CMake root file", errors)
    source_references = set(CMAKE_FILE_REFERENCE.findall(source_text or ""))
    test_references = set(CMAKE_FILE_REFERENCE.findall(test_text or ""))

    missing_required_coverage = REQUIRED_COVERAGE_SOURCES - source_references
    for reference in sorted(missing_required_coverage):
        errors.append(
            "CMake source manifest dropped required coverage source: " + reference
        )

    for reference in sorted(source_references):
        require_repository_file(
            ROOT / "core-c" / reference,
            "CMake source reference",
            required_paths,
            errors,
        )
    for reference in sorted(test_references):
        require_repository_file(
            ROOT / "core-c" / reference,
            "CMake test reference",
            required_paths,
            errors,
        )

    for include_target in CMAKE_LOCAL_INCLUDE.findall(root_text or ""):
        token = include_target.strip('"\'')
        if "/" not in token and not token.endswith(".cmake"):
            continue
        require_repository_file(
            ROOT / "core-c" / token,
            "CMake local include",
            required_paths,
            errors,
        )
    return {
        "source": source_references,
        "test": test_references,
    }


def run_cargo_metadata(manifest_path: Path | None = None) -> dict:
    command = [CARGO_EXE, "metadata", "--no-deps", "--format-version", "1"]
    if manifest_path is not None:
        command.extend(["--manifest-path", str(manifest_path)])
    try:
        raw = subprocess.check_output(
            command,
            cwd=ROOT,
            stderr=subprocess.STDOUT,
        )
        return json.loads(raw.decode("utf-8"))
    except FileNotFoundError as error:
        raise SourceTreeIntegrityError(
            "Cargo is required to validate the Rust workspace source graph"
        ) from error
    except subprocess.CalledProcessError as error:
        detail = error.output.decode("utf-8", errors="replace")
        raise SourceTreeIntegrityError(
            f"cargo metadata rejected the source tree:\n{detail}"
        ) from error
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SourceTreeIntegrityError(
            f"cargo metadata returned invalid JSON: {error}"
        ) from error


def register_cargo_metadata(
    metadata: dict,
    required_paths: set[str],
    crate_roots: set[Path],
    errors: list[str],
    collect_workspace_members: bool,
) -> set[Path]:
    packages = {
        package.get("id"): package
        for package in metadata.get("packages", [])
        if isinstance(package, dict) and package.get("id")
    }
    workspace_member_ids = set(metadata.get("workspace_members", []))
    member_roots: set[Path] = set()
    for member_id in sorted(workspace_member_ids):
        if member_id not in packages:
            errors.append(f"Cargo workspace member has no package metadata: {member_id}")

    for package_id, package in packages.items():
        manifest_value = package.get("manifest_path")
        if not isinstance(manifest_value, str):
            errors.append(f"Cargo package has no manifest path: {package_id}")
            continue
        manifest = Path(manifest_value)
        require_repository_file(
            manifest, "Cargo package manifest", required_paths, errors
        )
        if collect_workspace_members and package_id in workspace_member_ids:
            member_roots.add(manifest.parent.resolve())

        for target in package.get("targets", []):
            source_value = target.get("src_path") if isinstance(target, dict) else None
            if not isinstance(source_value, str):
                errors.append(f"Cargo target has no source path in {manifest}")
                continue
            source = Path(source_value)
            relative = require_repository_file(
                source, "Cargo target source", required_paths, errors
            )
            if relative is not None:
                crate_roots.add(source.resolve())

        for dependency in package.get("dependencies", []):
            dependency_path = (
                dependency.get("path") if isinstance(dependency, dict) else None
            )
            if not isinstance(dependency_path, str):
                continue
            require_repository_file(
                Path(dependency_path) / "Cargo.toml",
                "Cargo path dependency manifest",
                required_paths,
                errors,
            )
    return member_roots


def validate_cargo_workspace(
    required_paths: set[str], errors: list[str]
) -> tuple[set[Path], set[Path]]:
    workspace_manifest = ROOT / "Cargo.toml"
    require_repository_file(
        workspace_manifest, "Cargo workspace manifest", required_paths, errors
    )
    crate_roots: set[Path] = set()
    try:
        workspace_metadata = run_cargo_metadata()
    except SourceTreeIntegrityError as error:
        errors.append(str(error))
        return set(), crate_roots
    member_roots = register_cargo_metadata(
        workspace_metadata,
        required_paths,
        crate_roots,
        errors,
        collect_workspace_members=True,
    )

    desktop_manifest = ROOT / "apps/clearra-desktop/src-tauri/Cargo.toml"
    if desktop_manifest.exists():
        try:
            desktop_metadata = run_cargo_metadata(desktop_manifest)
        except SourceTreeIntegrityError as error:
            errors.append(str(error))
        else:
            register_cargo_metadata(
                desktop_metadata,
                required_paths,
                crate_roots,
                errors,
                collect_workspace_members=False,
            )
    return member_roots, crate_roots


def rust_module_candidates(
    source: Path, module_name: str, crate_roots: set[Path]
) -> tuple[Path, Path]:
    resolved_source = source.resolve()
    if resolved_source in crate_roots or source.name in {"lib.rs", "main.rs", "mod.rs"}:
        module_base = source.parent
    else:
        module_base = source.parent / source.stem
    return (
        module_base / f"{module_name}.rs",
        module_base / module_name / "mod.rs",
    )


def validate_rust_module_graph(
    required_paths: set[str], crate_roots: set[Path], errors: list[str]
) -> set[str]:
    references: set[str] = set()
    for source in iter_repository_files(RUST_SOURCE_ROOTS, ".rs"):
        source_relative = require_repository_file(
            source, "Rust source", required_paths, errors
        )
        text = read_integrity_text(source, "Rust source", errors)
        if source_relative is None or text is None:
            continue

        path_modules: set[str] = set()
        for path_value, module_name in RUST_PATH_MODULE.findall(text):
            path_modules.add(module_name)
            target = source.parent / path_value
            relative = require_repository_file(
                target,
                f"Rust #[path] module referenced by {source_relative}",
                required_paths,
                errors,
            )
            if relative is not None:
                references.add(relative)

        for module_name in RUST_PLAIN_MODULE.findall(text):
            if module_name in path_modules:
                continue
            candidates = rust_module_candidates(source, module_name, crate_roots)
            existing = [candidate for candidate in candidates if candidate.is_file()]
            if not existing:
                expected = " or ".join(
                    repository_relative_path(candidate) for candidate in candidates
                )
                errors.append(
                    f"Rust module target is missing for {source_relative}: {expected}"
                )
                continue
            if len(existing) > 1:
                errors.append(
                    f"Rust module target is ambiguous for {source_relative}: "
                    + ", ".join(repository_relative_path(path) for path in existing)
                )
                continue
            relative = require_repository_file(
                existing[0],
                f"Rust module referenced by {source_relative}",
                required_paths,
                errors,
            )
            if relative is not None:
                references.add(relative)

        literal_includes = list(RUST_LITERAL_INCLUDE.finditer(text))
        literal_include_starts = {match.start() for match in literal_includes}
        for include_call in RUST_ANY_INCLUDE.finditer(text):
            if include_call.start() not in literal_include_starts:
                errors.append(
                    f"Rust include target is not a verifiable literal in {source_relative}"
                )
        for include_match in literal_includes:
            include_value = include_match.group(1)
            target = source.parent / include_value
            relative = require_repository_file(
                target,
                f"Rust include target referenced by {source_relative}",
                required_paths,
                errors,
            )
            if relative is not None:
                references.add(relative)
    return references


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def validate_asset_manifest_targets(
    required_paths: set[str], errors: list[str]
) -> set[str]:
    asset_root = ROOT / "assets"
    references: set[str] = set()
    manifests = sorted(asset_root.rglob("*.json")) if asset_root.exists() else []
    if not manifests:
        errors.append("Asset manifest is missing below assets/")
        return references

    for manifest in manifests:
        manifest_relative = require_repository_file(
            manifest, "Asset manifest", required_paths, errors
        )
        try:
            data = json.loads(manifest.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
            errors.append(f"Asset manifest could not be parsed: {manifest}: {error}")
            continue
        if manifest_relative is None:
            continue

        resolved_targets: dict[str, Path] = {}
        for key in ("atlas_path", "source", "source_label"):
            value = data.get(key)
            if not isinstance(value, str) or not value:
                continue
            if key == "atlas_path":
                target = manifest.parent / value
            elif "/" in value or "\\" in value:
                target = ROOT / value
            else:
                continue
            relative = require_repository_file(
                target,
                f"Asset target '{key}' referenced by {manifest_relative}",
                required_paths,
                errors,
            )
            if relative is not None:
                references.add(relative)
                resolved_targets[key] = target

        hash_targets = {
            "atlas_png_sha256": resolved_targets.get("atlas_path")
            or (manifest.parent / "atlas.png"),
            "manifest_sha256": manifest.parent / "skin.json",
            "original_file_sha256": resolved_targets.get("source")
            or resolved_targets.get("source_label"),
            "provenance_sha256": manifest.parent / "provenance.json",
        }
        for hash_field, target in hash_targets.items():
            expected = data.get(hash_field)
            if not isinstance(expected, str) or target is None or not target.is_file():
                continue
            actual = file_sha256(target)
            if actual.casefold() != expected.casefold():
                errors.append(
                    f"Asset hash mismatch for {manifest_relative} field {hash_field}: "
                    f"expected {expected}, got {actual}"
                )
    return references


def validate_source_tree() -> dict:
    required_paths: set[str] = set()
    errors: list[str] = []
    require_repository_file(
        ROOT / "_local/bundle.py",
        "Review bundle generator",
        required_paths,
        errors,
    )
    cmake_references = validate_cmake_references(required_paths, errors)
    member_roots, crate_roots = validate_cargo_workspace(required_paths, errors)
    rust_references = validate_rust_module_graph(
        required_paths, crate_roots, errors
    )
    asset_references = validate_asset_manifest_targets(required_paths, errors)
    if errors:
        details = "\n".join(f"- {error}" for error in errors)
        raise SourceTreeIntegrityError(
            f"Source tree integrity validation failed:\n{details}"
        )
    return {
        "required_paths": required_paths,
        "cmake_source_reference_count": len(cmake_references["source"]),
        "cmake_test_reference_count": len(cmake_references["test"]),
        "cargo_workspace_member_count": len(member_roots),
        "rust_module_reference_count": len(rust_references),
        "asset_manifest_target_count": len(asset_references),
    }


def walk_untracked_candidates() -> list[str]:
    files: list[str] = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        current = Path(dirpath)
        relative_dir = current.relative_to(ROOT)
        if should_skip_dir(relative_dir):
            dirnames[:] = []
            continue

        dirnames[:] = [
            dirname
            for dirname in dirnames
            if not should_skip_dir(relative_dir / dirname)
        ]
        files.extend(
            (relative_dir / filename).as_posix()
            for filename in filenames
            if (relative_dir / filename).as_posix()
        )
    return files


def run_git_ignored_filter(candidates: list[str]) -> set[str]:
    if not candidates:
        return set()
    payload = b"\0".join(path.encode("utf-8") for path in candidates) + b"\0"
    process = subprocess.run(
        [GIT_EXE, "check-ignore", "--stdin", "-z"],
        cwd=ROOT,
        input=payload,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if process.returncode not in {0, 1}:
        raise subprocess.CalledProcessError(
            process.returncode,
            process.args,
            output=process.stderr,
        )
    return {
        part.decode("utf-8", errors="replace")
        for part in process.stdout.split(b"\0")
        if part
    }


def collect_bundle_files(required_paths: set[str]) -> list[str]:
    tracked = [
        file_path
        for file_path in run_git_tracked_files()
        if not should_skip_candidate_path(file_path)
    ]
    tracked_set = set(tracked)
    candidates = [
        file_path
        for file_path in walk_untracked_candidates()
        if not should_skip_candidate_path(file_path)
    ]
    ignored = run_git_ignored_filter(candidates)
    untracked = [
        file_path
        for file_path in candidates
        if file_path not in tracked_set and file_path not in ignored
    ]
    return unique_paths([*tracked, *untracked, *sorted(required_paths)])


def matches_any(value: str, patterns: set[str]) -> bool:
    return any(fnmatch.fnmatchcase(value, pattern) for pattern in patterns)


def is_secret_path(path: Path) -> bool:
    name = path.name.casefold()
    if name == ".env" or name.startswith(".env."):
        return True
    if name in SECRET_FILE_NAMES:
        return True
    return matches_any(name, SECRET_FILE_PATTERNS)


def is_binary_by_extension(path: Path) -> bool:
    if path.suffix.casefold() in BINARY_EXTENSIONS:
        return True
    lower_name = path.name.casefold()
    return lower_name.endswith((".tar.bz2", ".tar.gz", ".tar.xz"))


def contains_binary_marker(path: Path) -> bool:
    try:
        with path.open("rb") as stream:
            return b"\0" in stream.read(8192)
    except OSError:
        return True


def output_relative_path(output_file: Path) -> str | None:
    try:
        return output_file.resolve().relative_to(ROOT).as_posix()
    except ValueError:
        return None


def should_skip(
    file_path: str, output_file: Path, required_paths: set[str]
) -> bool:
    path = ROOT / file_path
    path_name = PurePosixPath(file_path.replace("\\", "/")).as_posix()

    if is_secret_path(path):
        return True
    if path_name in required_paths:
        if path_name == output_relative_path(output_file):
            raise SourceTreeIntegrityError(
                f"Bundle output would overwrite required source: {path_name}"
            )
        if not path.exists() or not path.is_file() or path.is_symlink():
            raise SourceTreeIntegrityError(
                f"Required source cannot be bundled: {path_name}"
            )
        return False
    if path_name in ALWAYS_INCLUDE:
        return False
    if path_name in BUNDLE_EXCLUDE_ONLY:
        return True
    if matches_any(path_name, BUNDLE_EXCLUDE_PATTERNS):
        return True
    if path_name == output_relative_path(output_file):
        return True
    if should_skip_candidate_path(path_name):
        return True
    if not path.exists() or not path.is_file() or path.is_symlink():
        return True
    if is_binary_by_extension(path):
        return True
    try:
        if path.stat().st_size > MAX_FILE_SIZE:
            return True
    except OSError:
        return True
    return contains_binary_marker(path)


def resolve_output_file(argument: Path | None) -> Path:
    if argument is None:
        return default_output_file()
    expanded = argument.expanduser()
    if not expanded.is_absolute():
        expanded = ROOT / expanded
    return expanded.resolve()


def read_bundle_payloads(included: list[str]) -> dict[str, bytes]:
    payloads: dict[str, bytes] = {}
    for file_path in included:
        try:
            payloads[file_path] = (ROOT / file_path).read_bytes()
        except OSError as error:
            raise SourceTreeIntegrityError(
                f"Bundle source could not be read: {file_path}: {error}"
            ) from error
    return payloads


def build_bundle_manifest(
    payloads: dict[str, bytes],
    integrity: dict,
    status_short: str,
    git_head: str,
) -> dict:
    source_files = []
    source_tree_digest = hashlib.sha256()
    for file_path in sorted(payloads):
        payload = payloads[file_path]
        record = {
            "path": file_path,
            "sha256": hashlib.sha256(payload).hexdigest(),
            "size": len(payload),
        }
        source_files.append(record)
        source_tree_digest.update(
            json.dumps(
                record,
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
            ).encode("utf-8")
        )
        source_tree_digest.update(b"\n")

    return {
        "bundle_schema_version": BUNDLE_SCHEMA_VERSION,
        "source_files": source_files,
        "source_tree_sha256": source_tree_digest.hexdigest(),
        "cmake_references_complete": True,
        "rust_module_references_complete": True,
        "cargo_workspace_members_complete": True,
        "assets_manifest_targets_complete": True,
        "cmake_source_reference_count": integrity[
            "cmake_source_reference_count"
        ],
        "cmake_test_reference_count": integrity["cmake_test_reference_count"],
        "cargo_workspace_member_count": integrity[
            "cargo_workspace_member_count"
        ],
        "rust_module_reference_count": integrity[
            "rust_module_reference_count"
        ],
        "asset_manifest_target_count": integrity[
            "asset_manifest_target_count"
        ],
        "generated_at": datetime.now().astimezone().isoformat(timespec="seconds"),
        "git_head": git_head,
        "git_status_summary": git_status_summary(status_short),
        "project": PROJECT_NAME,
    }


def write_bundle_payload(outfile, file_path: str, payload: bytes) -> None:
    outfile.write(f"\n\n--- FILE: {file_path} ---\n")
    if b"\0" not in payload:
        try:
            text_payload = payload.decode("utf-8")
        except UnicodeDecodeError:
            text_payload = None
        if text_payload is not None:
            outfile.write("[BUNDLE_ENCODING: utf-8]\n")
            outfile.write(text_payload)
            if not text_payload.endswith("\n"):
                outfile.write("\n")
            return

    encoded = base64.b64encode(payload).decode("ascii")
    outfile.write("[BUNDLE_ENCODING: base64]\n")
    for offset in range(0, len(encoded), 76):
        outfile.write(encoded[offset : offset + 76])
        outfile.write("\n")


def create_bundle(output_file: Path, dry_run: bool) -> None:
    try:
        integrity = validate_source_tree()
        required_paths = integrity["required_paths"]
        files = collect_bundle_files(required_paths)
        status_short = run_git_status_short()
        git_head = run_git_head()
    except SourceTreeIntegrityError as error:
        raise SystemExit(str(error)) from error
    except (FileNotFoundError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Git repository inspection failed: {error}") from error

    try:
        included = sorted(
            file_path
            for file_path in files
            if not should_skip(file_path, output_file, required_paths)
        )
    except SourceTreeIntegrityError as error:
        raise SystemExit(str(error)) from error
    missing_required = sorted(required_paths - set(included))
    if missing_required:
        raise SystemExit(
            "Review bundle omitted required build source:\n"
            + "\n".join(f"- {path}" for path in missing_required)
        )

    try:
        payloads = read_bundle_payloads(included)
    except SourceTreeIntegrityError as error:
        raise SystemExit(str(error)) from error
    manifest = build_bundle_manifest(payloads, integrity, status_short, git_head)
    skipped_count = len(files) - len(included)
    if dry_run:
        print(
            f"bundle_check=passed project={PROJECT_NAME} "
            f"included={len(included)} skipped={skipped_count} "
            f"source_tree_sha256={manifest['source_tree_sha256']} "
            "cmake_references_complete=true "
            "rust_module_references_complete=true "
            f"output={output_file}"
        )
        return

    output_file.parent.mkdir(parents=True, exist_ok=True)
    with output_file.open("w", encoding="utf-8", newline="\n") as outfile:
        outfile.write(
            json.dumps(manifest, ensure_ascii=False, separators=(",", ":"))
        )
        outfile.write("\n--- GIT STATUS --short ---\n")
        outfile.write(status_short if status_short else "(clean)\n")
        outfile.write("--- END GIT STATUS --short ---\n")

        for file_path, payload in payloads.items():
            write_bundle_payload(outfile, file_path, payload)

    print(
        f"bundle_created={output_file} included={len(included)} "
        f"skipped={skipped_count} "
        f"source_tree_sha256={manifest['source_tree_sha256']}"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create a review-oriented text bundle for the Clearra repository."
    )
    parser.add_argument(
        "--output",
        type=Path,
        help=(
            "Output path. Relative paths resolve from the repository root. "
            "The default is _local/project_bundle.txt."
        ),
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate collection and print counts without writing a bundle.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    create_bundle(resolve_output_file(args.output), args.dry_run)


if __name__ == "__main__":
    main()
