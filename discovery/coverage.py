#!/usr/bin/env python3
"""Account for every active package without confusing source association with testing."""

import argparse
import importlib.util
import json
from pathlib import Path
import re
import tomllib


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent


def references(path):
    return set(re.findall(r"\b(peritus_[a-z0-9_]+)::", path.read_text()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("praxis_inventory", HERE / "proptest-praxis/inventory.py")
    inventory = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(inventory)
    packages = inventory.collect(ROOT)
    lanes = {name: (lane, contract, oracle)
             for names, lane, _, contract, oracle in inventory.GROUPS for name in names.split()}
    if set(lanes) != set(packages):
        raise ValueError(f"package mapping drift: {set(packages) ^ set(lanes)}")
    consumers = {}
    for relative in ("proptest-praxis/src", "proptest-praxis/owner", "proptest-praxis/stage8-policy/tests", "fuzz"):
        for source in sorted((HERE / relative).rglob("*.rs")):
            if "target" not in source.parts:
                consumers[source.relative_to(ROOT).as_posix()] = references(source)
    owners = {"peritus-product-runner": ["reads", "decision", "plans", "inputs"],
              "peritus-daemon": ["daemon"], "peritus-provider-openai": ["openai_http"]}
    rows = []
    for name, (directory, manifest) in sorted(packages.items()):
        lane, contract, oracle = lanes[name]
        rows.append({"package": name, "manifest": directory + "/Cargo.toml", "lane": lane,
                     "declared_features": manifest.get("features", {}),
                     "contract": contract, "oracle": oracle,
                     "source_associations": [path for path, refs in consumers.items() if name.replace("-", "_") in refs],
                     "owner_campaigns": owners.get(name, []),
                     "execution_status": "consult source-bound run receipts; association alone is not coverage",
                     "native_platform_limit": "Linux runs cannot exercise macOS/Windows native backends"})
    fuzz_manifest = HERE / "fuzz/Cargo.toml"
    fuzz_targets = [b["name"] for b in tomllib.loads(fuzz_manifest.read_text()).get("bin", [])] if fuzz_manifest.is_file() else []
    report = {"schema_version": 1, "workspace_packages": len(rows), "packages": rows,
              "fuzz_targets": fuzz_targets,
              "limits": ["Inventory is not line, branch, or behavioral coverage.",
                         "Mutation inventory includes all active packages; each run records its actual filters and shard.",
                         "A surviving mutation is a coverage or oracle investigation, not proof of a product defect.",
                         "Fuzz targets exercise named public mechanisms; providers, external processes, GUI workflows and OS schedules require real-interface campaigns.",
                         "ASan careful mode rebuilds Rust std; native dependencies and subprocesses are not automatically instrumented.",
                         "Finite searches cannot establish correctness for every input or schedule."]}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as stream:
        json.dump(report, stream, indent=2, sort_keys=True)
        stream.write("\n")
    print(json.dumps({"workspace_packages": len(rows), "fuzz_targets": len(fuzz_targets), "output": str(args.output)}))


if __name__ == "__main__":
    main()
