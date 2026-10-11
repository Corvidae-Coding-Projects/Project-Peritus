#!/usr/bin/env python3
"""Open and observe a real KWrite File popup on its isolated display."""
import argparse
import json
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import uuid

parser = argparse.ArgumentParser()
parser.add_argument('--python', default=str(Path.home()/'.local/share/peritus/automation/runtime/bin/python'))
parser.add_argument('--driver', default=str(Path.home()/'.local/share/peritus/automation/driver/driver.py'))
parser.add_argument('--evidence-dir', type=Path)
args = parser.parse_args()
with tempfile.TemporaryDirectory(prefix='peritus-menu-replay-') as folder:
    root = Path(folder)
    workspace = root/'workspace'
    workspace.mkdir()
    document = workspace/'document.txt'
    document.write_text('Menu fixture\n')
    child = subprocess.Popen([args.python,args.driver,'serve','--state',str(root/'controller'),'--workspace',str(workspace)],stdout=subprocess.DEVNULL)
    try:
        deadline = time.monotonic()+10
        while not (root/'controller/ready.json').exists() and time.monotonic()<deadline:
            if child.poll() is not None:
                raise RuntimeError('controller exited during startup')
            time.sleep(.05)
        ready = json.loads((root/'controller/ready.json').read_text())
        def call(params):
            with socket.create_connection(('127.0.0.1',ready['port']),timeout=45) as client:
                client.sendall(json.dumps(dict(token=ready['token'],id=uuid.uuid4().hex,method='desktop',params=params)).encode()+b'\n')
                result = json.loads(client.makefile('rb').readline())
            if not result.get('success'):
                raise RuntimeError(result)
            return result
        state = call(dict(action='open',program='/usr/bin/kwrite',args=[str(document)],isolated=True))
        deadline = time.monotonic()+5
        while not any(n['name']=='UTF-8' and n.get('showing') for n in state.get('nodes',[])) and time.monotonic()<deadline:
            time.sleep(.1)
            state = call(dict(action='observe',session=state['session']))
        if not any(n['name']=='UTF-8' and n.get('showing') for n in state.get('nodes',[])):
            raise RuntimeError('KWrite did not expose its ready document controls')
        menu_bar = next(n for n in state['nodes'] if n['role']=='menu bar')
        if not menu_bar.get('showing'):
            raise RuntimeError('Isolated KWrite exported or hid its menu bar')
        file_menu = next(n for n in state['nodes'] if n['role']=='menu item' and n['name']=='File')
        state = call(dict(action='invoke',session=state['session'],observation=state['observation'],ref=file_menu['ref']))
        state = call(dict(action='observe',session=state['session']))
        new_window = next(n for n in state['nodes'] if n['name']=='New Window' and n.get('showing'))
        branch = new_window['parent']
        state = call(dict(action='observe',session=state['session'],ref=branch))
        controls = [n for n in state['nodes'] if n['role'] in ('menu item','check menu item','radio menu item')]
        if not any(n['name']=='New Window' and n.get('showing') for n in controls) or not state.get('image'):
            raise RuntimeError('Displayed File menu lost its real New Window control or image')
        if args.evidence_dir:
            args.evidence_dir.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(state['image']['path'],args.evidence_dir/'native-open-menu.png')
            (args.evidence_dir/'native-open-menu-evidence.json').write_text(json.dumps(state,ensure_ascii=False,indent=2))
        call(dict(action='close',session=state['session']))
        print('PASS: scoped real File menu exposes New Window and an actual popup image')
    finally:
        child.terminate()
        child.wait(timeout=15)
