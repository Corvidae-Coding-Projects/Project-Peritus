#!/usr/bin/env python3
"""Replay real mid-inference input through a disposable daemon and official model."""
import json, subprocess, time, argparse, hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--daemon',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--client',type=Path,required=True);p.add_argument('--cli',type=Path,required=True);p.add_argument('--case',required=True);p.add_argument('--old',action='store_true');p.add_argument('--question',action='store_true');p.add_argument('--pause',action='store_true');p.add_argument('--oauth-reference');p.add_argument('--after-text',action='store_true');a=p.parse_args()
base=a.output.resolve();base.mkdir(parents=True,exist_ok=True);case=base/a.case;case.mkdir();workspace=case/'workspace';workspace.mkdir();(workspace/'untouched.txt').write_text('MUST REMAIN UNCHANGED\n');state=case/'state';state.mkdir(mode=0o700)
digest=hashlib.sha256(b'peritus/daemon-endpoint/v1\0'+bytes([0x81])*16).hexdigest()[:32]
original=str(state/('peritus-'+digest+'.sock'));endpoint=original if len(original.encode())<=107 else '/tmp/peritus-'+hashlib.sha256(b'peritus/unix-runtime-endpoint/v1\0'+original.encode()).hexdigest()[:32]+'/daemon.sock'
spec={'mode':'plan','root':str(workspace),'registry':str(case/'registry.bin'),'endpoint':endpoint,'task':'This is an ordinary text-only conversation. Without inspecting files, using tools, or changing anything, write 160 numbered paragraphs about fictional colors, each at least 80 words long. Follow later user steering immediately. Local context bookkeeping is allowed. Workspace and process tools and changes are forbidden.'};path=case/'spec.json';
if a.question:spec['steering']='Question before continuing: what is 7 times 8? Answer now in one sentence containing QUESTION ANSWER 56, then wait for my next message.'
path.write_text(json.dumps(spec))
def call(action):
 r=subprocess.run([str(a.client.resolve()),str(path),action],capture_output=True,text=True,timeout=35)
 with (case/'client.jsonl').open('a') as f:f.write(json.dumps({'action':action,'code':r.returncode,'stdout':r.stdout,'stderr':r.stderr})+'\n')
 if r.returncode:raise RuntimeError(r.stderr)
 return json.loads(r.stdout)
folder=call('setup')['folder_identity'];q=json.dumps
config='version=1\nstore_id="'+'81'*16+'"\n[paths]\nstate_root='+q(str(state))+'\n'
for name in ['artifact','evidence','workspace','process','transaction','backup']:config+=name+'_root='+q(str(state/(name+'s')))+'\n'
config+='[approval_registry]\npayload_file='+q(spec['registry'])+'\ngeneration=1\n[human]\nactor_id="'+'31'*16+'"\n[telemetry]\nmode="disabled"\n[tools]\nallow=["fs.read","fs.discover","fs.search"]\n'
config+='[[folders]]\nworkspace_id="'+'82'*16+'"\nroot='+q(str(workspace))+'\nidentity='+q(folder)+'\nwritable=false\nprotected_paths=[]\n'
config+='[[providers]]\nkind="codex-runtime"\n[providers.profile]\nprofile_id="'+'95'*16+'"\nrevision=1\nmodel="gpt-6.1-sol"\ncapabilities=["tool-calls","parallel-tool-calls","usage-detail"]\nmax_input_tokens=200000\nmax_output_tokens=16000\nmax_tools=64\nmax_parallel_tool_calls=8\nmax_inline_media_bytes=1\n'
if a.oauth_reference:
 config=config.replace('kind="codex-runtime"\n','kind="chatgpt-oauth"\ncredential_reference='+q(a.oauth_reference)+'\n').replace('capabilities=["tool-calls","parallel-tool-calls","usage-detail"]','capabilities=["tool-calls","parallel-tool-calls","usage-detail","streaming","reasoning-controls","reasoning-summaries","reasoning-replay","prompt-caching"]')
