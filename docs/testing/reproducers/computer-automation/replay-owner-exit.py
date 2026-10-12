#!/usr/bin/env python3
"""Run real native apps; owner exit must close owned apps and preserve attached apps."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import uuid

parser = argparse.ArgumentParser()
parser.add_argument('--python', default=str(Path.home()/'.local/share/peritus/automation/runtime/bin/python'))
parser.add_argument('--driver', default=str(Path.home()/'.local/share/peritus/automation/driver/driver.py'))
args = parser.parse_args()
with tempfile.TemporaryDirectory(prefix='peritus-owner-exit-') as directory:
    root = Path(directory)
    workspace = root/'workspace'
    workspace.mkdir()
    processes = []
    def start(name, owner=None, env=None):
        state = root/name
        command = [args.python,args.driver,'serve','--state',str(state),'--workspace',str(workspace)]
        if owner is not None:
            command += ['--owner-pid',str(owner.pid)]
        child = subprocess.Popen(command,env=env,stdout=subprocess.DEVNULL)
        processes.append(child)
        deadline = time.monotonic()+10
        while not (state/'ready.json').exists() and time.monotonic()<deadline:
            if child.poll() is not None:
                raise RuntimeError('controller exited during startup')
            time.sleep(.05)
        ready = json.loads((state/'ready.json').read_text())
        def call(method, params):
            with socket.create_connection(('127.0.0.1',ready['port']),timeout=45) as client:
                client.sendall(json.dumps(dict(token=ready['token'],id=uuid.uuid4().hex,method=method,params=params)).encode()+b'\n')
                result = json.loads(client.makefile('rb').readline())
            if not result.get('success'):
                raise RuntimeError(result)
            return result
        return child, call
    try:
        existing_controller, existing = start('existing')
        external = existing('desktop',dict(action='open',program='/usr/bin/zenity',args=['--entry','--title=Existing fixture','--text=Preserve this app'],isolated=True))
        # Use this fixture's actual X11 display, so attachment never captures the host desktop.
        fields = Path('/proc',str(external['pid']),'environ').read_bytes().split(b'\0')
        display = next(value.split(b'=',1)[1].decode() for value in fields if value.startswith(b'DISPLAY='))
        env = os.environ.copy()
        env['DISPLAY'] = display
        env.pop('WAYLAND_DISPLAY',None)
        owner = subprocess.Popen(['/bin/sleep','60'])
        processes.append(owner)
        controlled, call = start('controlled',owner,env)
        attached = call('desktop',dict(action='attach',pid=external['pid']))
        owned = call('desktop',dict(action='open',program='/usr/bin/kwrite',isolated=True))
        owner.terminate()
        owner.wait(timeout=5)
        controlled.wait(timeout=15)
        if Path('/proc',str(owned['pid'])).exists():
            raise RuntimeError('owned app survived its daemon owner')
        if not Path('/proc',str(external['pid'])).exists():
            raise RuntimeError('owner-exit cleanup killed the attached existing app')
        preserved = existing('desktop',dict(action='observe',session=external['session']))
        if preserved.get('state') == 'closed' or not preserved.get('image'):
            raise RuntimeError('the existing app did not supply a fresh live window observation')
        if (root/'controlled/ready.json').exists():
            raise RuntimeError('exited controller retained a live readiness claim')
        existing('desktop',dict(action='close',session=external['session']))
        existing('shutdown',{})
        existing_controller.wait(timeout=10)
        print('PASS: owner exit closes real owned KWrite, preserves attached Zenity, removes readiness, and completes cleanup')
    finally:
        for child in reversed(processes):
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=15)
