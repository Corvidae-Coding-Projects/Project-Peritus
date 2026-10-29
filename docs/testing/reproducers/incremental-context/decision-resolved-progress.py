#!/usr/bin/env python3
"""A completed plan must not become unfinished work when its host freshness becomes stale."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
s = importlib.util.spec_from_file_location('host', HERE / 'decision-host-dependencies.py')
h = importlib.util.module_from_spec(s); s.loader.exec_module(h)


def main():
    p = argparse.ArgumentParser(); p.add_argument('--consumer', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True); p.add_argument('--expect-bug', action='store_true')
    a = p.parse_args(); root = a.output.resolve(); root.mkdir(parents=True)
    results = []
    for kind in ['direct', 'managed-changed', 'legacy']:
        case = root / kind; source = case / 'workspace'; source.mkdir(parents=True)
        (source/'alpha.txt').write_text('RESOLVED-EXACT-ALPHA-918327 Ω\n')
        if kind != 'direct':
            git = lambda *args: subprocess.check_output(['git','-c','user.name=Owned fixture','-c',
                'user.email=replay@invalid','-c','commit.gpgsign=false',*args],cwd=source,stderr=subprocess.PIPE)
            git('init','-q');git('add','--all');git('commit','-qm','owned source')
        spec = dict(root=str(source),trace=str(case/'run.trace'),command_state=str(case/'commands'),
            managed=kind!='direct',capacity=200000,include_memory_tools=True,
            task='Read alpha.txt; retain exact completed task progress.',
            local_context=dict(decision_view='history' if kind=='legacy' else 'conservative'))
        path=case/'input.json';path.write_text(json.dumps(spec));c=h.Client(a.consumer.resolve(),path,case)
        c.send(dict(action='observe',calls=[['workspace_read',dict(path='alpha.txt')]]));view=c.assemble()
        handle=next(json.loads(b['output'])['local_context']['handle'] for m in view['messages']
                    for b in m['blocks'] if b.get('type')=='result')
        c.send(dict(action='bind'));marker='COMPLETED-PLAN-DO-NOT-RESTART-851729'
        out=c.update(view,marker,handle,status='resolved');assert not out.get('rejected'),out
        if kind=='managed-changed':(source/'alpha.txt').write_text('RESOLVED-EXACT-ALPHA-619284 Ω\n')
        if kind=='legacy':
            c.assemble();c.close();spec['local_context']['decision_view']='conservative';path.write_text(json.dumps(spec))
            c=h.Client(a.consumer.resolve(),path,case)
        view=c.assemble();entry=c.entries()[marker]
        active=[json.loads(line) for m in view['messages'] for b in m['blocks']
                if b.get('text','').startswith('LOCAL WORKING STATE') for line in b['text'].splitlines() if line.startswith('{')]
        promoted=any(e.get('text')==marker for e in active)
        reproduced=entry.get('declared_status')!='resolved' or promoted
        assert entry['status']=='stale'
        results.append(dict(case=kind,effective_status=entry['status'],declared_status=entry.get('declared_status'),
                            presented_as_unfinished=promoted,bug_reproduced=reproduced))
        assert reproduced if a.expect_bug else not reproduced,results[-1]
        c.close()
        if not a.expect_bug:
            c=h.Client(a.consumer.resolve(),path,case);assert c.entries()[marker]['declared_status']=='resolved';c.assemble();c.close()
            h.verified_published_roots(case)
    result=dict(passed=not a.expect_bug,expected_bug=a.expect_bug,cases=results)
    (root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))


if __name__=='__main__':main()
