#!/usr/bin/env python3
"""Exercise actual config APIs: omitted/partial settings activate both defaults; opt-outs win."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    base = 'version=1\nstore_id="' + '81' * 16 + '"\n[paths]\n'
    state = root / 'state'
    base += 'state_root=' + json.dumps(str(state)) + '\n'
    for name in ['artifact', 'evidence', 'workspace', 'process', 'transaction', 'backup']:
        base += name + '_root=' + json.dumps(str(state / (name + 's'))) + '\n'
    base += '[approval_registry]\npayload_file=' + json.dumps(str(root / 'registry.bin')) + '\ngeneration=1\n'
    base += '[human]\nactor_id="' + '31' * 16 + '"\n[telemetry]\nmode="disabled"\n'
    base += '[tools]\nallow=["fs.read","fs.discover","fs.search"]\n'
    cases = [('omitted', {}, None), ('empty-local', {}, ''),
             ('partial', {'max_read_bytes': 8192}, 'max_read_bytes=8192\n'),
             ('history-only', {'decision_view': 'history'}, 'decision_view="history"\n'),
             ('plans-off-only', {'read_only_plans': 'disabled'}, 'read_only_plans="disabled"\n'),
             ('both-opt-outs', {'decision_view': 'history', 'read_only_plans': 'disabled'},
              'decision_view="history"\nread_only_plans="disabled"\n'),
             ('memory-off', {'enabled': False}, 'enabled=false\n')]
    results = []
    for name, local, toml in cases:
        case = root / name
        case.mkdir()
        daemon = case / 'daemon.toml'
        daemon.write_text(base + ('' if toml is None else '[context.local]\n' + toml))
        spec = case / 'spec.json'
        spec.write_text(json.dumps(dict(local=local, daemon_config=str(daemon))))
        process = subprocess.run([str(args.consumer.resolve()), str(spec)], capture_output=True, text=True)
        (case / 'stdout.json').write_text(process.stdout)
        (case / 'stderr.log').write_text(process.stderr)
        assert process.returncode == 0, (name, process.stderr)
        observed = json.loads(process.stdout)
        assert observed['builtin']['decision_view'] == observed['enum_defaults']['decision_view'] == 'conservative'
        assert observed['builtin']['read_only_plans'] == observed['enum_defaults']['read_only_plans'] == 'sequential'
        for key in ['parsed_local', 'daemon_local']:
            assert observed[key]['decision_view'] == local.get('decision_view', 'conservative'), (name, key)
            assert observed[key]['read_only_plans'] == local.get('read_only_plans', 'sequential'), (name, key)
            assert observed[key]['enabled'] == local.get('enabled', True), (name, key)
            assert observed[key]['max_read_bytes'] == local.get('max_read_bytes', 16384), (name, key)
            assert observed[key]['candidate_observation'] == 'separate', (name, key)
        results.append(dict(case=name, passed=True, local=observed['daemon_local']))
    (root / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(json.dumps(dict(passed=True, configuration_cases=len(results))))


if __name__ == '__main__':
    main()
