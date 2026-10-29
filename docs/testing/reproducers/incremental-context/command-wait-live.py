#!/usr/bin/env python3
"""Real native interactive process: active output wait, exact input, active terminal wait."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import time

HERE = Path(__file__).resolve().parent
s = importlib.util.spec_from_file_location('plan', HERE / 'plan-live-compare.py')
plan = importlib.util.module_from_spec(s); s.loader.exec_module(plan)


def main():
    p = argparse.ArgumentParser(); p.add_argument('--consumer', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True); a = p.parse_args()
    root = a.output.resolve(); workspace = root / 'workspace'; workspace.mkdir(parents=True)
    worker = ("from pathlib import Path\nimport sys,time\n"
              "with Path('starts.txt').open('a') as f:f.write('START-812795\\n')\n"
              "time.sleep(12)\nprint('READY-826451 Ω',flush=True)\n"
              "answer=input()\nPath('received.txt').write_text(answer,encoding='utf-8')\n"
              "end=time.monotonic()+90\n"
              "while not Path('release').exists() and time.monotonic()<end:time.sleep(.05)\n"
              "print('COMPLETE-739152 '+answer,flush=True)\n")
    (workspace / 'worker.py').write_text(worker)
    task = ('Operate the existing worker.py exactly once. Execution is authorized. Inspect it, '
            'then command_start program=/usr/bin/python3 args=["worker.py"] interactive=true '
            'purpose=verification timeout_seconds=120. Only this existing command may run. '
            'Use command_wait until=output_or_terminal stream=terminal after_bytes=0 with no '
            'max_wait_ms. When READY-826451 appears, command_stdin the exact text '
            'NATIVE-INPUT-937154 Ω followed by carriage return (\\r). Then command_wait '
            'until=terminal with no max_wait_ms. Never use command_poll, run_command, or restart '
            'the process. Read starts.txt and received.txt to verify one start and exact input. '
            'Do not modify source, manifests, dependencies or create other files. No additional '
            'builds, lint, tests or external access. Return only the observed terminal outcome.')
    spec = dict(root=str(workspace),trace=str(root/'private/run.trace'),command_state=str(root/'private/commands'),
                model='gpt-6.1-sol',capacity=200000,task=task,local_context=dict(read_only_plans='sequential'))
    path=root/'spec.json';path.write_text(json.dumps(spec));binary=a.consumer.resolve()
    source=HERE.parents[3]/'crates/app/peritus-product-runner/src/developer_tools/executor/wait.rs'
    line=next(i for i,text in enumerate(source.read_text().splitlines(),1) if 'let handle = required_string' in text)
    marker=root/'terminal-wait-entered.json';resume=root/'resume-debugger';gate=root/'gate.gdb'
    gate.write_text('set pagination off\nset confirm off\nset non-stop on\nset detach-on-fork on\n'
        + f'break {source}:{line}\nignore 1 1\nrun\npython\nimport gdb,json,time,os\n'
        + f'open({str(marker)!r},"w").write(json.dumps(dict(pid=gdb.selected_inferior().pid,stack=gdb.execute("bt 8",to_string=True))))\n'
        + f'end=time.monotonic()+60\nwhile not os.path.exists({str(resume)!r}) and time.monotonic()<end:time.sleep(.05)\n'
        + f'assert os.path.exists({str(resume)!r})\nend\ncontinue -a\n')
    with (root/'program.stdout').open('w') as out,(root/'stderr.log').open('w') as err:
        child=subprocess.Popen(['gdb','-nx','-batch','-x',str(gate),'--args',str(binary),'live',str(path)],stdout=out,stderr=err,cwd=HERE.parents[3])
        try:
            end=time.monotonic()+240
            while not marker.exists() and child.poll() is None and time.monotonic()<end:time.sleep(.1)
            assert marker.exists(),'actual terminal wait was not reached'
            assert (workspace/'received.txt').read_text()=='NATIVE-INPUT-937154 Ω'
            resume.touch()
            # Let the production wait run normally before independently releasing its process.
            time.sleep(2);(workspace/'release').touch()
            assert child.wait(timeout=180)==0
        finally:
            resume.touch();(workspace/'release').touch()
            if child.poll() is None:child.terminate();child.wait(timeout=15)
    observations=plan.observations(root/'private/run.trace')
    (root/'observations.json').write_text(json.dumps(observations,indent=2))
    starts=[r for r in observations if r['name']=='command_start'];waits=[r for r in observations if r['name']=='command_wait']
    assert len(starts)==1 and len(waits)==2,[(r['name'],r['is_error']) for r in observations]
    assert not any(r['name'] in ['command_poll','run_command'] or r['is_error'] for r in observations)
    handle=json.loads(starts[0]['output'])['handle'];outputs=[json.loads(row['output']) for row in waits]
    assert all(value['handle']==handle for value in outputs)
    assert outputs[0]['state']=='running' and outputs[0]['wait']['reason']=='output',outputs[0]
    assert 'READY-826451 Ω' in outputs[0]['output']['data'],outputs[0]
    assert outputs[0]['wait']['elapsed_ms']>=500,outputs[0]
    assert outputs[1]['state']=='completed' and outputs[1]['success'] and outputs[1]['exit_code']==0,outputs[1]
    assert outputs[1]['wait']['elapsed_ms']>=500,outputs[1]
    assert (workspace/'starts.txt').read_text()=='START-812795\n'
    assert (workspace/'received.txt').read_text()=='NATIVE-INPUT-937154 Ω'
    assert (workspace/'worker.py').read_text()==worker
    audited=subprocess.run([str(binary),'audit-usage',str(path)],capture_output=True,text=True,check=True)
    (root/'independent-trace-audit.json').write_text(audited.stdout)
    result=dict(passed=True,one_native_start=True,exact_interactive_input=True,no_model_polling=True,
                waits=outputs,consumer_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                qualification='actual wait/input interfaces; no qualified product acceptance claim')
    (root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))


if __name__=='__main__':main()
