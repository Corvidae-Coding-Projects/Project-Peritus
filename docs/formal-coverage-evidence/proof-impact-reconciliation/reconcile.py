#!/usr/bin/env python3
"""Produce an audit-only reconciliation; never write an approval or verification register.

Uses a built xtask's canonical source discovery on two frozen Git trees. Python 3.12+.
The caller must supply the actual protected base; Git ancestry alone does not prove protection.
"""

import argparse
import base64
import binascii
import collections
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import tomllib


ACTORS_PATH = "verification/actors.toml"
PROVENANCE_PATH = "verification/actor-provenance.json"
REPOSITORY = "Corvidae-Coding-Projects/Project-Peritus"
ACTOR_FIELDS = {"id", "kind", "principal", "display_name", "roles", "provenance"}
PROVENANCE_REQUIRED_FIELDS = {
    "actor_id", "kind", "principal", "repository", "issue", "issue_created_at",
    "session", "task", "mode", "record_locators",
}
PROVENANCE_OPTIONAL_FIELDS = {
    "model", "reasoning_effort", "public_key", "allowed_signer",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(argv, cwd):
    return subprocess.run(argv, cwd=cwd, check=True, capture_output=True).stdout


def git(root, *args):
    return run(["git", *args], root)


def save(directory, name, value):
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    (directory / name).write_bytes(data)
    return digest(data)


def exact_object(value, required, optional, label):
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be an object")
    missing = required - value.keys()
    unknown = value.keys() - required - optional
    if missing or unknown:
        raise ValueError(
            f"{label} has missing fields {sorted(missing)} or unknown fields {sorted(unknown)}"
        )


def unique_json_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"actor provenance JSON repeats field {key!r}")
        value[key] = item
    return value


def non_placeholder(value):
    if not isinstance(value, str):
        return False
    normalized = value.strip().lower()
    unfinished = "to" + "do"
    return (len(value.strip()) >= 4 and normalized not in {
        "n/a", "na", "none", "tbd", unfinished, "unknown", "placeholder",
    } and unfinished not in normalized and "placeholder" not in normalized)


def valid_actor_id(value):
    return isinstance(value, str) and re.fullmatch(r"ACTOR-(?!0000)[0-9]{4}", value) is not None


def positive_integer(value):
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def valid_issue_time(value):
    return (isinstance(value, str) and 20 <= len(value) <= 40 and value.endswith("Z")
            and "T" in value and not any(character.isspace() for character in value))


def normal_component(value, maximum, punctuation):
    return (value and len(value) <= maximum and value.isascii()
            and value[0].isalnum() and value[-1].isalnum()
            and all(character.isalnum() or character in punctuation for character in value))


def crosslink_key_matches(actor, provenance):
    public_key = provenance.get("public_key")
    allowed_signer = provenance.get("allowed_signer")
    if not isinstance(public_key, str) or not isinstance(allowed_signer, str):
        return False
    if not public_key.isascii() or not allowed_signer.isascii():
        return False
    public = public_key.split()
    signer = allowed_signer.split()
    if (len(public) != 3 or len(signer) != 4 or public[0] != "ssh-ed25519"
            or signer[1:3] != public[:2] or signer[3] != public[2]):
        return False
    identity = public[2].removeprefix("crosslink-agent:")
    if identity == public[2] or identity.count("@") != 1:
        return False
    agent_id, machine = identity.split("@")
    if (signer[0] != f"{agent_id}@crosslink"
            or not normal_component(agent_id, 64, "-_")
            or len(machine) > 253
            or not all(normal_component(label, 63, "-") for label in machine.split("."))):
        return False
    try:
        key_blob = base64.b64decode(public[1], validate=True)
    except (ValueError, binascii.Error):
        return False
    canonical_key = base64.b64encode(key_blob).decode()
    valid_blob = (len(key_blob) == 51 and key_blob[:4] == (11).to_bytes(4, "big")
                  and key_blob[4:15] == b"ssh-ed25519"
                  and key_blob[15:19] == (32).to_bytes(4, "big"))
    fingerprint = base64.b64encode(hashlib.sha256(key_blob).digest()).decode().rstrip("=")
    return (valid_blob and public[1] == canonical_key
            and actor["principal"] == f"SHA256:{fingerprint}")


