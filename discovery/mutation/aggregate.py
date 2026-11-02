#!/usr/bin/env python3
"""Prove exact candidate and shard accounting for a complete mutation sweep."""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re


def read_json(path):
    return json.loads(path.read_text())


def aggregate(preflight_dir, shard_dir, expected_shards):
    inventory_dir = preflight_dir / "inventory"
    expected = [item["name"] for item in read_json(inventory_dir / "mutants.json")]
    inventory_report = read_json(inventory_dir / "inventory.json")
    expected_head = inventory_report["source_head"]
    reports = sorted(shard_dir.glob("*/report.json"))
    problems = []
    totals = Counter()
    statuses = Counter()
    observed = []
    indices = []
    source_manifests = []

    for report_path in reports:
        folder = report_path.parent
        try:
            report = read_json(report_path)
            request = read_json(folder / "request.json")
            mutants = read_json(folder / "mutants.json")
            names = [item["name"] for item in mutants]
            if any(not isinstance(name, str) or not name for name in names):
                raise ValueError("mutant names must be nonempty strings")
        except (OSError, ValueError, KeyError, TypeError) as error:
            problems.append(f"invalid shard receipt {folder.name}: {error}")
            continue
        match = re.fullmatch(r"(\d+)/(\d+)", str(request.get("shard", "")))
        if match is None:
            problems.append(f"invalid shard selector in {folder.name}")
        else:
            index, denominator = map(int, match.groups())
            indices.append(index)
            if denominator != expected_shards:
                problems.append(f"wrong shard denominator in {folder.name}: {denominator}")
            if not 0 <= index < expected_shards:
                problems.append(f"out-of-range shard index in {folder.name}: {index}")

        observed.extend(names)
        status = report.get("status", "missing")
        statuses[status] += 1
        counts = report.get("counts", {})
        if not isinstance(counts, dict) or any(
            type(value) is not int or value < 0 for value in counts.values()
        ):
            problems.append(f"invalid outcome counts in {folder.name}")
            counts = {}
        totals.update(counts)
        if report.get("source_head") != expected_head:
            problems.append(f"source revision drift in {folder.name}")
        source_manifest = report.get("source_manifest_sha256")
        if not isinstance(source_manifest, str) or re.fullmatch(r"[0-9a-f]{64}", source_manifest) is None:
            problems.append(f"invalid source manifest digest in {folder.name}")
        else:
            source_manifests.append(source_manifest)
        if report.get("mutation_candidates") != len(names):
            problems.append(f"candidate count mismatch in {folder.name}")
        if sum(counts.values()) != len(names):
            problems.append(f"outcome count mismatch in {folder.name}")
        if report.get("unexecuted_candidates") != 0:
            problems.append(f"unexecuted candidates in {folder.name}")
        if report.get("completeness_problems"):
            problems.append(f"controller completeness failure in {folder.name}")
        if status not in {"pass", "fail"}:
            problems.append(f"incomplete shard status in {folder.name}: {status}")

    if len(reports) != expected_shards:
        problems.append(f"expected {expected_shards} shard reports, found {len(reports)}")
    index_counts = Counter(indices)
    duplicate_indices = sorted(index for index, count in index_counts.items() if count != 1)
    missing_indices = sorted(set(range(expected_shards)) - set(indices))
    if duplicate_indices:
        problems.append(f"{len(duplicate_indices)} duplicated shard indices")
    if missing_indices:
        problems.append(f"{len(missing_indices)} missing shard indices")
    if len(set(source_manifests)) > 1:
        problems.append("shards used different source manifests")

    counts = Counter(observed)
    duplicates = sorted(name for name, count in counts.items() if count != 1)
    missing = sorted(set(expected) - set(observed))
    unexpected = sorted(set(observed) - set(expected))
    if len(expected) != len(set(expected)):
        problems.append("canonical inventory contains duplicate mutant names")
    if duplicates:
        problems.append(f"{len(duplicates)} duplicated shard candidates")
    if missing:
        problems.append(f"{len(missing)} canonical candidates were not selected")
    if unexpected:
        problems.append(f"{len(unexpected)} unexpected candidates were selected")

    findings = totals.get("MissedMutant", 0) + totals.get("Timeout", 0)
    if problems:
        status = "incomplete"
    elif findings or statuses.get("fail", 0):
        status = "fail"
    else:
        status = "pass"
    return {
        "status": status,
        "source_head": expected_head,
        "source_manifest_sha256": source_manifests[0]
        if source_manifests and len(set(source_manifests)) == 1
        else None,
        "expected_shards": expected_shards,
        "observed_shards": len(reports),
        "canonical_candidates": len(expected),
        "observed_candidates": len(observed),
        "outcomes": dict(sorted(totals.items())),
        "shard_statuses": dict(sorted(statuses.items())),
        "problems": problems,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--preflight", type=Path, required=True)
    parser.add_argument("--shards", type=Path, required=True)
    parser.add_argument("--expected-shards", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.expected_shards < 1:
        parser.error("--expected-shards must be positive")
    summary = aggregate(args.preflight, args.shards, args.expected_shards)
    args.output.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    return 0 if summary["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
