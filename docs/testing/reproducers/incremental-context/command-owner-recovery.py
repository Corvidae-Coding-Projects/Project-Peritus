#!/usr/bin/env python3
"""Kill real C4/C2 consumers at running/terminal boundaries and reconcile original owners."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import time

HERE=Path(__file__).resolve().parent; REPO=HERE.parents[3]


class Client:
    def __init__(self,binary,spec,case):
        self.err=(case/'client.stderr').open('a');self.log=(case/'interface.jsonl').open('a')
        self.child=subprocess.Popen([str(binary),str(spec)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.err,text=True)
    def call(self,value):
        self.child.stdin.write(json.dumps(value)+'\n');self.child.stdin.flush()
        assert select.select([self.child.stdout],[],[],30)[0],value
        line=self.child.stdout.readline();assert line,(value,self.child.poll())
        result=json.loads(line);self.log.write(json.dumps(dict(request=value,response=result))+'\n');self.log.flush();return result
    def close(self,kill=False):
        if self.child.poll() is None:
            if kill:self.child.kill()
            else:self.child.stdin.close()
            self.child.wait(timeout=15)
        self.err.close();self.log.close()


def until(predicate,seconds=20):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        result=predicate()
        if result:return result
        time.sleep(.1)
    raise TimeoutError('owned command did not reach the expected boundary')


def main():
    p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    root=a.output.resolve();root.mkdir(parents=True);binary=a.consumer.resolve();results=[]
    for label in ['running-consumer-death','before-terminal-projection','after-terminal-projection','projection-write-failure','native-process-death','foreign-owner']:
        case=root/label;workspace=case/'workspace';workspace.mkdir(parents=True)
        worker=("from pathlib import Path\nimport os,time,sys\n"
                "Path('pid.txt').write_text(str(os.getpid()))\n"
                "with Path('starts.txt').open('a') as f:f.write('START-719426\\n')\n"
                "print('BEFORE-841296 Ω',flush=True)\nprint('ERR-615924',file=sys.stderr,flush=True)\n"
                'end=time.monotonic()+60\n'
                "while not Path('release').exists() and time.monotonic()<end:time.sleep(.03)\n"
                "Path('finished.txt').write_text('EXACT-TERMINAL-827451')\n"
                "print('EXACT-TERMINAL-827451',flush=True)\n")
        (workspace/'worker.py').write_text(worker)
        spec=dict(root=str(workspace),trace=str(case/'private/run.trace'),command_state=str(case/'private/commands'),
                  owner_file=str(case/'owner.json'),capacity=200000)
        path=case/'spec.json';path.write_text(json.dumps(spec));client=None;debugger=None
        try:
            if label in ['before-terminal-projection','after-terminal-projection']:
                witness=case/'killed.json';terminal=REPO/'crates/app/peritus-product-runner/src/developer_tools/command_runtime/result.rs'
                projection=REPO/'crates/app/peritus-product-runner/src/developer_tools/command_runtime/projections.rs'
                first=next(i for i,t in enumerate(terminal.read_text().splitlines(),1) if 'let store = ArtifactStore::open' in t)
                second=next(i for i,t in enumerate(projection.read_text().splitlines(),1) if 'directory.sync_all()' in t)
                script='set pagination off\nset confirm off\nset detach-on-fork on\n'+f'break {terminal}:{first}\nrun\n'
                if label=='after-terminal-projection':
                    script+=f'disable 1\ntbreak {projection}:{second}\ncontinue\n'
                script+='python\nimport gdb,json\n'+f'open({str(witness)!r},"w").write(json.dumps(dict(pid=gdb.selected_inferior().pid,stack=gdb.execute("bt 8",to_string=True))))\nend\nkill\n'
                gate=case/'gate.gdb';gate.write_text(script)
                out=(case/'gdb.stdout').open('w');err=(case/'gdb.stderr').open('w')
                debugger=subprocess.Popen(['gdb','-nx','-batch','-x',str(gate),'--args',str(binary),str(path)],stdin=subprocess.PIPE,stdout=out,stderr=err,text=True,cwd=REPO)
                debugger.stdin.write('{"action":"start"}\n');debugger.stdin.flush()
                until(lambda:(workspace/'starts.txt').exists());(workspace/'release').touch()
                while not witness.exists():
                    assert debugger.poll() is None,(case/'gdb.stderr').read_text()
                    debugger.stdin.write('{"action":"poll"}\n');debugger.stdin.flush();time.sleep(.1)
                debugger.wait(timeout=15);out.close();err.close()
                assert (workspace/'finished.txt').read_text()=='EXACT-TERMINAL-827451'
            else:
                client=Client(binary,path,case);started=client.call(dict(action='start'));assert started['ok'],started
                until(lambda:(workspace/'starts.txt').exists())
                if label=='running-consumer-death':
                    client.close(kill=True);client=None
                elif label=='projection-write-failure':
                    (case/'private/commands/router/command-handles-v1.json.pending').mkdir()
                elif label=='native-process-death':
                    pid=int((workspace/'pid.txt').read_text());assert Path(f'/proc/{pid}/cwd').resolve()==workspace
                    os.kill(pid,signal.SIGKILL)
                elif label=='foreign-owner':
                    original=json.loads((case/'owner.json').read_text());client.close(kill=True);client=None
                    rejections=[]
                    for field in ['source_run_id','run_id','action_id','process_id']:
                        wrong=dict(original);wrong[field]='ab'*16;(case/'owner.json').write_text(json.dumps(wrong))
                        fresh=Client(binary,path,case);value=fresh.call(dict(action='recover'));fresh.close()
                        assert not value['ok'] or value['value']['state']!='Succeeded',value
                        rejections.append(dict(field=field,result=value))
                    (case/'owner.json').write_text(json.dumps(original));(case/'foreign.json').write_text(json.dumps(rejections,indent=2))
            if client is None:client=Client(binary,path,case)
            first=client.call(dict(action='recover'));assert first['ok'],first
            if label=='running-consumer-death':assert first['value']['state']!='Succeeded',first
            (workspace/'release').touch()
            def finished():
                value=client.call(dict(action='poll'));assert value['ok'],value
                return value if value['value']['state']!='Running' else None
            terminal=until(finished)
            if label in ['before-terminal-projection','after-terminal-projection','projection-write-failure']:
                assert terminal['value']['state']=='Succeeded' and terminal['value']['exit_code']==0,terminal
                assert 'EXACT-TERMINAL-827451' in terminal['value']['stdout'],terminal
            if label=='native-process-death':assert terminal['value']['state']!='Succeeded',terminal
            if terminal['value']['state']=='Succeeded':
                assert (workspace/'finished.txt').read_text()=='EXACT-TERMINAL-827451'
                assert 'EXACT-TERMINAL-827451' in terminal['value']['stdout']
            assert (workspace/'starts.txt').read_text()=='START-719426\n'
            assert (workspace/'worker.py').read_text()==worker
            client.close();client=None
            fresh=Client(binary,path,case);again=fresh.call(dict(action='recover'));fresh.close()
            assert again['ok'] and again['value']['state']==terminal['value']['state'],(terminal,again)
            result=dict(case=label,passed=True,first=first,terminal=terminal,fresh=again,one_start=True,no_provider_inference=True)
            results.append(result);(root/'results.json').write_text(json.dumps(results,indent=2));print(json.dumps(dict(case=label,passed=True,state=terminal['value']['state'])),flush=True)
        finally:
            (workspace/'release').touch()
            if client is not None:client.close(kill=True)
            if debugger is not None and debugger.poll() is None:debugger.kill();debugger.wait(timeout=10)
    (root/'identity.json').write_text(json.dumps(dict(consumer_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()),indent=2))


if __name__=='__main__':main()
