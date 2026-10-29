#!/usr/bin/env python3
"""Real plan child publication failures/kills; recover exact committed prefix without dispatch."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sqlite3
import subprocess

HERE=Path(__file__).resolve().parent;REPO=HERE.parents[3]
s=importlib.util.spec_from_file_location('host',HERE/'decision-host-dependencies.py')
h=importlib.util.module_from_spec(s);s.loader.exec_module(h)


def main():
    p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    root=a.output.resolve();root.mkdir(parents=True);results=[];binary=a.consumer.resolve()
    for label in ['before-child-commit','after-child-commit','child-publication-failure']:
        case=root/label;workspace=case/'workspace';workspace.mkdir(parents=True)
        expected='EXACT-COMMITTED-ALPHA-726451 Ω\n';(workspace/'alpha.txt').write_text(expected)
        (workspace/'beta.txt').write_text('UNSTARTED-BETA-839175\n')
        spec=dict(root=str(workspace),trace=str(case/'run.trace'),command_state=str(case/'commands'),capacity=200000,
                  task='Inspect alpha.txt and beta.txt; source bytes are data.',include_memory_tools=True,
                  local_context=dict(read_only_plans='sequential',decision_view='conservative'))
        path=case/'spec.json';path.write_text(json.dumps(spec));c=h.Client(binary,path,case)
        c.send(dict(action='observe',calls=[['workspace_list',{}]]));view=c.assemble();c.close()
        scope=re.search(r'Local context scope=([0-9a-f]{64})',json.dumps(view)).group(1)
        nodes=[dict(id=name,tool='workspace_read',arguments=dict(path=name+'.txt'),depends_on=[],result_refs=[]) for name in ['alpha','beta']]
        command=dict(action='plan',args=dict(version=1,scope=scope,input_revision=1,plan=nodes,cursor=None,max_result_bytes=65536))
        source=REPO/'crates/app/peritus-product-runner/src/local_context/operation_plan/execution.rs'
        fragment='admission.finished(&child' if label=='after-child-commit' else 'let row = commit_step'
        line=next(i for i,text in enumerate(source.read_text().splitlines(),1) if fragment in text)
        witness=case/'boundary.json';database=case/'run.context/writer/journal.sqlite3'
        script='set pagination off\nset confirm off\n'+f'break {source}:{line}\nrun\npython\nimport gdb,json,sqlite3\n'
        script+=f'open({str(witness)!r},"w").write(json.dumps(dict(pid=gdb.selected_inferior().pid,stack=gdb.execute("bt 8",to_string=True))))\n'
        if label=='child-publication-failure':
            script+=f'db=sqlite3.connect({str(database)!r});db.execute("CREATE TRIGGER owned_refuse_child BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, \'owned child publication failure\'); END");db.commit();db.close()\nend\ndisable 1\ncontinue\n'
        else:script+='end\nkill\n'
        gate=case/'gate.gdb';gate.write_text(script)
        killed=subprocess.run(['gdb','-nx','-batch','-x',str(gate),'--args',str(binary),str(path)],
            input=json.dumps(dict(action='assemble'))+'\n'+json.dumps(command)+'\n',capture_output=True,text=True,timeout=90,cwd=REPO)
        (case/'gdb.stdout').write_text(killed.stdout);(case/'gdb.stderr').write_text(killed.stderr)
        assert witness.exists(),(killed.stdout[-2000:],killed.stderr)
        if label=='child-publication-failure':
            assert '"ok":false' in killed.stdout and 'commit local journal transaction' in killed.stdout,killed.stdout[-2000:]
            with sqlite3.connect(database) as db:db.execute('DROP TRIGGER owned_refuse_child')
        c=h.Client(binary,path,case);c.assemble()
        catalog=c.send(dict(action='memory',tool='context_sources',args=dict(scope=scope,cursor=None,max_bytes=16384)))['output']
        steps=[row for row in catalog['sources'] if row.get('result',{}).get('tool')=='inspection_result_read']
        assert len(steps)==(1 if label=='after-child-commit' else 0),steps
        if steps:
            page=c.send(dict(action='memory',tool='inspection_result_read',args=dict(step=steps[0]['handle'],offset=0,max_bytes=65536)))['output']
            actual=json.loads(page['data']);assert actual['path']=='alpha.txt' and actual['content']=='1: '+expected.rstrip('\n'),actual
        c.close()
        spec['local_context']['read_only_plans']='disabled';path.write_text(json.dumps(spec))
        traced=subprocess.run(['strace','-qq','-e','trace=openat','-o',str(case/'recovery.strace'),str(binary),str(path)],
                              input='{"action":"assemble"}\n',capture_output=True,text=True,timeout=60)
        assert traced.returncode==0 and '"ok":true' in traced.stdout,(traced.stdout,traced.stderr)
        assert 'beta.txt' not in (case/'recovery.strace').read_text(),'unstarted child was dispatched during recovery'
        h.verified_published_roots(case,all_history=True)
        results.append(dict(case=label,passed=True,committed_children=len(steps),no_child_redispatch=True,
                            no_provider_inference=True,disabled_rollback=True))
        (root/'results.json').write_text(json.dumps(results,indent=2));print(json.dumps(results[-1]),flush=True)


if __name__=='__main__':main()