def validate_actor_snapshot(actor_bytes, provenance_bytes, label):
    try:
        actors = tomllib.loads(actor_bytes.decode())
        provenance = json.loads(provenance_bytes, object_pairs_hook=unique_json_object)
    except (UnicodeDecodeError, tomllib.TOMLDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"{label} actor documents do not match their schemas: {error}") from error
    envelope = {"schema", "schema_version", "baseline", "entries"}
    exact_object(actors, envelope, set(), f"{label} actor registry")
    exact_object(provenance, envelope, set(), f"{label} actor provenance")
    expected_envelopes = (
        (actors, "peritus.verification.actors"),
        (provenance, "peritus.verification.actor-provenance"),
    )
    for document, schema in expected_envelopes:
        if (document["schema"] != schema or not positive_integer(document["schema_version"])
                or document["schema_version"] != 1 or document["baseline"] != "A1"
                or not isinstance(document["entries"], list)):
            raise ValueError(f"{label} actor document has an unexpected schema envelope")

    provenance_sha256 = digest(provenance_bytes)
    actor_ids, subjects = set(), set()
    for index, actor in enumerate(actors["entries"]):
        exact_object(actor, ACTOR_FIELDS, set(), f"{label} actor entry {index}")
        actor_id = actor["id"]
        if not valid_actor_id(actor_id) or actor_id in actor_ids:
            raise ValueError(f"{label} actor registry has a malformed or duplicate actor ID {actor_id!r}")
        actor_ids.add(actor_id)
        if actor["kind"] not in ("crosslink-agent", "codex-subagent"):
            raise ValueError(f"{label} actor {actor_id!r} has an unknown kind")
        principal = actor["principal"]
        valid_principal = isinstance(principal, str) and principal.isascii() and bool(principal)
        if actor["kind"] == "crosslink-agent":
            valid_principal = (isinstance(principal, str)
                               and re.fullmatch(r"SHA256:[A-Za-z0-9+/]{43}", principal) is not None)
        if not valid_principal or not non_placeholder(principal) or not non_placeholder(actor["display_name"]):
            raise ValueError(f"{label} actor {actor_id!r} has malformed identity text")
        subject = (actor["kind"], principal)
        if subject in subjects:
            raise ValueError(f"{label} actor registry repeats provider subject {subject!r}")
        subjects.add(subject)
        roles = actor["roles"]
        if (not isinstance(roles, list) or not roles
                or any(not isinstance(role, str) for role in roles)
                or roles != sorted(set(roles))
                or any(role not in ("owner", "reviewer") for role in roles)):
            raise ValueError(f"{label} actor {actor_id!r} has a malformed role set")
        reference = actor["provenance"]
        exact_object(reference, {"record_path", "record_sha256"}, set(),
                     f"{label} actor {actor_id!r} provenance reference")
        if (reference["record_path"] != PROVENANCE_PATH
                or reference["record_sha256"] != provenance_sha256):
            raise ValueError(
                f"{label} actor {actor_id!r} does not reference the exact raw provenance bytes"
            )

    provenance_ids = set()
    provenance_by_id = {}
    for index, entry in enumerate(provenance["entries"]):
        exact_object(entry, PROVENANCE_REQUIRED_FIELDS, PROVENANCE_OPTIONAL_FIELDS,
                     f"{label} provenance entry {index}")
        actor_id = entry["actor_id"]
        if not valid_actor_id(actor_id) or actor_id in provenance_ids:
            raise ValueError(
                f"{label} provenance has a malformed or duplicate actor ID {actor_id!r}"
            )
        provenance_ids.add(actor_id)
        provenance_by_id[actor_id] = entry
    if actor_ids != provenance_ids:
        raise ValueError(
            f"{label} actor/provenance ID sets differ: actors={sorted(actor_ids)}, "
            f"provenance={sorted(provenance_ids)}"
        )
    if not any("owner" in actor["roles"] for actor in actors["entries"]):
        raise ValueError(f"{label} actor registry has no owner")
    if not any("reviewer" in actor["roles"] for actor in actors["entries"]):
        raise ValueError(f"{label} actor registry has no reviewer")

    efforts = {"low", "medium", "high", "xhigh", "max", "ultra"}
    for actor in actors["entries"]:
        actor_id = actor["id"]
        entry = provenance_by_id[actor_id]
        task = entry["task"]
        valid_task = (isinstance(task, str) and (task == "/root" or (
            task.isascii() and task.startswith("/root/") and len(task) > len("/root/")
            and all(character.isalnum() or character in "/-_" for character in task[6:])
        )))
        model = entry.get("model")
        effort = entry.get("reasoning_effort")
        optional_types = (model is None or isinstance(model, str)) and (
            effort is None or isinstance(effort, str) and effort in efforts
        ) and (entry.get("public_key") is None or isinstance(entry.get("public_key"), str)) and (
            entry.get("allowed_signer") is None
            or isinstance(entry.get("allowed_signer"), str)
        )
        expected_principal = f"{entry['repository']}/session/{entry['session']}/task{task}"
        role_mode = "read-only-review" if "reviewer" in actor["roles"] else "implementation"
        if (entry["kind"] != actor["kind"] or entry["principal"] != actor["principal"]
                or entry["repository"] != REPOSITORY or not positive_integer(entry["issue"])
                or not positive_integer(entry["session"]) or not valid_issue_time(entry["issue_created_at"])
                or not valid_task or not optional_types or entry["mode"] != role_mode
                or (model is not None and not non_placeholder(model))):
            raise ValueError(f"{label} actor {actor_id!r} has malformed or mismatched provenance")
        locators = entry["record_locators"]
        if (not isinstance(locators, list) or not locators
                or any(not isinstance(locator, str) for locator in locators)
                or locators != sorted(set(locators))):
            raise ValueError(f"{label} actor {actor_id!r} has malformed record locators")
        if actor["kind"] == "codex-subagent":
            valid_locator = (actor["principal"] == expected_principal
                             and entry.get("public_key") is None
                             and entry.get("allowed_signer") is None
                             and locators == [f"codex-collaboration:{actor['principal']}"])
        else:
            valid_locator = (locators == ["embedded:allowed-signer", "embedded:public-key"]
                             and crosslink_key_matches(actor, entry))
        if not valid_locator:
            raise ValueError(f"{label} actor {actor_id!r} has a malformed provider locator")
        if "reviewer" in actor["roles"] and (model != "gpt-5.6-sol" or effort != "xhigh"):
            raise ValueError(f"{label} reviewer {actor_id!r} lacks the required model provenance")

    return {
        "registry_sha256": digest(actor_bytes),
        "provenance_sha256": provenance_sha256,
        "actors": actors,
        "provenance": provenance,
    }