configuration=case/'daemon.toml';configuration.write_text(config)
out=(case/'daemon.stdout').open('w');err=(case/'daemon.stderr').open('w');daemon=subprocess.Popen([a.daemon,'serve','--config',str(configuration)],stdout=out,stderr=err)
try:
 deadline=time.monotonic()+20
 while not Path(endpoint).exists():
  if daemon.poll() is not None:raise RuntimeError((case/'daemon.stderr').read_text())
  if time.monotonic()>deadline:raise TimeoutError('daemon readiness')
  time.sleep(.1)
 call('start');deadline=time.monotonic()+35
 while True:
  before=call('inspect')
  if before['usage'] and before['usage']['requests']>=1 and (not a.after_text or any(v['kind']=='Assistant' and v['text'] for v in before['activities'])):break
  if before['usage'] and before['usage']['state'] in ['Blocked','Cancelled','Failed']:raise RuntimeError('provider stopped before the requested observation: '+str(before['activities'][-1:]))
  if time.monotonic()>deadline:raise TimeoutError('first admitted inference')
  time.sleep(.1)
 time.sleep(.5);call('pause' if a.pause else 'steer');steered=time.monotonic();print(json.dumps({'case':a.case,'steering_accepted':True,'before':before['usage']}),flush=True)
 deadline=steered+(3 if a.pause else 12 if a.old else 45);second=None;answered=None
 while time.monotonic()<deadline:
  after=call('inspect')
  if after['usage']['requests']>=2 and second is None:second=time.monotonic()-steered
  if any(v['kind']=='Assistant' and ('QUESTION ANSWER 56' if a.question else 'I5 STEERING RECEIVED') in v['text'] for v in after['activities']) and after['usage']['state']=='WaitingForUser' and after['usage']['roles'][0]['completed_requests']==after['usage']['roles'][0]['requests']:answered=time.monotonic()-steered;break
  if a.old and second is not None:break
  time.sleep(.1)
 receipt={'case':a.case,'actual_provider':'chatgpt-oauth' if a.oauth_reference else 'codex-runtime','input_after_public_text':a.after_text,'real_daemon_ipc':True,'seconds_to_new_request':second,'seconds_to_answer':answered,'usage_before':before['usage'],'usage_after':after['usage'],'workspace_unchanged':(workspace/'untouched.txt').read_text()=='MUST REMAIN UNCHANGED\n'}
 rows=[]
 for trace in (state/'product-runs').glob('*.trace'):
  raw=trace.read_bytes();offset=0
  while offset+9<=len(raw):
   tag=raw[offset];size=int.from_bytes(raw[offset+1:offset+9],'little');payload=raw[offset+9:offset+9+size];offset+=9+size
   if tag==7:rows.append(json.loads(payload))
 names=[row['name'] for row in rows]
 receipt['tool_names']=names
 receipt['no_workspace_or_process_tools']=all(name in ['context_read','context_update','context_sources'] for name in names)
 receipt['passed']=(second is None and after['usage']['roles'][0]['completed_requests']==0 and after['usage']['state']=='Pausing') if a.pause else (second is None) if a.old else (second is not None and second<3 and answered is not None)
 receipt['passed']=receipt['passed'] and receipt['workspace_unchanged'] and receipt['no_workspace_or_process_tools']
 (case/'result.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
 if not (receipt['passed'] and receipt['workspace_unchanged'] and receipt['no_workspace_or_process_tools']):raise RuntimeError('steering requirement failed; inspect evidence')
finally:
 if Path(endpoint).exists():
  subprocess.run([str(a.cli.resolve()),'--endpoint',endpoint,'shutdown','--wait'],capture_output=True,timeout=40)
 try:daemon.wait(timeout=10)
 except subprocess.TimeoutExpired:daemon.terminate();daemon.wait(timeout=10)
 out.close();err.close()
