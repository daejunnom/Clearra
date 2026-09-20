from __future__ import annotations

import importlib.util
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
import zipfile


ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "clearra_manage_git_fixture", ROOT / "_local" / "clearra_manage.py"
)
assert SPEC and SPEC.loader
MANAGE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MANAGE
SPEC.loader.exec_module(MANAGE)


class GitManagementFixtureTests(unittest.TestCase):
    def git(self, *arguments: str, cwd: pathlib.Path | None = None) -> str:
        result = subprocess.run(
            ["git", *arguments],
            cwd=cwd or self.repo,
            check=True,
            text=True,
            encoding="utf-8",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        return result.stdout.strip()

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.base = pathlib.Path(self.temporary.name)
        self.repo = self.base / "repository"
        self.remote = self.base / "remote.git"
        self.detached = self.base / "detached"
        self.git("init", "--bare", str(self.remote), cwd=self.base)
        self.git("init", "-b", "main", str(self.repo), cwd=self.base)
        self.git("config", "user.name", "Clearra Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "core.autocrlf", "false")
        (self.repo / "base.txt").write_text("base\n", encoding="utf-8", newline="\n")
        self.git("add", "base.txt")
        self.git("commit", "-m", "base")
        self.base_sha = self.git("rev-parse", "HEAD")
        self.git("remote", "add", "origin", str(self.remote))
        self.git("push", "-u", "origin", "main")

        self.git("branch", "ancestor", self.base_sha)
        self.git("switch", "-c", "equivalent", self.base_sha)
        (self.repo / "equivalent.txt").write_text("same patch\n", encoding="utf-8", newline="\n")
        self.git("add", "equivalent.txt")
        self.git("commit", "-m", "equivalent source")

        self.git("switch", "main")
        (self.repo / "equivalent.txt").write_text("same patch\n", encoding="utf-8", newline="\n")
        self.git("add", "equivalent.txt")
        self.git("commit", "-m", "equivalent canonical")
        (self.repo / "canonical.txt").write_text("canonical\n", encoding="utf-8", newline="\n")
        self.git("add", "canonical.txt")
        self.git("commit", "-m", "canonical follow-up")
        self.git("push", "origin", "main")

        self.git("switch", "-c", "tree-identical")
        self.git("commit", "--allow-empty", "-m", "identical tree")
        self.git("switch", "-c", "unique", self.base_sha)
        (self.repo / "unique.txt").write_text("unique\n", encoding="utf-8", newline="\n")
        self.git("add", "unique.txt")
        self.git("commit", "-m", "unique")
        self.git("switch", "main")
        self.git("worktree", "add", "--detach", str(self.detached), self.base_sha)

        (self.repo / "base.txt").write_text("base changed\n", encoding="utf-8", newline="\n")
        (self.repo / "staged.bin").write_bytes(bytes(range(32)))
        self.git("add", "staged.bin")
        (self.repo / "notes.txt").write_text("untracked source\n", encoding="utf-8", newline="\n")
        (self.repo / "build" / "fixture").mkdir(parents=True)
        (self.repo / "build" / "fixture" / "generated.bin").write_bytes(b"generated")

        self.original_root = MANAGE.ROOT
        self.original_state_root = MANAGE.state_root
        MANAGE.ROOT = self.repo
        MANAGE.state_root = lambda: self.base / "state"
        self.policy = MANAGE.load_policy(ROOT / "config" / "clearra-management.v1.json")

    def tearDown(self) -> None:
        MANAGE.ROOT = self.original_root
        MANAGE.state_root = self.original_state_root
        self.temporary.cleanup()

    def test_inventory_classifies_refs_and_preserves_dirty_detached_worktree(self) -> None:
        inventory = MANAGE.git_inventory(self.policy, fetch=False)
        classifications = {item["ref"]: item["classification"] for item in inventory["refs"]}
        self.assertEqual(classifications["refs/heads/ancestor"], "ancestor")
        self.assertEqual(classifications["refs/heads/equivalent"], "patch-equivalent")
        self.assertEqual(classifications["refs/heads/tree-identical"], "tree-identical")
        self.assertEqual(classifications["refs/heads/unique"], "unique")
        statuses = {pathlib.Path(item["path"]): item for item in inventory["worktrees"]}
        self.assertGreaterEqual(statuses[self.repo]["dirty_entries"], 3)
        self.assertTrue(statuses[self.detached]["detached"])
        self.assertFalse(inventory["shallow"])

    def test_safety_bundle_archives_source_and_skips_reproducible_build_output(self) -> None:
        result = MANAGE.prepare_git_safety(self.policy)
        receipt = json.loads(pathlib.Path(result["receipt"]).read_text(encoding="utf-8"))
        self.assertTrue(receipt["ready_for_review"])
        bundle = pathlib.Path(receipt["bundle"])
        self.assertTrue(bundle.is_file())
        self.git("bundle", "verify", str(bundle))
        archive = bundle.parent / "worktree-uncommitted.zip"
        with zipfile.ZipFile(archive) as material:
            names = material.namelist()
        self.assertTrue(any(name.endswith("untracked/notes.txt") for name in names))
        self.assertFalse(any(name.endswith("generated.bin") for name in names))
        manifest = json.loads(
            (bundle.parent / "untracked-manifest.json").read_text(encoding="utf-8")
        )
        generated = [item for item in manifest if item.get("path") == "build/fixture/generated.bin"]
        self.assertEqual(generated[0]["classification"], "reproducible-generated")
        with self.assertRaisesRegex(MANAGE.ManagementError, "pending"):
            MANAGE.validate_convergence_review(
                result["receipt"], self.git("rev-parse", "refs/remotes/origin/main"), self.policy
            )
        review_path = pathlib.Path(receipt["review_decisions"])
        review = json.loads(review_path.read_text(encoding="utf-8"))
        for item in review["items"]:
            item["decision"] = "excluded"
            item["reason"] = "fixture explicitly excludes this preserved change"
        review_path.write_text(
            json.dumps(review, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        accepted = MANAGE.validate_convergence_review(
            result["receipt"], self.git("rev-parse", "refs/remotes/origin/main"), self.policy
        )
        self.assertEqual(accepted["excluded"], len(review["items"]))

    def test_dirty_default_main_blocks_promotion_before_remote_mutation(self) -> None:
        with self.assertRaisesRegex(MANAGE.ManagementError, "default checkout main is dirty"):
            MANAGE.default_main_preflight(self.git("rev-parse", "refs/remotes/origin/main"))


if __name__ == "__main__":
    unittest.main()
