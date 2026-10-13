"""Abruptly kill only a process this driver started, then reopen its actual daemon state."""
import json, os, pathlib, signal, subprocess, sys, time
root=pathlib.Path(sys.argv[1]).resolve()
binary=pathlib.Path(os.environ['PERITUS_RSI_CONSUMER']).resolve()
log=root/'native-crash-killed-owner.log'
with log.open('wb') as output:
    owner=subprocess.Popen([str(binary),str(root),'crash','start'],stdout=output,stderr=subprocess.STDOUT)
    began=time.monotonic()
    observation=None
    while time.monotonic()-began<40:
        if owner.poll() is not None: raise RuntimeError(f'owned daemon exited before crash: {owner.returncode}')
        started=root/'native-crash-started.json'
        if started.exists():
            start=json.loads(started.read_text())
            if start['pid']!=owner.pid: raise RuntimeError('fixture owner identity changed')
            campaign=bytes.fromhex(start['campaign'])
            for path in (root/'rsi/rsi-child-receipts').glob('*-evaluator.json'):
                receipt=json.loads(path.read_text())
                if bytes(receipt['campaign'])!=campaign: continue
                process=bytes(receipt['process']).hex()
                manifest=root/'processes/manifests-v1'/f'{process}.manifest'
                leaf=pathlib.Path(os.environ['PERITUS_RSI_CGROUP_ROOT'])/('peritus-'+bytes(receipt['preparation'])[:12].hex())
                try: members=(leaf/'cgroup.procs').read_text().split()
                except FileNotFoundError: members=[]
                if manifest.exists() and members:
                    observation={'owner_pid':owner.pid,'campaign':start['campaign'],'receipt':str(path),'actual_manifest':str(manifest),'actual_cgroup':str(leaf),'actual_cgroup_members':members,'signal':'SIGKILL only driver-owned daemon process'}
                    break
        if observation: break
        time.sleep(.02)
    if observation is None:
        owner.terminate(); owner.wait(timeout=40)
        raise RuntimeError('no actual live owned evaluator observed within finite crash bound')
    # Popen owns this PID from creation; the child is still running and its own startup receipt agrees.
    cmdline=pathlib.Path(f'/proc/{owner.pid}/cmdline').read_bytes().split(b'\0')
    if str(root).encode() not in cmdline: raise RuntimeError('owned child command scope differs')
    owner.send_signal(signal.SIGKILL)
    observation['owner_exit']=owner.wait(timeout=10)
(root/'actual-crash-observation.json').write_text(json.dumps(observation,indent=2))
print(json.dumps(observation,indent=2))
with (root/'native-crash-resumed-owner.log').open('wb') as output:
    result=subprocess.run([str(binary),str(root),'crash','resume'],stdout=output,stderr=subprocess.STDOUT,timeout=200)
print('ACTUAL RESUME EXIT',result.returncode)
sys.exit(result.returncode)
