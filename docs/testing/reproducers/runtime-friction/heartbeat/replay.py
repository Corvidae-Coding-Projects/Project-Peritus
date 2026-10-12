#!/usr/bin/env python3
"""Real Peritus TUI/daemon heartbeat replay; no replacement protocol or application."""
import argparse, fcntl, hashlib, json, os, pathlib, pty, select, shutil, signal, struct, subprocess, termios, time, uuid

p=argparse.ArgumentParser()
p.add_argument('--bin', required=True, type=pathlib.Path)
p.add_argument('--output', required=True, type=pathlib.Path)
p.add_argument('--registry', type=pathlib.Path, default=pathlib.Path(__file__).with_name('public-approval-registry.bin'))
p.add_argument('--pause-seconds', type=float, default=17, help='Duration for delayed case; contract requires resume before age40s')
p.add_argument('--cases', nargs='+', choices=['ordinary','delayed','timeout'], default=['ordinary','delayed','timeout'])
a=p.parse_args()
if not 0<a.pause_seconds<34: p.error('delayed pause must be greater than0 and below34s')
a.output.mkdir(mode=0o700, parents=True, exist_ok=True)
binary={n:hashlib.sha256((a.bin/n).read_bytes()).hexdigest() for n in ('peritusd','peritus-tui','peritus')}
results=[]
for case in a.cases:
    root=a.output/case
    root.mkdir(mode=0o700)
    state=root/'state'
    env=os.environ.copy()
    for kind in ('config','state','cache'):
        d=root/kind;d.mkdir(mode=0o700,exist_ok=True)
        env['XDG_'+kind.upper()+'_HOME']=str(d)
    env['TERM']='xterm-256color';env['NO_COLOR']='1'
    registry=root/'registry.bin';shutil.copyfile(a.registry,registry);registry.chmod(0o600)
    sid=uuid.uuid4().hex
    lines=['version = 1',f'store_id = "{sid}"','[paths]']
    for key,folder in [('state_root','daemon'),('artifact_root','artifacts'),('evidence_root','evidence'),('workspace_root','workspaces'),('process_root','processes'),('transaction_root','transactions'),('backup_root','backups')]:
        dest=state/'daemon' if key=='state_root' else state/'daemon'/folder
        lines.append(f'{key} = "{dest}"')
    lines.extend(['[approval_registry]',f'payload_file = "{registry}"','generation = 1','[human]','actor_id = "48e4f31c25f0a19c5ee5947ad18d24c7"','[product]','automatic_provider_failover = false','[telemetry]','mode = "disabled"','[tools]','allow = []'])
    cfg=root/'daemon.toml';cfg.write_text('\n'.join(lines)+'\n');cfg.chmod(0o600)
    name='peritus-'+hashlib.sha256(b'peritus/daemon-endpoint/v1\0'+bytes.fromhex(sid)).hexdigest()[:32]+'.sock'
    original=str(state/'daemon'/name)
    endpoint=original if len(original.encode())<=107 else '/tmp/peritus-'+hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0'+original.encode()).hexdigest()[:32]+'/daemon.sock'
    logf=open(root/'daemon.log','wb')
    daemon=subprocess.Popen([str(a.bin/'peritusd'),'serve','--config',str(cfg)],env=env,cwd=root,stdout=logf,stderr=subprocess.STDOUT)
    tui=None;master=None;stopped=False;events=[];begin=time.monotonic(); transcript=bytearray()
    def event(name):
        events.append({'event':name,'elapsed_s':round(time.monotonic()-begin,3)})
        print(case,name,events[-1]['elapsed_s'],flush=True)
    def pump(duration):
        end=time.monotonic()+duration
        while time.monotonic()<end:
            if master is None: time.sleep(min(.1,end-time.monotonic()));continue
            ready,_,_=select.select([master],[],[],min(.1,max(0,end-time.monotonic())))
            if ready:
                try: chunk=os.read(master,65536)
                except OSError: chunk=b''
                if chunk:
                    transcript.extend(chunk)
                    if b"\x1b[6n" in chunk:os.write(master,b"\x1b[1;1R")
    try:
        deadline=time.monotonic()+15
        while not pathlib.Path(endpoint).is_socket():
            if daemon.poll() is not None: raise RuntimeError('daemon exited '+str(daemon.returncode))
            if time.monotonic()>deadline:raise RuntimeError('daemon endpoint not ready')
            time.sleep(.05)
        event('daemon-ready')
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,150,0,0))
        tui=subprocess.Popen([str(a.bin/'peritus-tui'),'--endpoint',endpoint],stdin=slave,stdout=slave,stderr=slave,env=env,cwd=root,start_new_session=True)
        os.close(slave);event('tui-launched');pump(5)
        if tui.poll() is not None:raise RuntimeError('TUI exited before pause: '+str(tui.returncode))
        if case=='ordinary':pump(22)
        else:
            os.kill(tui.pid,signal.SIGSTOP);stopped=True;event('tui-stopped')
            pump(a.pause_seconds if case=='delayed' else 42)
            os.kill(tui.pid,signal.SIGCONT);stopped=False;event('tui-resumed');pump(11 if case=='delayed' else 5)
        event('before-quit')
        if tui.poll() is None:os.write(master,b'\x11');pump(2)
        if tui.poll() is None:tui.terminate();pump(1)
        if tui.poll() is None:tui.kill()
        tui.wait(timeout=5)
    finally:
        if tui is not None:
            if stopped and tui.poll() is None:os.kill(tui.pid,signal.SIGCONT)
            if tui.poll() is None:tui.kill();tui.wait(timeout=5)
        if master is not None:os.close(master)
        if daemon.poll() is None:daemon.terminate()
        try:daemon.wait(timeout=10)
        except subprocess.TimeoutExpired:daemon.kill();daemon.wait(timeout=5)
        logf.close()
        (root/'tui.ansi').write_bytes(transcript)
    log=(root/'daemon.log').read_text(errors='replace')
    mismatch='heartbeat reply does not match' in log
    timeout='heartbeat reply bound was exceeded' in log
    expected='session remains connected after valid replies' if case!='timeout' else 'daemon rejects genuine missed-reply timeout'
    connected=b'connected to daemon' in transcript
    disconnected=b'daemon disconnected' in transcript
    passed=(connected and not disconnected and not mismatch and not timeout and tui.returncode==0) if case!='timeout' else timeout and not mismatch and disconnected
    result={'case':case,'pause_seconds':a.pause_seconds if case=='delayed' else 42 if case=='timeout' else None,'expected':expected,'status':'PASS' if passed else 'FAIL','tui_exit':tui.returncode,'daemon_exit':daemon.returncode,'heartbeat_mismatch':mismatch,'heartbeat_timeout':timeout,'ui_connected':connected,'ui_disconnected':disconnected,'events':events,'endpoint':endpoint,'tui_pid':tui.pid,'daemon_pid':daemon.pid,'binary_sha256':binary}
    (root/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    results.append(result);print(json.dumps(result),flush=True)
(a.output/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
raise SystemExit(0 if all(r['status']=='PASS' for r in results) else 1)
