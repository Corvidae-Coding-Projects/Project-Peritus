#!/usr/bin/env python3
"""Real native TUI, official account, workspace read and durable working-context operations."""
import argparse, hashlib, json, os, subprocess, tempfile, time
from pathlib import Path
from terminal import Terminal
p=argparse.ArgumentParser();p.add_argument('--binary-dir',type=Path,required=True);p.add_argument('--seed-consumer',type=Path,required=True);p.add_argument('--ui',type=Path,required=True);p.add_argument('--probe',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
root=Path(tempfile.mkdtemp(prefix='p145-'))
overrides={f'XDG_{k}_HOME':str(root/v) for k,v in [('CONFIG','c'),('STATE','s'),('CACHE','k')]}
overrides.update(PERITUS_INSTALL_DEPS='0',PERITUS_REAL_ACCOUNT_MODEL='gpt-6.1-sol',PERITUS_FIXTURE_WORKSPACE=str(root/'w'),TERM='xterm-256color',TOKIO_WORKER_THREADS='2')
env=os.environ|overrides
seeded=json.loads(subprocess.check_output([str(a.seed_consumer),'seed','["codex-account"]'],env=env,text=True))
workspace=Path(seeded['state']['workspaces']['recent'][0]['repository_root'])
entry='DISPLAY-CONTEXT-ENTRY-ORACLE-46219';answer='DISPLAY-CONTEXT-TEST-COMPLETE-64291'
content=entry+'\n'+''.join(f'READROW-{index:04d}: independent fixture line {index:04d}.\n' for index in range(300))
(workspace/'large.txt').write_text(content)
(workspace/'AGENTS.md').write_text('Disposable read-only display fixture. Do not create or run unit tests, commands, or workspace edits. Follow the explicit display request.\n')
before=hashlib.sha256((workspace/'large.txt').read_bytes()).hexdigest()
log=(a.output/'daemon.log').open('w');daemon=subprocess.Popen([str(a.binary_dir/'peritusd'),'serve','--config',seeded['configuration']],env=env,stdout=log,stderr=log);ui=None
def inspect(run):
    result=subprocess.run([str(a.probe),seeded['endpoint'],run],env=env,capture_output=True,text=True,timeout=20)
    assert result.returncode==0,result.stderr
    return json.loads(result.stdout)
def find_run(value):
    if isinstance(value,dict):
        for key,item in value.items():
            if key in ('run_id','runId','run') and isinstance(item,str) and len(item)==32:return item
            result=find_run(item)
            if result:return result
    if isinstance(value,list):
        for item in value:
            result=find_run(item)
            if result:return result
try:
    deadline=time.monotonic()+45
    while not Path(seeded['endpoint']).exists():
        assert daemon.poll() is None,(a.output/'daemon.log').read_text()
        assert time.monotonic()<deadline;time.sleep(.05)
    ui=Terminal([a.ui],env,a.output/'native',height=50)
    ui.wait(lambda text:'Enter sends' in text,'ready')
    ui.command('/effort low');ui.pump(.2)
    prompt=f'''Read-only display exercise in this disposable folder. Use only workspace_read and context_read. Read large.txt exactly once through workspace_read. Then use context_read with the actual observation handle for that file in observation_ids, query=null, cursor=null, offset=0, max_bytes=16000, to retrieve the exact archived source through this role's permitted source-retrieval path. Use the actual handle reported in your context, not an invented ID. Do not request working-entry pages or use context_update: this role correctly excludes derived memory. Do not edit files or run commands/tests. Finally respond with exactly {answer} and nothing else.'''
    ui.command(prompt)
    deadline=time.monotonic()+180;run=None;final=None
    while time.monotonic()<deadline:
        ui.pump(.25)
        if not run:
            listed=subprocess.check_output([str(a.binary_dir/'peritus'),'--endpoint',seeded['endpoint'],'--json','runs','list'],env=env,text=True)
            run=find_run(json.loads(listed))
        if run:
            observed=inspect(run)
            if any(item['kind']=='Assistant' and item['text'].strip()==answer for item in observed['activities']):final=observed;break
    assert final,ui.text
    reads=[item for item in final['activities'] if item['kind']=='Tool' and 'context_read' in item['text'] and entry in item['detail']]
    assert reads,final
    assert all('query=null' not in item['text'] for item in final['activities'] if item['kind']=='Tool' and 'context_read' in item['text'])
    for _ in range(60):
        if 'Read saved context' in ui.text:break
        ui.send(b'\x1b[5~');ui.pump(.05)
    ui.wait(lambda text:'Read saved context' in text,'visible-context',timeout=5)
    assert 'saved context for null' not in ui.text
    assert before==hashlib.sha256((workspace/'large.txt').read_bytes()).hexdigest()
    ui.close();ui=None
    (a.output/'fixture.json').write_text(json.dumps({'root':str(root),'overrides':overrides,'configuration':seeded['configuration'],'endpoint':seeded['endpoint'],'run':run,'workspace':str(workspace),'source_sha256':before},indent=2)+'\n')
    (a.output/'result.json').write_text(json.dumps({'passed':True,'actual_context_readback':True,'same_source_bytes':True,'final':final},indent=2)+'\n')
    print(json.dumps({'passed':True,'run':run,'actual_context_readback':True,'fixture_retained_for_display_replay':str(root)}))
finally:
    if ui:ui.close()
    if daemon.poll() is None:
        subprocess.run([str(a.binary_dir/'peritus'),'--endpoint',seeded['endpoint'],'shutdown','--wait'],env=env,capture_output=True,timeout=30)
    if daemon.poll() is None:daemon.terminate()
    daemon.wait(timeout=10);log.close()
