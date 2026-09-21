from __future__ import annotations

import importlib.util
import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest import mock


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
        (self.repo / "config").mkdir()
        shutil.copy2(
            ROOT / "config" / "clearra-management.v1.json",
            self.repo / "config" / "clearra-management.v1.json",
        )
        shutil.copy2(ROOT / "pnpm-lock.yaml", self.repo / "pnpm-lock.yaml")
        shutil.copy2(ROOT / "rust-toolchain.toml", self.repo / "rust-toolchain.toml")
        (self.repo / "base.txt").write_text("base\n", encoding="utf-8", newline="\n")
        self.git(
            "add",
            "base.txt",
            "config/clearra-management.v1.json",
            "pnpm-lock.yaml",
            "rust-toolchain.toml",
        )
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

    def decide(
        self,
        receipt: str,
        item: dict,
        decision: str,
        reason: str = "fixture records an explicit reviewed decision",
    ) -> None:
        MANAGE.record_review_decision(
            receipt,
            self.policy,
            decision=decision,
            reason=reason,
            ref=item.get("ref") if item.get("kind") == "ref" else None,
            worktree=(
                item.get("worktree") if item.get("kind") == "dirty-worktree" else None
            ),
        )

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
            self.decide(
                result["receipt"],
                item,
                "excluded",
                "fixture explicitly excludes this preserved change",
            )
        review = json.loads(review_path.read_text(encoding="utf-8"))
        accepted = MANAGE.validate_convergence_review(
            result["receipt"], self.git("rev-parse", "refs/remotes/origin/main"), self.policy
        )
        self.assertEqual(accepted["excluded"], len(review["items"]))

    def test_dirty_default_main_blocks_promotion_before_remote_mutation(self) -> None:
        with self.assertRaisesRegex(MANAGE.ManagementError, "default checkout main is dirty"):
            MANAGE.default_main_preflight(self.git("rev-parse", "refs/remotes/origin/main"))

    def test_candidate_check_is_single_lookup_and_exact_sha_closed(self) -> None:
        candidate = "codex/converge-check-fixture"
        self.git("branch", candidate, "main")
        self.git("push", "origin", candidate)
        sha = self.git("rev-parse", "main")
        original_run = MANAGE.run

        def exercise(checks):
            calls = []

            def fixture_run(command, **kwargs):
                if list(command[:2]) == ["gh", "api"]:
                    calls.append(list(command))
                    return subprocess.CompletedProcess(
                        command, 0, json.dumps({"check_runs": checks}), ""
                    )
                return original_run(command, **kwargs)

            with mock.patch.object(MANAGE, "repository_name", return_value="owner/repo"), mock.patch.object(
                MANAGE, "run", side_effect=fixture_run
            ):
                result = MANAGE.check_candidate_once(candidate, self.policy)
            self.assertEqual(len(calls), 1)
            return result

        success = {
            "id": 2,
            "name": "Clearra management policy",
            "head_sha": sha,
            "status": "completed",
            "conclusion": "success",
            "html_url": "https://example.invalid/run/2",
        }
        accepted_sha, selected = exercise(
            [
                {**success, "id": 1, "head_sha": "0" * 40},
                success,
            ]
        )
        self.assertEqual(accepted_sha, sha)
        self.assertEqual(selected[0]["id"], 2)

        with self.assertRaisesRegex(MANAGE.ManagementError, "missing"):
            exercise([{**success, "head_sha": "0" * 40}])
        with self.assertRaisesRegex(MANAGE.ManagementError, "not successful"):
            exercise([{**success, "status": "in_progress", "conclusion": None}])
        with self.assertRaisesRegex(MANAGE.ManagementError, "not successful"):
            exercise([{**success, "conclusion": "failure"}])

    def test_remote_main_race_blocks_candidate_push_without_data_loss(self) -> None:
        self.git("switch", "-c", "codex/converge-race")
        (self.repo / "candidate.txt").write_text(
            "candidate\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "candidate.txt")
        self.git("commit", "-m", "candidate")
        candidate_sha = self.git("rev-parse", "HEAD")
        expected_base = self.git("rev-parse", "refs/remotes/origin/main")

        racer = self.base / "racer"
        self.git("clone", "--branch", "main", str(self.remote), str(racer), cwd=self.base)
        self.git("config", "user.name", "Clearra Race Fixture", cwd=racer)
        self.git("config", "user.email", "race@example.invalid", cwd=racer)
        (racer / "remote.txt").write_text("remote\n", encoding="utf-8", newline="\n")
        self.git("add", "remote.txt", cwd=racer)
        self.git("commit", "-m", "advance remote main", cwd=racer)
        self.git("push", "origin", "main", cwd=racer)
        raced_sha = self.git("rev-parse", "HEAD", cwd=racer)

        with self.assertRaisesRegex(MANAGE.ManagementError, "changed after"):
            MANAGE.push_main_fast_forward(candidate_sha, expected_base)
        self.assertEqual(
            self.git("ls-remote", "origin", "refs/heads/main").split()[0], raced_sha
        )

    def test_candidate_upload_is_review_closed_and_never_force_pushes(self) -> None:
        self.git("restore", "base.txt")
        self.git("restore", "--staged", "staged.bin")
        (self.repo / "staged.bin").unlink()
        (self.repo / "notes.txt").unlink()
        shutil.rmtree(self.repo / "build")
        candidate = "codex/converge-upload-fixture"
        self.git("switch", "-c", candidate, "main")
        (self.repo / "candidate.txt").write_text(
            "candidate\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "candidate.txt")
        self.git("commit", "-m", "candidate")
        first_sha = self.git("rev-parse", "HEAD")

        with mock.patch.object(
            MANAGE,
            "validate_convergence_review",
            return_value={"selected": 1, "excluded": 0},
        ), mock.patch.object(
            MANAGE,
            "authorized_github_maintainers",
            return_value=[{"login": "fixture", "github_user_id": 1}],
        ):
            uploaded = MANAGE.upload_candidate(candidate, "fixture-receipt", self.policy)
        self.assertTrue(uploaded["uploaded"])
        self.assertEqual(uploaded["remote_candidate_after"], first_sha)
        self.assertTrue(pathlib.Path(uploaded["receipt"]).is_file())

        racer = self.base / "candidate-racer"
        self.git("clone", "--branch", candidate, str(self.remote), str(racer), cwd=self.base)
        self.git("config", "user.name", "Clearra Candidate Race Fixture", cwd=racer)
        self.git("config", "user.email", "race@example.invalid", cwd=racer)
        (racer / "remote-candidate.txt").write_text(
            "remote\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "remote-candidate.txt", cwd=racer)
        self.git("commit", "-m", "advance remote candidate", cwd=racer)
        self.git("push", "origin", candidate, cwd=racer)
        raced_sha = self.git("rev-parse", "HEAD", cwd=racer)

        (self.repo / "local-candidate.txt").write_text(
            "local\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "local-candidate.txt")
        self.git("commit", "-m", "diverge local candidate")
        with mock.patch.object(
            MANAGE,
            "validate_convergence_review",
            return_value={"selected": 1, "excluded": 0},
        ), mock.patch.object(
            MANAGE,
            "authorized_github_maintainers",
            return_value=[{"login": "fixture", "github_user_id": 1}],
        ), self.assertRaisesRegex(MANAGE.ManagementError, "not a fast-forward"):
            MANAGE.upload_candidate(candidate, "fixture-receipt", self.policy)
        self.assertEqual(
            self.git("ls-remote", "--heads", "origin", f"refs/heads/{candidate}").split()[0],
            raced_sha,
        )

    def test_detached_exact_sha_upload_does_not_move_protected_local_candidate(self) -> None:
        self.git("restore", "base.txt")
        self.git("restore", "--staged", "staged.bin")
        (self.repo / "staged.bin").unlink()
        (self.repo / "notes.txt").unlink()
        shutil.rmtree(self.repo / "build")
        candidate = "codex/converge-detached-upload-fixture"
        protected = self.base / "protected-candidate"
        self.git("branch", candidate, "main")
        protected_sha = self.git("rev-parse", candidate)
        self.git("push", "origin", candidate)
        self.git("worktree", "add", str(protected), candidate)
        (protected / "protected-uncommitted.txt").write_text(
            "must remain untouched\n", encoding="utf-8", newline="\n"
        )

        self.git("switch", "--detach", "main")
        (self.repo / "detached-candidate.txt").write_text(
            "reviewed detached candidate\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "detached-candidate.txt")
        self.git("commit", "-m", "detached exact candidate")
        detached_sha = self.git("rev-parse", "HEAD")

        with mock.patch.object(
            MANAGE,
            "validate_convergence_review",
            return_value={"selected": 1, "excluded": 0},
        ), mock.patch.object(
            MANAGE,
            "authorized_github_maintainers",
            return_value=[{"login": "fixture", "github_user_id": 1}],
        ):
            uploaded = MANAGE.upload_candidate(
                candidate,
                "fixture-receipt",
                self.policy,
                exact_commit=detached_sha,
            )

        self.assertEqual(uploaded["source_mode"], "detached-exact-sha")
        self.assertEqual(uploaded["remote_candidate_after"], detached_sha)
        self.assertEqual(uploaded["local_candidate_ref_sha"], protected_sha)
        self.assertEqual(uploaded["local_candidate_worktrees"][0]["head"], protected_sha)
        self.assertEqual(uploaded["local_candidate_worktrees"][0]["dirty_entries"], 1)
        self.assertTrue(uploaded["local_candidate_worktrees"][0]["preserved"])
        self.assertEqual(uploaded["parents"], [protected_sha])
        self.assertRegex(uploaded["diff_sha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(self.git("rev-parse", candidate), protected_sha)
        self.assertEqual(
            self.git("status", "--porcelain=v1", cwd=protected),
            "?? protected-uncommitted.txt",
        )

        self.git("switch", "main")
        with mock.patch.object(
            MANAGE,
            "validate_convergence_review",
            return_value={"selected": 1, "excluded": 0},
        ), mock.patch.object(
            MANAGE,
            "authorized_github_maintainers",
            return_value=[{"login": "fixture", "github_user_id": 1}],
        ), self.assertRaisesRegex(MANAGE.ManagementError, "detached clean worktree"):
            MANAGE.upload_candidate(
                candidate,
                "fixture-receipt",
                self.policy,
                exact_commit=detached_sha,
            )

    def test_repository_lock_recovers_dead_owner_and_blocks_live_owner(self) -> None:
        identity = MANAGE.hashlib.sha256(MANAGE.normalized(self.repo).encode()).hexdigest()[:24]
        lock = MANAGE.state_root() / "git-locks" / f"{identity}.lock"
        lock.parent.mkdir(parents=True, exist_ok=True)
        lock.write_text(
            json.dumps({"pid": 2_147_483_647, "created_utc": "2026-01-01T00:00:00+00:00"}),
            encoding="utf-8",
        )
        with MANAGE.repository_lock() as acquired:
            owner = json.loads(acquired.read_text(encoding="utf-8"))
            self.assertEqual(owner["pid"], MANAGE.os.getpid())
            self.assertEqual(owner["recovered_stale_owner"]["pid"], 2_147_483_647)
            with self.assertRaisesRegex(MANAGE.ManagementError, "another Clearra"):
                with MANAGE.repository_lock():
                    self.fail("a live repository lock must not be replaced")
        self.assertFalse(lock.exists())
        recovery_receipts = list(
            MANAGE.state_root().rglob("*-git-stale-lock-recovered-*.json")
        )
        self.assertEqual(len(recovery_receipts), 1)

    def test_reviewed_ref_tip_covers_its_unique_ancestor_for_finalization(self) -> None:
        self.git("switch", "unique")
        ancestor = self.git("rev-parse", "HEAD")
        self.git("switch", "-c", "reviewed-descendant")
        (self.repo / "reviewed-descendant.txt").write_text(
            "reviewed descendant\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "reviewed-descendant.txt")
        self.git("commit", "-m", "reviewed descendant")
        reviewed_tip = self.git("rev-parse", "HEAD")
        self.git("switch", "main")

        accepted, reason = MANAGE.ref_is_reviewed_or_equivalent(
            "", ancestor, self.git("rev-parse", "main"), {}, {reviewed_tip}
        )
        self.assertTrue(accepted)
        self.assertEqual(reason, "ancestor-of-reviewed-sha")

    def test_moved_ref_does_not_inherit_an_older_review_decision(self) -> None:
        reviewed_sha = self.git("rev-parse", "main")
        moved_sha = self.git("rev-parse", "unique")
        accepted, reason = MANAGE.ref_is_reviewed_or_equivalent(
            "refs/heads/moved-after-review",
            moved_sha,
            reviewed_sha,
            {
                "refs/heads/moved-after-review": {
                    "sha": reviewed_sha,
                    "decision": "excluded",
                }
            },
            {reviewed_sha},
        )
        self.assertFalse(accepted)
        self.assertEqual(reason, "unique")

    def test_dirty_worktree_evidence_reconstructs_the_exact_candidate_tree(self) -> None:
        result = MANAGE.prepare_git_safety(self.policy)
        self.git("add", "base.txt", "staged.bin", "notes.txt")
        self.git("commit", "-m", "preserve reviewed dirty worktree")
        candidate = self.git("rev-parse", "HEAD")
        evidence = MANAGE.record_dirty_candidate_evidence(
            result["receipt"], str(self.repo), candidate, self.policy
        )
        self.assertEqual(evidence["candidate_sha"], candidate)

        receipt = json.loads(pathlib.Path(result["receipt"]).read_text(encoding="utf-8"))
        review_path = pathlib.Path(receipt["review_decisions"])
        review = json.loads(review_path.read_text(encoding="utf-8"))
        dirty = [item for item in review["items"] if item["kind"] == "dirty-worktree"]
        self.assertEqual(len(dirty), 1)
        self.assertEqual(dirty[0]["decision"], "selected")
        self.assertIsInstance(dirty[0].get("candidate_evidence"), dict)
        for item in review["items"]:
            if item["decision"] == "pending":
                self.decide(
                    result["receipt"],
                    item,
                    "excluded",
                    "fixture explicitly excludes this preserved change",
                )

        accepted = MANAGE.validate_convergence_review(
            result["receipt"], candidate, self.policy
        )
        self.assertEqual(accepted["selected"], 1)
        self.assertEqual(len(accepted["dirty_evidence"]), 1)

        evidence_path = pathlib.Path(evidence["evidence"])
        material = json.loads(evidence_path.read_text(encoding="utf-8"))
        material["verified_exact_tree"] = False
        MANAGE.write_json_atomic(evidence_path, material)
        with self.assertRaisesRegex(MANAGE.ManagementError, "digest changed"):
            MANAGE.validate_convergence_review(result["receipt"], candidate, self.policy)

    def test_selected_unique_ref_is_replayed_with_verifiable_provenance(self) -> None:
        result = MANAGE.prepare_git_safety(self.policy)
        self.git("restore", "base.txt")
        self.git("restore", "--staged", "staged.bin")
        (self.repo / "staged.bin").unlink()
        (self.repo / "notes.txt").unlink()
        shutil.rmtree(self.repo / "build")
        self.git("switch", "-c", "codex/converge-fixture")

        receipt = json.loads(pathlib.Path(result["receipt"]).read_text(encoding="utf-8"))
        review_path = pathlib.Path(receipt["review_decisions"])
        review = json.loads(review_path.read_text(encoding="utf-8"))
        unique_sha = self.git("rev-parse", "refs/heads/unique")
        for item in review["items"]:
            if item.get("kind") == "ref" and item.get("sha") == unique_sha:
                self.decide(
                    result["receipt"],
                    item,
                    "selected",
                    "fixture selects this unique source history",
                )
            else:
                self.decide(
                    result["receipt"],
                    item,
                    "excluded",
                    "fixture explicitly excludes this preserved change",
                )

        replay = MANAGE.apply_convergence_review(
            result["receipt"], "codex/converge-fixture", self.policy
        )
        self.assertNotEqual(replay["initial_sha"], replay["final_sha"])
        self.assertEqual((self.repo / "unique.txt").read_text(encoding="utf-8"), "unique\n")
        self.assertEqual(len(replay["convergence"]["ref_evidence"]), 1)
        self.assertNotEqual(replay["applied"][0]["source"], replay["applied"][0]["result"])
        accepted = MANAGE.validate_convergence_review(
            result["receipt"], replay["final_sha"], self.policy
        )
        self.assertEqual(accepted["selected"], 1)

    def test_conflicting_selected_ref_is_receipted_and_candidate_is_restored(self) -> None:
        self.git("restore", "base.txt")
        self.git("restore", "--staged", "staged.bin")
        (self.repo / "staged.bin").unlink()
        (self.repo / "notes.txt").unlink()
        shutil.rmtree(self.repo / "build")

        self.git("switch", "-c", "conflicting-source", self.base_sha)
        (self.repo / "base.txt").write_text(
            "selected history\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "base.txt")
        self.git("commit", "-m", "selected conflicting change")
        conflicting_sha = self.git("rev-parse", "HEAD")

        self.git("switch", "-c", "codex/converge-conflict", "main")
        (self.repo / "base.txt").write_text(
            "candidate history\n", encoding="utf-8", newline="\n"
        )
        self.git("add", "base.txt")
        self.git("commit", "-m", "candidate conflicting change")
        initial_sha = self.git("rev-parse", "HEAD")
        result = MANAGE.prepare_git_safety(self.policy)

        receipt = json.loads(pathlib.Path(result["receipt"]).read_text(encoding="utf-8"))
        review_path = pathlib.Path(receipt["review_decisions"])
        review = json.loads(review_path.read_text(encoding="utf-8"))
        for item in review["items"]:
            if item.get("kind") == "ref" and item.get("sha") == conflicting_sha:
                self.decide(
                    result["receipt"],
                    item,
                    "selected",
                    "fixture selects a deliberately conflicting commit",
                )
            else:
                self.decide(
                    result["receipt"],
                    item,
                    "excluded",
                    "fixture explicitly excludes this preserved change",
                )

        with self.assertRaisesRegex(MANAGE.ManagementError, "rolled back"):
            MANAGE.apply_convergence_review(
                result["receipt"], "codex/converge-conflict", self.policy
            )
        self.assertEqual(self.git("rev-parse", "HEAD"), initial_sha)
        self.assertEqual(self.git("status", "--porcelain=v1"), "")
        conflict_receipts = list(
            pathlib.Path(result["receipt"]).parent.glob("replay-conflict-*.json")
        )
        self.assertEqual(len(conflict_receipts), 1)
        conflict = json.loads(conflict_receipts[0].read_text(encoding="utf-8"))
        self.assertEqual(conflict["restored_head"], initial_sha)
        self.assertEqual(conflict["source_commit"], conflicting_sha)
        self.assertEqual(conflict["conflict"]["count"], 1)
        self.assertTrue(conflict["rollback_ref"].startswith("refs/clearra-safety/"))
        self.assertEqual(self.git("rev-parse", conflict["rollback_ref"]), initial_sha)

    def test_finalization_removes_only_fully_reviewed_and_verified_git_state(self) -> None:
        result = MANAGE.prepare_git_safety(self.policy)
        self.git("restore", "base.txt")
        self.git("restore", "--staged", "staged.bin")
        (self.repo / "staged.bin").unlink()
        (self.repo / "notes.txt").unlink()
        shutil.rmtree(self.repo / "build")

        receipt = json.loads(pathlib.Path(result["receipt"]).read_text(encoding="utf-8"))
        review_path = pathlib.Path(receipt["review_decisions"])
        review = json.loads(review_path.read_text(encoding="utf-8"))
        for item in review["items"]:
            self.decide(
                result["receipt"],
                item,
                "excluded",
                "fixture explicitly excludes this safely bundled state",
            )

        candidate = "codex/converge-fixture"
        candidate_path = self.base / "candidate"
        self.git("branch", candidate, "main")
        self.git("push", "origin", candidate)
        self.git("worktree", "add", str(candidate_path), candidate)
        sha = self.git("rev-parse", "main")
        tree = self.git("rev-parse", "main^{tree}")

        independent = self.base / "state" / "git-verification" / "fixture"
        self.git("clone", "--no-checkout", str(self.remote), str(independent), cwd=self.base)
        self.git("checkout", "--detach", sha, cwd=independent)
        promotion_path = MANAGE.write_receipt(
            "git-promotion",
            {
                "candidate": candidate,
                "sha": sha,
                "tree": tree,
                "ruleset": {"ruleset": {"id": 42}},
                "independent_checkout": {"path": str(independent), "sha": sha, "tree": tree},
            },
        )
        ruleset = {
            "id": 42,
            "name": "Clearra main fast-forward gate",
            "target": "branch",
            "enforcement": "active",
            "bypass_actors": [],
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
                            {"context": "Clearra management policy"}
                        ],
                    },
                },
            ],
        }
        with mock.patch.object(MANAGE, "read_ruleset", return_value=ruleset):
            blocker = self.detached / "late-unreviewed.txt"
            blocker.write_text("late dirty state\n", encoding="utf-8", newline="\n")
            with self.assertRaisesRegex(MANAGE.ManagementError, "receipt=") as blocked:
                MANAGE.finalize_candidate(
                    candidate,
                    result["receipt"],
                    str(promotion_path),
                    self.policy,
                    apply=False,
                )
            blocker_receipt = pathlib.Path(str(blocked.exception).split("receipt=", 1)[1])
            blocked_material = json.loads(blocker_receipt.read_text(encoding="utf-8"))
            self.assertEqual(
                blocked_material["blocked_worktrees"][0]["path"], str(self.detached)
            )
            blocker.unlink()
            plan = MANAGE.finalize_candidate(
                candidate,
                result["receipt"],
                str(promotion_path),
                self.policy,
                apply=False,
            )
            self.assertGreaterEqual(len(plan["worktrees"]), 2)
            final = MANAGE.finalize_candidate(
                candidate,
                result["receipt"],
                str(promotion_path),
                self.policy,
                apply=True,
            )
        self.assertTrue(final["independent_checkout_removed"])
        self.assertTrue(final["safety_transaction_removed"])
        self.assertEqual(self.git("branch", "--format=%(refname)"), "refs/heads/main")
        self.assertEqual(
            self.git("ls-remote", "--heads", "origin").split()[1], "refs/heads/main"
        )


if __name__ == "__main__":
    unittest.main()