def actor_enrollment(protected, candidate):
    base_actors = protected["actors"]["entries"]
    current_actors = candidate["actors"]["entries"]
    base_provenance = protected["provenance"]["entries"]
    current_provenance = candidate["provenance"]["entries"]
    if (protected["actors"]["schema"], protected["actors"]["schema_version"],
        protected["actors"]["baseline"]) != (
            candidate["actors"]["schema"], candidate["actors"]["schema_version"],
            candidate["actors"]["baseline"]):
        raise ValueError("candidate rewrites the protected actor registry envelope")
    if (protected["provenance"]["schema"], protected["provenance"]["schema_version"],
        protected["provenance"]["baseline"]) != (
            candidate["provenance"]["schema"], candidate["provenance"]["schema_version"],
            candidate["provenance"]["baseline"]):
        raise ValueError("candidate rewrites the protected actor provenance envelope")
    if len(current_actors) < len(base_actors) or len(current_provenance) < len(base_provenance):
        raise ValueError("candidate deletes protected actor or provenance entries")

    pointer_updates = []
    for index, historical in enumerate(base_actors):
        current = current_actors[index]
        before = {**historical, "provenance": dict(historical["provenance"])}
        after = {**current, "provenance": dict(current["provenance"])}
        previous_sha256 = before["provenance"].pop("record_sha256")
        current_sha256 = after["provenance"].pop("record_sha256")
        if before != after:
            raise ValueError(f"candidate rewrites protected actor {historical['id']!r}")
        pointer_updates.append({
            "actor_id": historical["id"],
            "protected_record_sha256": previous_sha256,
            "candidate_record_sha256": current_sha256,
            "changed": previous_sha256 != current_sha256,
        })
    for index, historical in enumerate(base_provenance):
        if current_provenance[index] != historical:
            raise ValueError(
                f"candidate rewrites protected provenance for {historical['actor_id']!r}"
            )

    appended_actors = current_actors[len(base_actors):]
    appended_provenance = current_provenance[len(base_provenance):]
    appended_actor_ids = [actor["id"] for actor in appended_actors]
    appended_provenance_ids = [entry["actor_id"] for entry in appended_provenance]
    if set(appended_actor_ids) != set(appended_provenance_ids):
        raise ValueError("candidate appended actor/provenance ID sets differ")
    provenance_by_id = {entry["actor_id"]: entry for entry in appended_provenance}
    return {
        "status": "pending-independent-review",
        "authority": "audit-only",
        "authenticity_status": "unverified-self-reported-provenance",
        "protected_actor_registry_sha256": protected["registry_sha256"],
        "candidate_actor_registry_sha256": candidate["registry_sha256"],
        "protected_actor_provenance_sha256": protected["provenance_sha256"],
        "candidate_actor_provenance_sha256": candidate["provenance_sha256"],
        "protected_actor_count": len(base_actors),
        "candidate_actor_count": len(current_actors),
        "appended_actor_ids": appended_actor_ids,
        "appended_provenance_ids": appended_provenance_ids,
        "appended_actors": [
            {"actor": actor, "provenance": provenance_by_id[actor["id"]]}
            for actor in appended_actors
        ],
        "historical_actor_provenance_pointer_updates": pointer_updates,
        "review_requirement": (
            "The candidate records are structurally valid append-only data, not authenticated "
            "identities or approval; the authorization checker and an independent source-bound "
            "review must validate them."
        ),
    }


