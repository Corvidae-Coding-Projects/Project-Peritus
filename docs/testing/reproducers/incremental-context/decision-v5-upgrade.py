#!/usr/bin/env python3
"""Read actual v5 prototype state, migrate under v6, retain resolved progress and reject old cursors/readers."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

HERE=Path(__file__).resolve().parent
s=importlib.util.spec_from_file_location('host',HERE/'decision-host-dependencies.py')
h=importlib.util.module_from_spec(s);s.loader.exec_module(h)


def main():
    p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True)
    p.add_argument('--v5-consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    a=p.parse_args();root=a.output.resolve();workspace=root/'workspace';workspace.mkdir(parents=True)
    (workspace/'alpha.txt').write_text('V5-COMPATIBILITY-EXACT-918372 Ω\n')
    git=lambda *args:subprocess.check_output(['git','-c','user.name=Owned fixture','-c',
        'user.email=replay@invalid','-c','commit.gpgsign=false',*args],cwd=workspace,stderr=subprocess.PIPE)
    git('init','-q');git('add','--all');git('commit','-qm','owned source')
    spec=dict(root=str(workspace),trace=str(root/'run.trace'),command_state=str(root/'commands'),managed=True,
        capacity=200000,include_memory_tools=True,task='Read alpha.txt and retain exact completed progress.',
        local_context=dict(decision_view='conservative'))
    path=root/'input.json';path.write_text(json.dumps(spec));old=h.Client(a.v5_consumer.resolve(),path,root)
    old.send(dict(action='observe',calls=[['workspace_read',dict(path='alpha.txt')]]));view=old.assemble()
    handle=next(json.loads(b['output'])['local_context']['handle'] for m in view['messages']
                for b in m['blocks'] if b.get('type')=='result')
    old.send(dict(action='bind'));marker='V5-RESOLVED-PLAN-DO-NOT-RESTART-183729 ' + 'x'*10000
    assert not old.update(view,'v5-resolved-plan',handle,text=marker,status='resolved').get('rejected')
    old.assemble();assert h.pages.page.checkpoint(root)[1]['schema_version']==5
    page=old.send(dict(action='plan',tool='context_read',args=dict(observation_ids=[],query=None,cursor=None,
        offset=0,max_bytes=2048)))['outputs'][0]
    cursor=page['cursor'];assert cursor
    old.close()
    current=h.Client(a.consumer.resolve(),path,root)
    assert current.entries()[marker]['status']=='resolved', 'current decoder lost readable v5 state'
    view=current.assemble();assert h.pages.page.checkpoint(root)[1]['schema_version']==6
    entry=current.entries()[marker];assert entry['status']=='stale' and entry['declared_status']=='resolved',entry
    stale=current.send(dict(action='plan',tool='context_read',args=dict(observation_ids=[],query=None,cursor=cursor,
        offset=0,max_bytes=2048)))['outputs'][0]
    assert stale.get('rejected'),stale
    current.close();h.verified_published_roots(root)
    refused=subprocess.run([str(a.v5_consumer.resolve()),str(path)],input='{"action":"assemble"}\n',
                           text=True,capture_output=True,timeout=90)
    (root/'v5-refusal.stdout').write_text(refused.stdout);(root/'v5-refusal.stderr').write_text(refused.stderr)
    assert refused.returncode!=0 and 'checkpoint' in refused.stderr
    spec['local_context']['decision_view']='history';path.write_text(json.dumps(spec))
    current=h.Client(a.consumer.resolve(),path,root);current.assemble()
    assert current.entries()[marker]['declared_status']=='resolved';current.close()
    assert h.pages.page.checkpoint(root)[1]['schema_version']==6
    result=dict(passed=True,v5_readable=True,v6_committed_before_new_semantics=True,
                resolved_progress_retained=True,old_cursor_rejected=True,old_reader_refuses=True,history_rollback=True)
    (root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))


if __name__=='__main__':main()
