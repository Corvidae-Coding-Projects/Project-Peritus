#!/usr/bin/env python3
"""Replay confirmed defects through real KWrite/Zenity windows and the shipped driver.
No unit framework or substituted application. Requires Linux native dependencies.
"""
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
with tempfile.TemporaryDirectory(prefix='peritus-native-replay-') as directory:
    root = Path(directory)
    workspace = root/'workspace'
    workspace.mkdir()
    controller = subprocess.Popen([args.python,args.driver,'serve','--state',str(root/'controller'),'--workspace',str(workspace)],stdout=subprocess.DEVNULL)
    try:
        deadline = time.monotonic()+10
        while not (root/'controller/ready.json').exists() and time.monotonic()<deadline:
            if controller.poll() is not None:
                raise RuntimeError('real controller exited during startup')
            time.sleep(.05)
        ready = json.loads((root/'controller/ready.json').read_text())
        def call(method, params):
            request=dict(token=ready['token'],id=uuid.uuid4().hex,method=method,params=params)
            with socket.create_connection(('127.0.0.1',ready['port']),timeout=50) as client:
                client.sendall(json.dumps(request).encode()+b'\n')
                result=json.loads(client.makefile('rb').readline())
            if not result.get('success'):
                raise RuntimeError(result)
            return result
        def action(state, operation, **params):
            return call('desktop',dict(action=operation,session=state['session'],observation=state['observation'],**params))
        document=workspace/'document.txt'
        document.write_text('Before\n')
        state=call('desktop',dict(action='open',program='/usr/bin/kwrite',args=[str(document)],isolated=True))
        # Two consecutive clicks at the same position reproduced the old --sync hang.
        state=action(state,'click',x=140,y=100)
        state=action(state,'click',x=140,y=100)
        state=action(state,'press',key='ctrl+a')
        expected='First distinct line\nSecond distinct line\n'
        state=action(state,'type',text=expected)
        state=action(state,'press',key='ctrl+s')
        deadline=time.monotonic()+5
        while document.read_text()!=expected and time.monotonic()<deadline:
            time.sleep(.1)
        if document.read_text()!=expected:
            raise RuntimeError('KWrite disk readback lost or changed multiline input')
        call('desktop',dict(action='close',session=state['session']))
        state=call('desktop',dict(action='open',program='/usr/bin/zenity',args=['--entry','--title=Peritus real replay','--text=Fixture value'],isolated=True))
        deadline=time.monotonic()+5
        while not any(node.get('editable') for node in state['nodes']) and time.monotonic()<deadline:
            time.sleep(.1)
            state=action(state,'observe')
        entry=next(node['ref'] for node in state['nodes'] if node.get('editable'))
        expected='Renée 43 ✓'
        state=action(state,'fill',ref=entry,text=expected)
        if not any(node.get('text')==expected for node in state['nodes']):
            raise RuntimeError('AT-SPI text lookup hid the native entry or its value')
        external=call('desktop',dict(action='attach',pid=state['pid']))
        call('desktop',dict(action='close',session=external['session']))
        if not Path('/proc',str(state['pid'])).exists():
            raise RuntimeError('detaching killed an existing app')
        log=Path(state['image']['path']).parent/'app.log'
        button=next(node['ref'] for node in state['nodes'] if node['name']=='OK' and node['role']=='button')
        state=action(state,'invoke',ref=button)
        deadline=time.monotonic()+5
        while state.get('state')!='closed' and time.monotonic()<deadline:
            time.sleep(.1)
            state=action(state,'observe')
        if state.get('state')!='closed' or state.get('image') is not None:
            raise RuntimeError('closed app returned live or unrelated screenshot evidence')
        if expected not in log.read_text():
            raise RuntimeError('native dialog stdout differs from entered value')
        call('desktop',dict(action='close',session=state['session']))
        call('shutdown',{})
        controller.wait(timeout=10)
        print('PASS: multiline save, repeated-position click, AT-SPI Unicode readback, detach preservation, closed app evidence and cleanup')
    finally:
        if controller.poll() is None:
            controller.terminate()
            controller.wait(timeout=15)