def materialize(root, commit, destination):
    # Read raw blobs, independent of checkout filters and archive export attributes.
    entries = git(root, "ls-tree", "-rz", "--full-tree", commit).split(b"\0")
    expected = {}
    for entry in filter(None, entries):
        header, raw_path = entry.split(b"\t", 1)
        mode, kind, object_id = header.split()
        path = raw_path.decode()
        if mode not in (b"100644", b"100755") or kind != b"blob":
            raise ValueError(f"unsupported Git object: {path}")
        if PurePosixPath(path).is_absolute() or any(
            part in ("", ".", "..") or part.lower() == ".git" for part in path.split("/")
        ) or "\\" in path:
            raise ValueError(f"non-normal Git path: {path}")
        if path in expected or len(expected) >= 30_000:
            raise ValueError("duplicate or excessive Git tree entries")
        expected[path] = object_id
    hashes, total = {}, 0
    with subprocess.Popen(
        ["git", "cat-file", "--batch"], cwd=root,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    ) as process:
        for path, object_id in expected.items():
            process.stdin.write(object_id + b"\n")
            process.stdin.flush()
            oid, kind, raw_size = process.stdout.readline().split()
            size = int(raw_size)
            total += size
            if oid != object_id or kind != b"blob" or size < 0 or total > 512 * 1024 * 1024:
                raise ValueError("invalid or excessive Git blob data")
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            sha256 = hashlib.sha256()
            git_hash = hashlib.sha1(b"blob " + raw_size + b"\0", usedforsecurity=False)
            remaining = size
            with target.open("xb") as stream:
                while remaining:
                    chunk = process.stdout.read(min(remaining, 65536))
                    if not chunk:
                        raise ValueError("truncated Git blob")
                    stream.write(chunk)
                    sha256.update(chunk)
                    git_hash.update(chunk)
                    remaining -= len(chunk)
            if process.stdout.read(1) != b"\n" or git_hash.hexdigest().encode() != object_id:
                raise ValueError("Git blob identity mismatch")
            hashes[path] = sha256.hexdigest()
        process.stdin.close()
        if process.wait() != 0:
            raise ValueError("Git blob materialization failed")
    return hashes


