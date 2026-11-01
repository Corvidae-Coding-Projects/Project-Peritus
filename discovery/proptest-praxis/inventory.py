#!/usr/bin/env python3
"""Inventory actual manifests and source references without claiming source review."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[2]


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def sha(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write("\n")


# Explicit ownership from the design; source paths and call references are discovered below.
GROUPS = [
    ("peritus-types peritus-codec peritus-protocol peritus-plugin-sdk", "protocol", [5],
     "Checked identity/byte/grammar semantics and canonical field preservation", "Explicit primitives and independently encoded semantic records"),
    ("peritus-budget peritus-policy peritus-kernel", "authority", [6],
     "Reservation conservation, exact authority and causal lifecycle transitions", "Wider arithmetic, declarative scope table, explicit phase model"),
    ("peritus-spec peritus-quality-policy peritus-security-policy peritus-release-policy", "acceptance", [3, 6, 8],
     "Dependency completeness and exact revision/evidence/review acceptance", "Graph enumeration and independent Boolean evidence truth table"),
    ("peritus-tcb", "static-contract", [1, 8],
     "Exported static manifest-path vocabulary and current consumer meaning", "Exact declared strings plus explicit historical/active classification"),
    ("peritus-journal peritus-projection", "durability", [4],
     "Append/CAS/idempotency/outbox and projection prefix publication", "Append-only list and independent sequential semantic fold"),
    ("peritus-artifact-store peritus-evidence", "durability", [4],
     "Exact bytes/reference GC, causal and freshness completeness", "Bytes map, root sets and independently traversed provenance DAG"),
    ("peritus-leases peritus-approval peritus-migrations", "durability", [4, 6],
     "Exclusive versioned ownership, authenticated exact use and supported forward migration", "Lease/approval tables, supported interval enumeration and actual reopened state"),
    ("peritus-context peritus-memory peritus-run-knowledge peritus-role", "context", [2],
     "Required dependency closure, invalidation, retrieval and role isolation", "BFS closure, full recomputation and explicit role/source map"),
    ("peritus-agent peritus-orchestrator", "execution", [3, 4],
     "Separate pure/durable agent and orchestrator phases and effect ownership", "Independent phase table, exact committed effect counter"),
    ("peritus-gates peritus-review peritus-obligations peritus-run-settlement", "execution", [3, 8],
     "Exact check/evidence accounting, finding conservation and terminal settlement", "Command multiset, finding map, Boolean clauses and checkpoint table"),
    ("peritus-scheduler peritus-collaboration", "coordination", [4],
     "Feasible resource assignment, dispatch ownership and descendant joins", "Exhaustive small assignments, independent resource sum and parent traversal"),
    ("peritus-harness peritus-modules", "coordination", [4, 8],
     "Dependency-ordered materialization, exact package/job/stage identity", "Plain path-content map, dependency DAG and ordered stage model"),
    ("peritus-model-protocol peritus-provider-core peritus-provider-openai peritus-provider-anthropic peritus-provider-google peritus-provider-compatible peritus-provider-opencode", "provider", [5],
     "Selected request identity, complete normalized stream, usage and cancellation", "Constructed message grammar and explicit request/response phase table"),
    ("peritus-tool-protocol peritus-tool-router", "tools", [5, 6],
     "Exact schema/call/authority/replay identity and dispatch ownership", "Simple action-call-state map and actual effect counter"),
    ("peritus-tools-fs peritus-tools-git peritus-tools-shell peritus-tools-quality", "tools", [6],
     "Exact bytes/path/argv/check observations with truthful completion", "Plain tree/raw Git/echo fixture and independently parsed expected check semantics"),
    ("peritus-tools-web peritus-tools-computer", "tools", [6, 7],
     "Immutable source paging/cache and browser/desktop session/observation ownership", "Full source map, clock/origin model and independent session state machine"),
    ("peritus-patch peritus-workspace peritus-git", "runtime", [6],
     "Preimage/candidate/rollback/restart correspondence and failure atomicity where promised", "Virtual tree plus actual disposable files and raw Git observations"),
    ("peritus-process", "runtime", [6],
     "Exact output retention and owned process/PTY lifecycle", "Independent byte-window arithmetic and actual owned child observations"),
    ("peritus-sandbox peritus-sandbox-linux peritus-sandbox-macos peritus-sandbox-windows", "native", [6],
     "Plan admission and platform-native permitted/denied effects", "Independent capability matrix plus OS-specific observed effect attempts"),
    ("peritus-network peritus-secrets peritus-local-socket", "runtime", [6],
     "Destination/lease/custody authority, redirects, expiry and accounting", "Explicit address and lease tables, substring scan and actual ownership observations"),
    ("peritus-plugin-host peritus-mcp", "extensions", [5],
     "Bounded trust/quota/lifecycle and non-authoritative framed requests", "Permit counter table, explicit grammar and downstream authority observations"),
    ("peritus-app-protocol peritus-app-client", "clients", [5, 7],
     "Wire payload correspondence and exact request/response/reconnect bindings", "Explicit semantic payloads and independent correlation/operation ledger"),
    ("peritus-product-state peritus-product-runner peritus-daemon", "product", [2, 3, 4, 7],
     "Input/candidate/receipt/context and interactive recovery ownership", "Separate input/phase/effect models with real disposable daemon readback"),
    ("peritus-cli peritus-tui peritus-web peritus-launcher peritus-provider-onboarding", "clients", [7],
     "Arguments/text/async operations, bootstrap and exact setup choices", "Argument grammar, text/cursor and per-session operation model"),
    ("peritus-debugger peritus-eval peritus-evolution", "analysis", [8],
     "Causal citations/clusters, exact statistical accounting and eligible promotion", "Exact small calculations, equivalence classes and exhaustive variant ordering"),
    ("peritus-trace peritus-telemetry", "observability", [8],
     "Causal observation and bounded queue/export/disposition correctness", "Span map and independently recomputed list queue/disposition model"),
    ("peritus-external-benchmarks peritus-platform-qualification peritus-release-artifacts peritus-release-operator xtask", "delivery", [7, 8],
     "Truthful parser/manifest/install/release result and exact artifact binding", "Explicit expected files/digests/results, native readback where required"),
]


def collect(root):
    workspace = tomllib.loads((root / "Cargo.toml").read_text())
    packages = {}
    for member in workspace["workspace"]["members"]:
        data = tomllib.loads((root / member / "Cargo.toml").read_text())
        packages[data["package"]["name"]] = (member, data)
    return packages


def inventory(output, source_checkout):
    output.mkdir(mode=0o700)
    packages = collect(ROOT)
    assigned = {}
    for names, lane, stages, contract, oracle in GROUPS:
        for name in names.split():
            if name in assigned:
                raise ValueError(f"duplicate package {name}")
            assigned[name] = (lane, stages, contract, oracle)
    if set(assigned) != set(packages):
        raise ValueError(f"package drift: missing={set(packages)-set(assigned)}, stale={set(assigned)-set(packages)}")

    tracked = git(ROOT, "ls-files", "-z").decode().split("\0")[:-1]
    source_files = [path for path in tracked if path.endswith(".rs") and (path.startswith("crates/") or path.startswith("xtask/"))]
    reference_index = {name: [] for name in packages}
    needles = {name.replace("-", "_") + "::": name for name in packages}
    for relative in source_files:
        for line_number, line in enumerate((ROOT / relative).read_text().splitlines(), 1):
            for needle in re.findall(r"\b(peritus_[a-z0-9_]+)::", line):
                name = needles.get(needle + "::")
                if name and len(reference_index[name]) < 8:
                    reference_index[name].append({"path": relative, "line": line_number, "kind": "lexical-reference-not-traced-call"})

    entries = []
    for name, (member, data) in packages.items():
        lane, stages, contract, oracle = assigned[name]
        directory = ROOT / member
        source = sorted(p.relative_to(ROOT).as_posix() for p in (directory / "src").rglob("*.rs"))
        roots = [p for p in source if p in (member + "/src/lib.rs", member + "/src/main.rs")]
        errors = [p for p in source if Path(p).name in ("error.rs", "errors.rs", "failure.rs")]
        callers = []
        for candidate, (caller_path, caller_data) in packages.items():
            dependencies = dict(caller_data.get("dependencies", {}))
            dependencies.update(caller_data.get("dev-dependencies", {}))
            for cfg in caller_data.get("target", {}).values():
                dependencies.update(cfg.get("dependencies", {}))
            if any((value.get("package", key) if isinstance(value, dict) else key) == name for key, value in dependencies.items()):
                callers.append({"package": candidate, "manifest": caller_path + "/Cargo.toml"})
        entries.append({
            "package": name, "manifest": member + "/Cargo.toml", "owner_lane": lane,
            "stages": stages, "status": "inventoried", "contract": contract,
            "independent_oracle": oracle, "entrypoints": roots, "error_paths_to_trace": errors,
            "caller_manifests": callers, "candidate_source_references": reference_index[name],
            "source_files": source, "source_file_count": len(source),
            "source_digest": sha(b"".join(p.encode()+b"\0"+(ROOT/p).read_bytes() for p in source)),
            "source_review": "inventory only; trace selected implementation, caller and error path before promoting to contract-read",
            "harness": None, "native_pending": lane == "native", "last_run": None,
        })
    save(output / "targets.json", {"schema_version": 1, "source_head": git(ROOT, "rev-parse", "HEAD").decode().strip(), "packages": entries})

    surfaces = []
    for relative in tracked:
        path = ROOT / relative
        suffix = path.suffix.lower()
        language = {".py": "python", ".js": "javascript", ".ts": "typescript", ".svelte": "svelte", ".sh": "shell", ".ps1": "powershell"}.get(suffix)
        if language is None and path.is_file() and path.stat().st_size < 1024 * 1024:
            prefix = path.read_bytes()[:128]
            if prefix.startswith(b"#!"):
                language = "shebang-script"
        if language:
            classification = "runtime-or-tooling"
            if relative.startswith("docs/"):
                classification = "retained-reproducer-or-historical-evidence"
            elif relative.startswith(("benchmarks/", "fixtures/", "compat/")):
                classification = "campaign-adapter-or-fixture"
            elif relative.startswith(".claude/"):
                classification = "local-integration-hook"
            surfaces.append({"path": relative, "language": language, "classification": classification,
                             "sha256": sha(path.read_bytes()), "status": "inventoried", "stage": 7 if classification == "runtime-or-tooling" else 1})
    save(output / "surfaces.json", {"schema_version": 1, "surfaces": surfaces,
         "note": "Classifications identify inventory routing, not proof a retained script is active. Trace packaging/loaders before runtime qualification."})

    tracked_digest = hashlib.sha256()
    for relative in tracked:
        tracked_digest.update(relative.encode() + b"\0" + (ROOT / relative).read_bytes())
    save(output / "baseline.json", {"schema_version": 1,
         "selected_source_head": git(ROOT, "rev-parse", "HEAD").decode().strip(),
         "selection": "committed develop production source; only owned discovery navigation documents differ; pre-existing dirty work excluded and preserved in original checkout",
         "documentation_delta":git(ROOT,"diff","--name-only","HEAD","--","AGENTS.md",".design/AGENTS.md").decode().splitlines(),
         "tracked_source_tree_sha256": tracked_digest.hexdigest(),
         "tracked_file_count": len(tracked), "active_packages": len(entries),
         "additional_code_surfaces": len(surfaces), "root_lock_sha256": sha((ROOT/"Cargo.lock").read_bytes()),
         "harness_lock_sha256": sha((Path(__file__).parent/"Cargo.lock").read_bytes()),
         "toolchain": (ROOT/"rust-toolchain.toml").read_text(),
         "production_delta": git(ROOT,"diff","--name-only","HEAD","--","crates","Cargo.toml","Cargo.lock").decode().splitlines()})

    if source_checkout:
        paths = set(git(source_checkout, "diff", "--name-only", "HEAD").decode().splitlines())
        paths.update(git(source_checkout, "ls-files", "--others", "--exclude-standard", "-z").decode().split("\0")[:-1])
        ignored = git(source_checkout, "ls-files", "--others", "--ignored", "--exclude-standard", "-z", "--", "crates/tools/peritus-tools-computer").decode().split("\0")[:-1]
        paths.update(ignored)
        saved = [{"path": p, "sha256": sha((source_checkout/p).read_bytes()) if (source_checkout/p).is_file() else None} for p in sorted(paths)]
        save(output/"preserved-checkout.json", {"schema_version": 1, "source_head":git(source_checkout,"rev-parse","HEAD").decode().strip(), "paths":saved, "ignored_build_input_candidates":ignored,
             "note":"Path/hash inventory only; originals were neither changed nor added to the campaign baseline."})
    print(json.dumps({"packages":len(entries),"surfaces":len(surfaces),"output":str(output)}))


def verify(output, source_checkout):
    targets = json.loads((output/"targets.json").read_text())
    current = collect(ROOT)
    actual = [row["package"] for row in targets["packages"]]
    if len(actual) != len(set(actual)) or set(actual) != set(current):
        raise ValueError("target inventory does not match workspace")
    for row in targets["packages"]:
        for path in row["entrypoints"] + row["error_paths_to_trace"] + row["source_files"]:
            if not (ROOT/path).is_file():
                raise ValueError(f"missing source path {path}")
        actual_digest=sha(b"".join(p.encode()+b"\0"+(ROOT/p).read_bytes() for p in row["source_files"]))
        if actual_digest != row["source_digest"]:
            raise ValueError(f"source digest changed: {row['package']}")
    baseline=json.loads((output/"baseline.json").read_text())
    if baseline["selected_source_head"] != git(ROOT,"rev-parse","HEAD").decode().strip():
        raise ValueError("baseline HEAD changed")
    for filename,field in [(ROOT/"Cargo.lock","root_lock_sha256"),(Path(__file__).parent/"Cargo.lock","harness_lock_sha256")]:
        if sha(filename.read_bytes()) != baseline[field]:
            raise ValueError(f"lock fingerprint changed: {filename}")
    tracked=git(ROOT,"ls-files","-z").decode().split("\0")[:-1]
    tree=hashlib.sha256()
    for relative in tracked:
        tree.update(relative.encode()+b"\0"+(ROOT/relative).read_bytes())
    if tree.hexdigest() != baseline["tracked_source_tree_sha256"]:
        raise ValueError("tracked baseline tree changed")
    for surface in json.loads((output/"surfaces.json").read_text())["surfaces"]:
        if sha((ROOT/surface["path"]).read_bytes()) != surface["sha256"]:
            raise ValueError(f"surface changed: {surface['path']}")
    delta=git(ROOT,"diff","--name-only","HEAD","--","crates","Cargo.toml","Cargo.lock").decode().splitlines()
    if delta != baseline["production_delta"]:
        raise ValueError("production delta changed")
    if source_checkout:
        saved=json.loads((output/"preserved-checkout.json").read_text())
        for row in saved["paths"]:
            path=source_checkout/row["path"]
            if row["sha256"] is not None and sha(path.read_bytes()) != row["sha256"]:
                raise ValueError(f"original checkout changed: {row['path']}")
    print(json.dumps({"status":"pass","packages":len(actual),"preserved_checkout_verified":bool(source_checkout)}))


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command",choices=["capture","verify"])
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--source-checkout",type=Path)
    args=parser.parse_args()
    (inventory if args.command=="capture" else verify)(args.output,args.source_checkout)
