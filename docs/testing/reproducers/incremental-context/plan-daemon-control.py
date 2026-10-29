#!/usr/bin/env python3
"""Control actual provider-proposed plans/waits through the production daemon's IPC."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]


def endpoint(state):
    digest = hashlib.sha256(b'peritus/daemon-endpoint/v1\0' + bytes([0x81]) * 16).hexdigest()[:32]
    original = str(state / ('peritus-' + digest + '.sock'))
    if len(original.encode()) <= 107:
        return original
    short = hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0' + original.encode()).hexdigest()[:32]
    return '/tmp/peritus-' + short + '/daemon.sock'


def host_records(state):
    found = {}
    prefix = b'PERITUS HOST INSPECTION RECORD V1\n'
    for path in state.rglob('*'):
        if not path.is_file() or path.stat().st_size > 2_000_000 or 'artifacts' not in path.parts:
            continue
        raw = path.read_bytes(); at = raw.find(prefix)
        if at >= 0:
            value, _ = json.JSONDecoder().raw_decode(raw[at + len(prefix):].decode('utf-8', errors='ignore'))
            found[json.dumps(value, sort_keys=True)] = value
    return list(found.values())


def tool_observations(state):
    rows = []
    for path in (state / 'product-runs').glob('*.trace'):
        raw = path.read_bytes(); offset = 0
        while offset + 9 <= len(raw):
            tag = raw[offset]; size = int.from_bytes(raw[offset + 1:offset + 9], 'little')
            if offset + 9 + size > len(raw):
                break
            if tag == 7:
                rows.append(json.loads(raw[offset + 9:offset + 9 + size]))
            offset += 9 + size
    return rows


def wait_for(predicate, timeout=180):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(.1)
    raise TimeoutError('owned daemon workflow did not reach its expected boundary')


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--daemon', type=Path, required=True)
    p.add_argument('--cli', type=Path, required=True)
    p.add_argument('--client', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--expect-before-edit-bug', action='store_true')
    p.add_argument('--cases', nargs='+', default=['plan-steer', 'plan-deny-read', 'plan-pause', 'plan-clear',
                                               'wait-steer', 'wait-deny-process', 'wait-pause', 'wait-clear'])
    a = p.parse_args(); root = a.output.resolve(); root.mkdir(parents=True)
    results = []
    for label in a.cases:
        mode, control = label.split('-', 1)
        case = root / label; workspace = case / 'workspace'; workspace.mkdir(parents=True)
        state = case / 'state'; state.mkdir(mode=0o700)
        (workspace / 'alpha.txt').write_text('INDEPENDENT-ALPHA-826451 Ω\n')
        (workspace / 'beta.txt').write_text('MUST-NOT-READ-BETA-715382 Ω\n')
        worker = ('from pathlib import Path\nimport time,os\n'
                  "Path('pid.txt').write_text(str(os.getpid()))\n"
                  "with Path('starts.log').open('a') as f:f.write('START-826451\\n')\n"
                  "print('READY-827159',flush=True)\n"
                  'end=time.monotonic()+120\n'
                  "while not Path('release-command').exists() and time.monotonic()<end:time.sleep(.05)\n"
                  "print('TERMINAL-937154',flush=True)\n")
        (workspace / 'worker.py').write_text(worker)
        task = ('Use one inspection_plan version 1 containing exactly two workspace_read nodes, '
                'alpha for alpha.txt followed by beta for beta.txt, without result references. '
                'Use the current scope/input_revision. Set cursor=null and max_result_bytes=65536. '
                'Do not read these files outside that plan. If ordinary initial workspace listing is '
                'required, perform it first. No commands or file changes. Return the two exact values.')
        if mode == 'wait':
            task = ('This effectful operation is explicitly authorized. First call run_pipeline to hand it '
                    'to the execution pipeline. Inspect worker.py, then use command_start with program python3, args ["worker.py"], '
                    'interactive=false, purpose=external_effect and timeout_seconds=120. '
                    'The only authorized effects are this one fixture command and its starts.log and pid.txt files. '
                    'Then use command_wait with until=terminal and no max_wait_ms on that exact handle. '
                    'Never restart the process or use command_poll. If the wait yields for control, return its handle and observation without any further tool call. Otherwise return its exact terminal output. '
                    'The fixture controller will release the process; do not create release-command yourself.')
        spec = dict(mode=mode, root=str(workspace), registry=str(case / 'registry.bin'), endpoint=endpoint(state), task=task)
        path = case / 'spec.json'; path.write_text(json.dumps(spec))
        client = a.client.resolve()
        def call(action):
            done = subprocess.run([str(client), str(path), action], capture_output=True, text=True, timeout=35)
            with (case / 'client.jsonl').open('a') as log:
                log.write(json.dumps(dict(action=action, code=done.returncode, stdout=done.stdout, stderr=done.stderr)) + '\n')
            assert done.returncode == 0, (label, action, done.stderr)
            return json.loads(done.stdout)
        folder = call('setup')['folder_identity']
        q = json.dumps
        config = 'version=1\nstore_id="' + '81'*16 + '"\n[paths]\nstate_root=' + q(str(state)) + '\n'
        for name in ['artifact', 'evidence', 'workspace', 'process', 'transaction', 'backup']:
            config += name + '_root=' + q(str(state / (name+'s'))) + '\n'
        config += f'[approval_registry]\npayload_file={q(spec["registry"])}\ngeneration=1\n[human]\nactor_id="' + '31'*16 + '"\n'
        config += '[telemetry]\nmode="disabled"\n[tools]\nallow=["shell.exec","fs.read","fs.discover","fs.search"]\n'
        config += '[context.local]\nread_only_plans="sequential"\ndecision_view="conservative"\n'
        config += '[[folders]]\nworkspace_id="'+'82'*16+f'"\nroot={q(str(workspace))}\nidentity={q(folder)}\nwritable=true\nprotected_paths=[]\n'
        config += '[[providers]]\nkind="codex-runtime"\n[providers.profile]\nprofile_id="'+'95'*16+'"\nrevision=1\nmodel="gpt-6.1-sol"\n'
        config += 'capabilities=["tool-calls","parallel-tool-calls","usage-detail"]\nmax_input_tokens=200000\nmax_output_tokens=16000\nmax_tools=64\nmax_parallel_tool_calls=8\nmax_inline_media_bytes=1\n'
        configuration = case / 'daemon.toml'; configuration.write_text(config)
        source = REPO / 'crates/app/peritus-product-runner/src'
        file = source / ('local_context/operation_plan/execution.rs' if mode != 'wait' else 'developer_tools/executor/wait.rs')
        fragment = 'let allowed = admission.complete' if mode != 'wait' else 'let mut observation = runtime.poll(handle)?;'
        line = next(i for i, text in enumerate(file.read_text().splitlines(), 1) if fragment in text)
        marker = case / 'boundary.json'; release = case / 'release-debugger'
        commands = case / 'gate.gdb'
        commands.write_text('set pagination off\nset confirm off\nset non-stop on\nset detach-on-fork on\npython\n'
            'import gdb,json,time,os\nclass Gate(gdb.Breakpoint):\n'
            ' def stop(self):\n  self.enabled=False\n'
            f'  open({str(marker)+".pending"!r},"w").write(json.dumps(dict(pid=gdb.selected_inferior().pid,stack=gdb.execute("bt 8",to_string=True))))\n'
            f'  os.replace({str(marker)+".pending"!r},{str(marker)!r})\n'
            '  return True\n' + f'Gate({str(file)+":"+str(line)!r})\nend\nrun\npython\n'
            f'assert os.path.exists({str(marker)!r}),"expected boundary not reached"\n'
            f'end=time.monotonic()+90\nwhile not os.path.exists({str(release)!r}) and time.monotonic()<end:time.sleep(.05)\n'
            f'assert os.path.exists({str(release)!r}),"controller did not release boundary"\nend\ncontinue -a\n')
        out = (case / 'daemon.stdout').open('w'); err = (case / 'daemon.stderr').open('w')
        restarted = None
        debugger = subprocess.Popen(['gdb','-nx','-batch','-x',str(commands),'--args',str(a.daemon.resolve()),
                                     'serve','--config',str(configuration)],stdout=out,stderr=err,cwd=REPO)
        try:
            wait_for(lambda: Path(spec['endpoint']).exists() or debugger.poll() is not None, 45)
            assert debugger.poll() is None, (case / 'daemon.stderr').read_text()
            call('start'); wait_for(marker.exists)
            before = call('inspect')
            if control == 'crash':
                native_pid=json.loads(marker.read_text())['pid']
                assert str(configuration).encode() in Path(f'/proc/{native_pid}/cmdline').read_bytes()
                os.kill(native_pid,signal.SIGKILL);release.touch();debugger.wait(timeout=15)
                restart_out=(case/'restart.stdout').open('w');restart_err=(case/'restart.stderr').open('w')
                restarted=subprocess.Popen([str(a.daemon.resolve()),'serve','--config',str(configuration)],stdout=restart_out,stderr=restart_err,cwd=REPO)
                wait_for(lambda:'startup Ready completed' in (case/'restart.stderr').read_text(),45)
                after=call('inspect');time.sleep(1);again=call('inspect')
                assert after['usage']['requests']==again['usage']['requests']==before['usage']['requests']
                assert after['usage']['tool_calls']==again['usage']['tool_calls']==before['usage']['tool_calls']
                records=host_records(state);steps=[r for r in records if r['record']['kind']=='step']
                assert len(steps)==1 and steps[0]['record']['arguments']['path']=='alpha.txt',steps
                assert 'RecoveryRequired' in after['interaction'] or after['usage']['state'] in ['Blocked','Paused'],after
                results.append(dict(case=label,passed=True,actual_daemon_death=True,fresh_daemon=True,
                                    usage_before=before['usage'],usage_after=again['usage'],committed_children=1,
                                    no_automatic_provider_or_child_dispatch=True))
                (root/'results.json').write_text(json.dumps(results,indent=2));print(json.dumps(results[-1]),flush=True)
                continue
            if control == 'permission-restore':
                call('deny-process'); call('allow-process')
            else:
                call(control)
            release.write_text('continue\n')
            if mode == 'wait':
                def ended_wait():
                    return next((row for row in tool_observations(state) if row['name']=='command_wait'), None)
                wait_result = wait_for(ended_wait, 30)
                wait_output = json.loads(wait_result['output'])
                assert wait_output['state']=='unobserved' and not wait_output['wait']['redispatched'], wait_output
                assert wait_output['wait']['reason'] in ['control_changed','cancelled','permission_changed'], wait_output
                pid = int((workspace / 'pid.txt').read_text())
                assert Path(f'/proc/{pid}/cwd').resolve() == workspace, 'ending a wait lost the original live process'
                assert 'worker.py' in Path(f'/proc/{pid}/cmdline').read_bytes().decode(), 'native identity changed'
            time.sleep(1)
            after = call('inspect')
            if control == 'steer':
                def answered():
                    state = call('inspect')
                    return state if any(v['kind']=='Assistant' and 'I5 STEERING RECEIVED' in v['text'] for v in state['activities']) else None
                after = wait_for(answered, 90)
            (case / 'before.json').write_text(json.dumps(before,indent=2)); (case / 'after.json').write_text(json.dumps(after,indent=2))
            if mode == 'wait':
                assert (workspace / 'starts.log').read_text() == 'START-826451\n'
                (workspace / 'release-command').write_text('finish\n')
            if control == 'before-edit':
                outcome = wait_for(lambda: next((row for row in tool_observations(state) if row['name']=='inspection_plan'), None), 20)
                plan_output = json.loads(outcome['output'])
                assert plan_output['status']==('yielded' if a.expect_before_edit_bug else 'completed'), plan_output
                assert plan_output['completed_nodes']==(1 if a.expect_before_edit_bug else 2), plan_output
            records = host_records(state)
            (case / 'host-records.json').write_text(json.dumps(records,indent=2))
            steps = [r for r in records if r['record']['kind'] == 'step']
            if mode != 'wait':
                expected_steps = 2 if control=='before-edit' and not a.expect_before_edit_bug else 1
                assert len(steps) == expected_steps and steps[0]['record']['arguments']['path']=='alpha.txt', steps
            expected_calls = before['usage']['tool_calls'] + (1 if control=='before-edit' and not a.expect_before_edit_bug else 0)
            assert after['usage']['tool_calls']==expected_calls, (before['usage'],after['usage'])
            if mode == 'review':
                assert before['usage']['roles'][1]['tool_calls']==before['usage']['tool_calls'] and before['usage']['tool_calls']>=2 and before['usage']['roles'][0]['tool_calls']==0, before
                assert steps[0]['record']['parent']['model_role']=='reviewer', steps
            if control == 'permission-restore':
                assert after['permissions']['effective']==before['permissions']['effective']
                assert after['permissions']['revision']==before['permissions']['revision']+2
            if control == 'pause':
                assert after['usage']['state'] in ['Paused','Pausing'], after
            if control == 'clear':
                assert after['usage']['state']=='Cancelled', after
            if control == 'steer':
                assert any(v['kind']=='Assistant' and 'I5 STEERING RECEIVED' in v['text'] for v in after['activities']), after
            results.append(dict(case=label,passed=not a.expect_before_edit_bug,expected_bug=a.expect_before_edit_bug,usage_before=before['usage'],usage_after=after['usage'],
                                committed_children=len(steps),actual_native_provider=True,real_daemon_ipc=True,
                                wait_output=wait_output if mode=='wait' else None))
            (root / 'results.json').write_text(json.dumps(results,indent=2)); print(json.dumps(results[-1]),flush=True)
        finally:
            release.touch()
            (workspace / 'release-command').touch()
            if Path(spec['endpoint']).exists():
                subprocess.run([str(a.cli.resolve()),'--endpoint',spec['endpoint'],'shutdown','--wait'],capture_output=True,timeout=40)
            if debugger.poll() is None:
                try: debugger.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    if marker.exists():
                        pid=json.loads(marker.read_text())['pid'];native=Path(f'/proc/{pid}/cmdline')
                        if native.exists() and str(configuration).encode() in native.read_bytes():os.kill(pid,signal.SIGTERM)
                    debugger.terminate();debugger.wait(timeout=10)
            if restarted is not None:
                restarted.wait(timeout=15);restart_out.close();restart_err.close()
            out.close();err.close()


if __name__ == '__main__':
    main()
