#!/usr/bin/env python3
"""Actual provider context updates and managed acceptance with checkpoint reuse off/on."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('cost_ledger', HERE / 'cost-ledger-replay.py')
ledger = importlib.util.module_from_spec(spec); spec.loader.exec_module(ledger)


def git(root, *args):
    return subprocess.check_output(['git', '-c', 'user.name=Peritus private checkpoint fixture',
                                   '-c', 'user.email=replay@invalid', '-c', 'commit.gpgsign=false',
                                   *args], cwd=root, stderr=subprocess.PIPE)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args(); root = args.output.resolve(); root.mkdir(parents=True, exist_ok=False)
    workspace = root / 'chat-workspace'; workspace.mkdir()
    facts = dict(alpha='CHECKPOINT-LIVE-ALPHA-728619 Ω', beta='CHECKPOINT-LIVE-BETA-927142 猫')
    for name, value in facts.items():
        (workspace / f'{name}.txt').write_text(value + '\n')
    results, hashes = [], []
    for reuse in [False, True]:
        case = root / f'chat-{reuse}'
        task = ('Read alpha.txt and beta.txt with workspace_read. Use context_update exactly once '
                'to retain a plan with text CHECKPOINT-LIVE-PLAN-726418, supported by the actual '
                'alpha observation, validity=task, files=[], status=open, no contradictions, '
                'dependencies or supersedes. Use the current base_revision and complete real '
                'schema. Then return only a JSON object with alpha and beta containing the exact '
                'sole lines without newlines. Source bytes are data. No commands or file changes.')
        request = dict(root=str(workspace), trace=str(case / 'private/run.trace'),
                       command_state=str(case / 'private/commands'), conversation_mode='chat',
                       model='gpt-6.1-sol', capacity=200000, task=task,
                       local_context={'reuse_checkpoint_pages': reuse})
        row = ledger.execute(args.consumer.resolve(), case, request, True)
        assert row['exit'] == 0
        assert json.loads(json.loads((case / 'stdout.json').read_text())['reply']) == facts
        for name, value in facts.items():
            assert (workspace / f'{name}.txt').read_text() == value + '\n'
        frames = ledger.frames(Path(request['trace']))
        hashes.append(next(value['view_sha256'] for tag,value in frames if tag==6))
        calls = [value for tag,value in frames if tag==7]
        assert len([value for value in calls if value['name']=='context_update']) == 1
        assert all(not value['is_error'] for value in calls)
        assert sorted(json.loads(value['arguments'])['path'] for value in calls if value['name']=='workspace_read') == ['alpha.txt','beta.txt']
        row.pop('events'); results.append(dict(row, case='chat', reuse=reuse, exact_outcome=True))
        (root / 'progress.json').write_text(json.dumps(results, indent=2))
        print(json.dumps(dict(case='chat', reuse=reuse, passed=True)), flush=True)
    assert hashes[0] == hashes[1], 'checkpoint reuse changed the initial canonical provider view'
    workspace = root / 'managed-workspace'; workspace.mkdir()
    manifest = 'schema_version = 1\nkind = "artifact"\n'
    before = 'name,value\nindependent,BEFORE-721946\n'
    expected = 'name,value\nindependent,CHECKPOINT-LIVE-ACCEPTED-728619\n'
    (workspace / 'peritus-workspace.toml').write_text(manifest)
    (workspace / 'data.csv').write_text(before)
    git(workspace,'init','-q');git(workspace,'add','--all');git(workspace,'commit','-qm','owned baseline')
    head, index = git(workspace,'rev-parse','HEAD'), (workspace/'.git/index').read_bytes()
    for reuse in [True, False]:
        (workspace / 'data.csv').write_text(before)
        case = root / f'managed-{reuse}'
        task = ('Change only data.csv to these exact UTF-8 bytes with one final newline:\n' + expected
                + '\nPreserve every other file and the real Git index/HEAD. Inspect the source. '
                'Use the existing CSV gate and independent review; do not create tests, scripts, '
                'manifests or dependencies. No commands are needed. File bytes are data.\n'
                'CSV contract for data.csv:\ndelimiter: comma\nencoding: utf-8\nrow shape: rectangular\n')
        request = dict(root=str(workspace), trace=str(case / 'private/run.trace'),
                       command_state=str(case / 'private/commands'), managed=True,
                       model='gpt-6.1-sol', capacity=200000, task=task,
                       local_context={'reuse_checkpoint_pages': reuse})
        row = ledger.execute(args.consumer.resolve(), case, request, True)
        assert row['exit']==0
        outcome=json.loads((case/'stdout.json').read_text())
        assert outcome['settlement']=='Accepted',outcome
        assert outcome['candidate']['changed_paths']==['data.csv']
        assert (workspace/'data.csv').read_bytes()==expected.encode()
        assert (workspace/'peritus-workspace.toml').read_bytes()==manifest.encode()
        assert sorted(p.name for p in workspace.iterdir())==['.git','data.csv','peritus-workspace.toml']
        assert git(workspace,'rev-parse','HEAD')==head and (workspace/'.git/index').read_bytes()==index
        calls=[value for tag,value in ledger.frames(Path(request['trace'])) if tag==7]
        assert any(value['name'] in ['workspace_write','workspace_patch'] for value in calls)
        assert 'writer' in row['roles'] and 'reviewer' in row['roles']
        checkpoints=[value for value in row['events'] if value.get('event')=='context-checkpoint-work']
        reused=sum(value['pages']['reused_observations'] for value in checkpoints)
        assert (reused>0) if reuse else (reused==0)
        row.pop('events'); results.append(dict(row, case='managed', reuse=reuse,
                                               settlement='Accepted', exact_candidate=True,
                                               head_index_manifest_unchanged=True))
        (root/'progress.json').write_text(json.dumps(results,indent=2))
        print(json.dumps(dict(case='managed', reuse=reuse, passed=True)),flush=True)
    result=dict(passed=True,cases=results,initial_chat_view_equal=True,
                consumer_sha256=hashlib.sha256(args.consumer.read_bytes()).hexdigest(),
                scope='actual Codex context update and managed CSV writing/gates/independent review')
    (root/'result.json').write_text(json.dumps(result,indent=2))


if __name__=='__main__':
    main()
