"""Disposable Git probes for the audit script's input and publication boundaries."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SCRIPT = Path(__file__).with_name("reconcile.py")
SPEC = importlib.util.spec_from_file_location("reconcile", SCRIPT)
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class ReconciliationBoundaries(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="peritus-impact-probe-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.git("init", "-q")
        (self.root / ".gitattributes").write_text("*.txt export-subst\n")
        (self.root / "source.txt").write_text("$Format:%H$\n")
        self.base = self.commit()
        (self.root / "second.txt").write_text("second source\n")
        self.candidate = self.commit()

    def git(self, *args, data=None):
        return subprocess.run(
            ["git", *args], cwd=self.root, input=data, check=True, capture_output=True
        ).stdout

    def commit(self):
        self.git("add", ".gitattributes", "source.txt")
        if (self.root / "second.txt").exists():
            self.git("add", "second.txt")
        self.git("-c", "user.name=Audit fixture", "-c", "user.email=fixture@example.invalid",
                 "-c", "commit.gpgsign=false", "commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD").decode().strip()

    def invoke(self, base, output):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--base", base, "--candidate", self.candidate,
             "--xtask", sys.executable, "--output", str(output)],
            cwd=self.root, capture_output=True,
        )

    def test_materialization_preserves_raw_bytes_despite_export_substitution(self):
        destination = self.root / "materialized"
        destination.mkdir()
        hashes = AUDIT.materialize(self.root, self.candidate, destination)
        self.assertEqual((destination / "source.txt").read_bytes(), b"$Format:%H$\n")
        self.assertEqual(hashes["source.txt"], AUDIT.digest(b"$Format:%H$\n"))

    def test_full_annotated_tag_identity_is_rejected_as_a_commit(self):
        self.git("-c", "user.name=Audit fixture", "-c", "user.email=fixture@example.invalid",
                 "-c", "tag.gpgsign=false", "tag", "-a", "fixture", self.base, "-m", "fixture")
        tag = self.git("rev-parse", "fixture").decode().strip()
        result = self.invoke(tag, self.root / "output")
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"must name a commit directly", result.stderr)

    def test_existing_output_and_symlink_are_never_overwritten(self):
        target = self.root / "existing"
        target.mkdir()
        sentinel = target / "reconciliation.json"
        sentinel.write_bytes(b"preserve me")
        alias = self.root / "alias"
        alias.symlink_to(target, target_is_directory=True)
        for output in (target, alias):
            result = self.invoke(self.base, output)
            self.assertEqual(result.returncode, 2)
            self.assertIn(b"output must be a fresh path", result.stderr)
            self.assertEqual(sentinel.read_bytes(), b"preserve me")

    def test_failed_inventory_publishes_no_result_directory(self):
        output = self.root / "output"
        # Python is deliberately not an xtask exporter; inventory execution must fail.
        result = self.invoke(self.base, output)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(output.exists())

    def test_symlink_and_excessive_tree_entries_fail_before_materialization(self):
        blob = self.git("hash-object", "-w", "--stdin", data=b"bytes").decode().strip()
        for rows in (
            f"120000 blob {blob}\tlink\n".encode(),
            b"".join(f"100644 blob {blob}\tp{index:05d}\n".encode() for index in range(30001)),
        ):
            tree = self.git("mktree", data=rows).decode().strip()
            destination = self.root / "refused"
            with self.assertRaises(ValueError):
                AUDIT.materialize(self.root, tree, destination)
            self.assertFalse(destination.exists())


class CandidateActorEnrollment(unittest.TestCase):
    BASE = (
        ("ACTOR-0001", "Historical Owner", "owner", 11, "/root/base_owner"),
        ("ACTOR-0002", "Historical Reviewer", "reviewer", 12, "/root/base_review"),
    )
    FRESH = (
        ("ACTOR-0003", "Candidate Owner", "owner", 22, "/root/candidate_owner"),
        ("ACTOR-0004", "Independent Reviewer", "reviewer", 23, "/root/candidate_review"),
    )

    def documents(self, specs, provenance_entries=None, pointer_sha256=None):
        if provenance_entries is None:
            provenance_entries = [self.provenance(spec) for spec in specs]
        provenance = {
            "schema": "peritus.verification.actor-provenance",
            "schema_version": 1,
            "baseline": "A1",
            "entries": provenance_entries,
        }
        provenance_bytes = (json.dumps(provenance, indent=2) + "\n").encode()
        pointer = pointer_sha256 or AUDIT.digest(provenance_bytes)
        actor_lines = [
            'schema = "peritus.verification.actors"',
            "schema_version = 1",
            'baseline = "A1"',
        ]
        for actor_id, name, role, session, task in specs:
            principal = self.principal(session, task)
            actor_lines.extend([
                "",
                "[[entries]]",
                f'id = "{actor_id}"',
                'kind = "codex-subagent"',
                f'principal = "{principal}"',
                f'display_name = "{name}"',
                f'roles = ["{role}"]',
                (f'provenance = {{ record_path = "{AUDIT.PROVENANCE_PATH}", '
                 f'record_sha256 = "{pointer}" }}'),
            ])
        return ("\n".join(actor_lines) + "\n").encode(), provenance_bytes

    @staticmethod
    def principal(session, task):
        return f"{AUDIT.REPOSITORY}/session/{session}/task{task}"

    def provenance(self, spec):
        actor_id, _name, role, session, task = spec
        principal = self.principal(session, task)
        entry = {
            "actor_id": actor_id,
            "kind": "codex-subagent",
            "principal": principal,
            "repository": AUDIT.REPOSITORY,
            "issue": 75,
            "issue_created_at": "2026-09-12T12:34:56Z",
            "session": session,
            "task": task,
            "mode": "read-only-review" if role == "reviewer" else "implementation",
            "record_locators": [f"codex-collaboration:{principal}"],
        }
        if role == "reviewer":
            entry.update({"model": "gpt-5.6-sol", "reasoning_effort": "xhigh"})
        return entry

    def snapshots(self):
        base_actor, base_provenance = self.documents(self.BASE)
        candidate_actor, candidate_provenance = self.documents(self.BASE + self.FRESH)
        return (
            AUDIT.validate_actor_snapshot(base_actor, base_provenance, "protected"),
            AUDIT.validate_actor_snapshot(candidate_actor, candidate_provenance, "candidate"),
            base_actor,
            base_provenance,
            candidate_actor,
            candidate_provenance,
        )

    def test_valid_append_updates_only_historical_aggregate_hash_pointers(self):
        protected, candidate, base_actor, base_provenance, candidate_actor, candidate_provenance = (
            self.snapshots()
        )
        enrollment = AUDIT.actor_enrollment(protected, candidate)
        self.assertEqual(enrollment["status"], "pending-independent-review")
        self.assertEqual(enrollment["authority"], "audit-only")
        self.assertEqual(
            enrollment["authenticity_status"], "unverified-self-reported-provenance"
        )
        self.assertEqual(enrollment["appended_actor_ids"], ["ACTOR-0003", "ACTOR-0004"])
        self.assertEqual(enrollment["appended_provenance_ids"], ["ACTOR-0003", "ACTOR-0004"])
        self.assertEqual(enrollment["protected_actor_registry_sha256"], AUDIT.digest(base_actor))
        self.assertEqual(enrollment["candidate_actor_registry_sha256"], AUDIT.digest(candidate_actor))
        self.assertEqual(
            enrollment["protected_actor_provenance_sha256"], AUDIT.digest(base_provenance)
        )
        self.assertEqual(
            enrollment["candidate_actor_provenance_sha256"], AUDIT.digest(candidate_provenance)
        )
        for update in enrollment["historical_actor_provenance_pointer_updates"]:
            self.assertTrue(update["changed"])
            self.assertEqual(update["protected_record_sha256"], AUDIT.digest(base_provenance))
            self.assertEqual(update["candidate_record_sha256"], AUDIT.digest(candidate_provenance))

    def test_historical_actor_and_provenance_mutations_are_rejected(self):
        for field, value in (
            ("display_name", "Rebound historical owner"),
            ("principal", self.principal(99, "/root/rebound_owner")),
            ("roles", ["reviewer"]),
        ):
            protected, candidate, *_unused = self.snapshots()
            candidate["actors"]["entries"][0][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(
                ValueError, "rewrites protected actor"
            ):
                AUDIT.actor_enrollment(protected, candidate)

        protected, candidate, *_unused = self.snapshots()
        candidate["provenance"]["entries"][0]["issue"] += 1
        with self.assertRaisesRegex(ValueError, "rewrites protected provenance"):
            AUDIT.actor_enrollment(protected, candidate)

    def test_deletion_duplicate_ids_and_mismatched_id_sets_fail_closed(self):
        protected, candidate, *_unused = self.snapshots()
        candidate["actors"]["entries"] = candidate["actors"]["entries"][:1]
        with self.assertRaisesRegex(ValueError, "deletes protected"):
            AUDIT.actor_enrollment(protected, candidate)

        duplicate_specs = self.BASE + (self.FRESH[0], self.FRESH[0])
        actor_bytes, provenance_bytes = self.documents(duplicate_specs)
        with self.assertRaisesRegex(ValueError, "duplicate actor ID"):
            AUDIT.validate_actor_snapshot(actor_bytes, provenance_bytes, "candidate")

        specs = self.BASE + self.FRESH
        provenance_entries = [self.provenance(spec) for spec in specs]
        provenance_entries[-1]["actor_id"] = provenance_entries[-2]["actor_id"]
        actor_bytes, provenance_bytes = self.documents(specs, provenance_entries)
        with self.assertRaisesRegex(ValueError, "provenance has a malformed or duplicate actor ID"):
            AUDIT.validate_actor_snapshot(actor_bytes, provenance_bytes, "candidate")

        specs = self.BASE + (self.FRESH[0],)
        provenance_entries = [self.provenance(spec) for spec in self.BASE + (self.FRESH[1],)]
        actor_bytes, provenance_bytes = self.documents(specs, provenance_entries)
        with self.assertRaisesRegex(ValueError, "ID sets differ"):
            AUDIT.validate_actor_snapshot(actor_bytes, provenance_bytes, "candidate")

    def test_raw_provenance_hash_pointer_and_unknown_fields_are_rejected(self):
        actor_bytes, provenance_bytes = self.documents(
            self.BASE + self.FRESH, pointer_sha256="0" * 64
        )
        with self.assertRaisesRegex(ValueError, "exact raw provenance bytes"):
            AUDIT.validate_actor_snapshot(actor_bytes, provenance_bytes, "candidate")

        entries = [self.provenance(spec) for spec in self.BASE + self.FRESH]
        entries[-1]["self_asserted_authenticity"] = True
        actor_bytes, provenance_bytes = self.documents(self.BASE + self.FRESH, entries)
        with self.assertRaisesRegex(ValueError, "unknown fields"):
            AUDIT.validate_actor_snapshot(actor_bytes, provenance_bytes, "candidate")

    def test_actor_hash_changes_do_not_replace_the_separate_approval_artifact_check(self):
        history = {"changes": [{"id": "PCR-0005"}]}
        protected = {
            "proof_impact_sha256": "a" * 64,
            "approval_artifacts": {"verification/reviews/PCR-0005.toml": "b" * 64},
            "actor_registry_sha256": "c" * 64,
        }
        candidate = {
            **protected,
            "actor_registry_sha256": "d" * 64,
        }
        result = AUDIT.preserved_approval_history(protected, candidate, history, history)
        self.assertEqual(result["status"], "preserved")

        candidate["approval_artifacts"] = {
            "verification/reviews/PCR-0005.toml": "e" * 64
        }
        with self.assertRaisesRegex(ValueError, "historical approval artifacts changed"):
            AUDIT.preserved_approval_history(protected, candidate, history, history)


if __name__ == "__main__":
    unittest.main(verbosity=2)