def read_tree(root, commit, binary, output, label):
    with tempfile.TemporaryDirectory(prefix="peritus-impact-reconcile-") as temporary:
        tree = Path(temporary)
        hashes = materialize(root, commit, tree)
        inventory = json.loads(run([str(binary), "proof-impact-inventory"], tree))
        envelope = {
            "schema": "peritus.proof-impact-inventory", "schema_version": 1,
            "status": "audit-only", "hash_algorithm": "sha256-raw-bytes-v1",
        }
        if any(inventory.get(key) != value for key, value in envelope.items()):
            raise ValueError("unexpected inventory schema or authority")
        for path, snapshot in inventory["sources"].items():
            if hashes[path] != snapshot["sha256"]:
                raise ValueError(f"inventory differs from Git bytes: {path}")
        manifest_bytes = (tree / "verification/proof-impact.toml").read_bytes()
        manifest = tomllib.loads(manifest_bytes.decode())
        obligations = tomllib.loads((tree / "verification/obligations.toml").read_text())
        approval_artifacts = {}
        for path in sorted((tree / "verification/reviews").rglob("*")):
            if path.is_file():
                approval_artifacts[str(path.relative_to(tree))] = digest(path.read_bytes())
        actor_bytes = (tree / ACTORS_PATH).read_bytes()
        provenance_bytes = (tree / PROVENANCE_PATH).read_bytes()
        actors = validate_actor_snapshot(actor_bytes, provenance_bytes, label)
        identity = {
            "commit": commit,
            "tree": git(root, "rev-parse", f"{commit}^{{tree}}").decode().strip(),
            "inventory_sha256": save(output, label + "-inputs.json", inventory),
            "proof_impact_sha256": digest(manifest_bytes),
            "approval_artifacts": approval_artifacts,
            "actor_registry_sha256": actors["registry_sha256"],
            "actor_provenance_sha256": actors["provenance_sha256"],
        }
        return identity, inventory["sources"], manifest, obligations["entries"], actors


def transitions(previous, current):
    return [
        {"source_file": path, "previous": previous.get(path), "current": current.get(path)}
        for path in sorted(previous.keys() | current.keys())
        if previous.get(path) != current.get(path)
    ]


def preserved_approval_history(protected, candidate, history, current_history):
    if (candidate["proof_impact_sha256"] != protected["proof_impact_sha256"]
            or current_history != history):
        raise ValueError("protected proof-impact history changed; reconciliation is refused")
    if candidate["approval_artifacts"] != protected["approval_artifacts"]:
        raise ValueError("historical approval artifacts changed; reconciliation is refused")
    return {
        "status": "preserved",
        "protected_artifact_count": len(protected["approval_artifacts"]),
        "candidate_artifact_count": len(candidate["approval_artifacts"]),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--xtask", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    for value in (args.base, args.candidate):
        if not re.fullmatch(r"[0-9a-f]{40}", value):
            parser.error("use full lowercase commit identities")
        if git(root, "cat-file", "-t", value) != b"commit\n":
            parser.error("object identity must name a commit directly, not a tag")
    if args.base == args.candidate:
        parser.error("protected base must differ from candidate")
    git(root, "merge-base", "--is-ancestor", args.base, args.candidate)
    binary = args.xtask.resolve(strict=True)
    destination = args.output.absolute()
    if os.path.lexists(destination):
        parser.error("output must be a fresh path; existing files, directories, and symlinks are refused")
    with tempfile.TemporaryDirectory(prefix="peritus-impact-output-") as temporary:
        private = Path(temporary)
        frozen_binary = private / "xtask"
        before = digest(binary.read_bytes())
        shutil.copy2(binary, frozen_binary)
        if digest(frozen_binary.read_bytes()) != before or digest(binary.read_bytes()) != before:
            raise ValueError("inventory executable changed while being frozen")
        output = private / "artifacts"
        output.mkdir()
        summary = reconcile(root, args, frozen_binary, output)
        # copytree exclusively creates the output directory and refuses existing symlinks.
        # Validation failures leave no published result. An I/O failure is a failed command.
        shutil.copytree(output, destination)
        print(json.dumps(summary, sort_keys=True))


def reconcile(root, args, binary, output):
    base_id, base, history, base_obligations, base_actors = read_tree(
        root, args.base, binary, output, "protected"
    )
    candidate_id, candidate, current_history, obligations, candidate_actors = read_tree(
        root, args.candidate, binary, output, "candidate"
    )
    approval_preservation = preserved_approval_history(
        base_id, candidate_id, history, current_history
    )
    enrollment = actor_enrollment(base_actors, candidate_actors)
    declared = {
        item["source_file"]: {key: item[key] for key in ("sha256", "affected_packages")}
        for item in history["sources"]
    }
    delta = transitions(declared, candidate)
    impacted = {}
    for change in delta:
        for snapshot in (change["previous"], change["current"]):
            for package in (snapshot or {}).get("affected_packages", []):
                name, classification = package["package"], package["verification_class"]
                if name in impacted and impacted[name] != classification:
                    raise ValueError(f"classification changed for {name}; needs separate policy review")
                impacted[name] = classification
    gates = []
    for package, classification in sorted(impacted.items()):
        common = f"--package {package} --all-features --locked"
        strict = "--no-cheating " if classification in ("V", "H") else ""
        for kind, command in (
            ("ordinary-test", f"cargo test --package {package} --all-targets --all-features --locked"),
            ("verus-verify", f"cargo verus verify {common} --check-toolchain --fwd-verus-args-to roots -- {strict}--rlimit 20"),
        ):
            gates.append({"kind": kind, "owning_crate": package, "command": command,
                          "status": "required; no final-candidate pass claimed"})
    old_obligations = {entry["id"]: entry for entry in base_obligations}
    obligation_rows = []
    for entry in obligations:
        previous = old_obligations.get(entry["id"])
        paths = sorted({entry["source_file"]} | {item["source_file"] for item in entry["evidence"]})
        hashes = {path: digest(git(root, "show", f"{args.candidate}:{path}")) for path in paths}
        changed = []
        for path in paths:
            before = subprocess.run(
                ["git", "show", f"{args.base}:{path}"], cwd=root, capture_output=True
            )
            if before.returncode != 0 or digest(before.stdout) != hashes[path]:
                changed.append(path)
        obligation_rows.append({
            "id": entry["id"], "owning_crate": entry["owning_crate"], "owner": entry["owner"],
            "protected_status": previous["status"] if previous else None,
            "candidate_status": entry["status"], "declaration_changed": previous != entry,
            "final_candidate_discharge_review": None,
            "source_sha256": hashes,
            "evidence_paths_outside_formal_impact_inventory": [path for path in paths if path not in candidate],
            "changed_sources_since_protected_base": changed,
            "reason": "Retained proof/test locators and patch reviews are not final-candidate discharge authorization.",
        })
    proposal = {
        "schema": "peritus.proof-impact-reconciliation", "schema_version": 1,
        "status": "pending-authorization", "authority": "audit-only",
        "protected_base": base_id, "candidate": candidate_id,
        "inventory_tool_binary_sha256": digest(binary.read_bytes()),
        "approved_history_preserved": True,
        "approval_artifact_preservation": approval_preservation,
        "candidate_actor_enrollment": enrollment,
        "approved_change_ids": [change["id"] for change in history["changes"]],
        "source_transitions": delta,
        "protected_record_drift": transitions(declared, base),
        "branch_source_transitions": transitions(base, candidate),
        "affected_packages": impacted, "required_gates": gates,
        "obligations": obligation_rows,
        "removed_obligation_ids": sorted(old_obligations.keys() - {entry["id"] for entry in obligations}),
        "summary": {
            "protected_record_sources": len(declared), "protected_actual_sources": len(base),
            "candidate_sources": len(candidate), "proposed_transitions": len(delta),
            "inherited_record_transitions": len(transitions(declared, base)),
            "branch_transitions": len(transitions(base, candidate)),
            "affected_packages": len(impacted), "required_gates": len(gates),
            "appended_actor_records": len(enrollment["appended_actor_ids"]),
            "actor_enrollment_status": enrollment["status"],
            "obligation_statuses": dict(collections.Counter(entry["status"] for entry in obligations)),
        },
        "remaining_requirements": [
            "Retain fresh actual owner/reviewer provenance before freezing an approval candidate; never relabel historical actors.",
            "Run and retain every exact package gate on the frozen candidate and obtain a source-bound independent verdict.",
            "Append that verdict and exact transitions through the authorization-only protected-base phase, then apply those exact sources.",
            "Rerun against the resulting protected base; deploy trusted checker authority and qualify hosted CI separately.",
        ],
    }
    save(output, "reconciliation.json", proposal)
    return proposal["summary"]


if __name__ == "__main__":
    main()
