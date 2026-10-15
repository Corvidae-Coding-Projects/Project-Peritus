#!/usr/bin/env python3
"""Operate two real Peritus terminal clients on an owned disposable XDG installation."""
from pathlib import Path
import argparse,re,os,pty,fcntl,termios,struct,subprocess,select,time,codecs,json,hashlib,signal
import pyte
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--bare',action='store_true');p.add_argument('--unavailable-inactive',action='store_true');p.add_argument('--ack-recovery',action='store_true');p.add_argument('--legacy-timeout',action='store_true');p.add_argument('--upgrade-from',type=Path);p.add_argument('--reject-updates',action='store_true');p.add_argument('--expect-bug',action='store_true');p.add_argument('--active-task',action='store_true');p.add_argument('--first-read-only',action='store_true');p.add_argument('--same-workspace',action='store_true');p.add_argument('--git',action='store_true');p.add_argument('--second-read-only',action='store_true');a=p.parse_args()
root=a.output.resolve();root.mkdir(parents=True,exist_ok=False)
(root/'owned-fixture.json').write_text(json.dumps({'owned_disposable_fixture':True}))
env=os.environ.copy();env.update({f'XDG_{n.upper()}_HOME':str(root/n) for n in ('config','state','cache')});env['TERM']='xterm-256color'
windows=[];results=[]
class Window:
 def __init__(self,name,workspace,trust=True,binary=None):
  self.name=name;self.trust=trust;self.closed=False;self.master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',36,140,0,0));self.proc=subprocess.Popen([str((binary or a.binary).resolve())]+([] if a.bare else ['open',str(workspace)]),cwd=workspace,stdin=slave,stdout=slave,stderr=slave,env=env,start_new_session=True);os.close(slave)
  self.screen=pyte.Screen(140,36);self.stream=pyte.Stream(self.screen);self.decoder=codecs.getincrementaldecoder('utf-8')('replace');self.raw=bytearray();self.answered=set();windows.append(self)
 def pump(self,seconds=.1):
  if self.closed:return
  end=time.monotonic()+seconds
  while time.monotonic()<end:
   if select.select([self.master],[],[],min(.03,max(.001,end-time.monotonic())))[0]:
    try:data=os.read(self.master,65536)
    except OSError:break
    self.raw.extend(data);self.stream.feed(self.decoder.decode(data))
    if b'\x1b[?u' in data:os.write(self.master,b'\x1b[?1;2c')
    if b'\x1b[6n' in data:os.write(self.master,f'\x1b[{self.screen.cursor.y+1};{self.screen.cursor.x+1}R'.encode())
 def ready(self):return any(s.startswith('Peritus ·') for s in self.screen.display) and any(s.startswith(('Ready ·','Idle — ready')) for s in self.screen.display)
 def open(self):
  started=time.monotonic()
  while time.monotonic()-started<40 and self.proc.poll() is None:
   for w in windows:w.pump(.08)
   text='\n'.join(self.screen.display)
   for prompt,answer in [('Trust this workspace?','y\r' if self.trust else 'n\r'),('Providers (comma-separated','1\r' if a.active_task else '0\r'),('Choose a model number','1\r')]:
    if prompt in text and prompt not in self.answered:self.answered.add(prompt);os.write(self.master,answer.encode())
   if self.ready():self.save('opened');return round(time.monotonic()-started,3)
  self.save('open-failed');return None
 def save(self,name):
  (root/f'{self.name}-{name}.screen.txt').write_text('\n'.join(self.screen.display)+'\n');(root/f'{self.name}.ansi').write_bytes(self.raw)
 def close(self):
  if self.closed:return
  self.closed=True
  if self.proc.poll() is None:
   os.write(self.master,b'\x11');self.pump(.2)
   try:self.proc.wait(timeout=4)
   except subprocess.TimeoutExpired:self.proc.terminate();self.proc.wait(timeout=4)
  self.save('closed');os.close(self.master)
def daemon():
 p=root/'state/peritus/daemon/daemon.instance'
 if not p.exists():return None
 return dict(line.split('=',1) for line in p.read_text().splitlines() if '=' in line)
try:
 first=root/'workspace alpha';second=root/'workspace β';first.mkdir();second.mkdir();(first/'canary.txt').write_text('FIRST-WORKSPACE-CANARY\n');(second/'canary.txt').write_text('SECOND-WORKSPACE-CANARY\n')
 if a.git:
  for folder in (first,second):
   subprocess.run(['git','init','--quiet',str(folder)],check=True)
   subprocess.run(['git','-C',str(folder),'-c','user.name=Replay','-c','user.email=replay@example.invalid','add','canary.txt'],check=True)
   subprocess.run(['git','-C',str(folder),'-c','user.name=Replay','-c','user.email=replay@example.invalid','commit','--quiet','-m','Fixture baseline'],check=True)
 if a.same_workspace:second=first
 legacy_hashes={}
 if a.upgrade_from:
  legacy=Window('legacy',first,not a.first_read_only,a.upgrade_from)
  if legacy.open() is None:raise RuntimeError('Legacy fixture failed to start')
  current=daemon();legacy.close()
  listener=next(line for line in subprocess.check_output(['ss','-xlpn'],text=True).splitlines() if f"pid={current['pid']}," in line);endpoint=re.search(r'(/\S+\.sock)',listener).group(1)
  stop=subprocess.run([str(a.upgrade_from.resolve()),'--endpoint',endpoint,'--json','shutdown','--wait'],env=env,capture_output=True,text=True,timeout=40)
  if stop.returncode:raise RuntimeError('Legacy fixture did not shut down cleanly')
  if a.legacy_timeout:
   latest=sorted((root/'config/peritus').glob('peritus-*.toml'))[-1]
   text=latest.read_text();old='\n\n[telemetry]\n';legacy='\nprovider_turn_timeout_seconds = 600\n\n[telemetry]\n'
   if old not in text:raise RuntimeError('Legacy fixture has unexpected configuration framing')
   latest.write_text(text.replace(old,legacy,1))
  legacy_hashes={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in (root/'config/peritus').glob('peritus-*.toml')}
 one=Window('first',first,not a.first_read_only);elapsed1=one.open();before=daemon();one.save('before-second')
 if elapsed1 is None:raise RuntimeError('First actual terminal client did not become ready')
 active_started=False
 if a.active_task:
  (first/'worker.py').write_text('import pathlib,time\np=pathlib.Path("worker.log")\nfor i in range(100):\n with p.open("a") as f:f.write(str(i)+"\\n")\n time.sleep(.2)\nprint("WORKER-FINISHED-100")\n')
  task='In read-only planning mode, read canary.txt and worker.py with workspace tools. Echo the literal canary line and explain the script. Do not execute it, invoke run_pipeline, edit files, or run tests or verification.'
  os.write(one.master,b'/plan\r');one.pump(.2)
  os.write(one.master,b'\x1b[200~'+task.encode()+b'\x1b[201~\r')
  deadline=time.monotonic()+60
  while time.monotonic()<deadline:
   one.pump(.1);one.save('active-current')
   paths=list((root/'state/peritus/daemon/workbench-v1/runs').glob('*.json'))
   records=[json.loads(path.read_text()) for path in paths]
   if any(record.get('progress',{}).get('provider_started_unix_millis') is not None for record in records):
    active_started=True;(root/'active-at-second-launch.json').write_text(json.dumps([{'run_id':r['run_id'],'phase':r['phase'],'progress':r['progress']} for r in records],indent=2));break
  one.save('active-task-started')
  if not active_started:raise RuntimeError('No real provider turn observed before the second launch')
 if a.unavailable_inactive:
  spare=root/'remembered spare';spare.mkdir();(spare/'canary.txt').write_text('SPARE-CANARY\n')
  extra=Window('spare',spare)
  if extra.open() is None:raise RuntimeError('Spare workspace did not become ready')
  extra.close();spare.rename(root/'unavailable-held')
 ack_failure_preserved=True
 if a.ack_recovery:
  pending=root/'state/peritus/daemon/workspace-applied.pending';pending.mkdir()
  blocked=Window('ack-blocked-second',second,not a.second_read_only)
  blocked_ready=blocked.open();one.pump(.2)
  ack_failure_preserved=blocked_ready is None and one.ready() and daemon()['pid']==before['pid']
  blocked.close();pending.rmdir()
 two=Window('second',second,not a.second_read_only);elapsed2=two.open();after=daemon()
 for w in windows:w.pump(.5);w.save('after-second')
 permission_ok=False
 if two.ready():
  os.write(two.master,b'/permissions\r')
  for _ in range(20):
   for w in windows:w.pump(.05)
   if any(line.lstrip('│ ').startswith('write ') and ('effective=denied' if a.second_read_only else 'effective=allowed') in line for line in two.screen.display):permission_ok=True;break
  two.save('permissions');os.write(two.master,b'\x1b');two.pump(.2)

 if active_started and two.ready():
  os.write(two.master,b'/plan\r');two.pump(.2)
  task='Read only canary.txt with workspace tools and echo its literal line. Do not execute commands, edit files, invoke run_pipeline, or run tests or verification.'
  os.write(two.master,b'\x1b[200~'+task.encode()+b'\x1b[201~\r');two.pump(.2)
 same=before is not None and after is not None and before['pid']==after['pid'] and before['start_token']==after['start_token']
 results={'ack_failure_preserved_first':ack_failure_preserved,'second_permission_inspection':permission_ok,'active_task_started':active_started,'binary_sha256':hashlib.file_digest(open(a.binary,'rb'),'sha256').hexdigest(),'first_ready_seconds':elapsed1,'second_ready_seconds':elapsed2,'daemon_before':before,'daemon_after':after,'daemon_unchanged':same,'first_still_ready':one.ready(),'first_process_alive':one.proc.poll() is None,'second_ready':elapsed2 is not None,'expected':'Both clients ready; daemon PID/start token unchanged; first client remains connected.'}
 if active_started:
  deadline=time.monotonic()+100
  completed=[];overlap=False
  while time.monotonic()<deadline:
   for w in windows:w.pump(.1)
   completed=[json.loads(path.read_text()) for path in (root/'state/peritus/daemon/workbench-v1/runs').glob('*.json')]
   overlap=overlap or sum(r.get('progress',{}).get('provider_started_unix_millis') is not None for r in completed)>1
   if len(completed)>=2 and all(r['status'].startswith(('Idle','Completed','Stopped')) for r in completed):break
  results['simultaneous_provider_turns_observed']=overlap
  results['active_task_completed']=any(r['status'].startswith(('Idle','Completed')) and 'FIRST-WORKSPACE-CANARY' in r.get('summary','') for r in completed)
  results['second_task_completed']=any(r['status'].startswith(('Idle','Completed')) and 'SECOND-WORKSPACE-CANARY' in r.get('summary','') and 'FIRST-WORKSPACE-CANARY' not in r.get('summary','') for r in completed)
  results['first_remains_connected']=not any('Disconnected' in line or 'connection timed out' in line for line in one.screen.display)
  results['read_only_task_left_files_unchanged']=(first/'canary.txt').read_text()=='FIRST-WORKSPACE-CANARY\n' and not (first/'worker.log').exists()
  one.save('active-task-finished')
 if a.reject_updates:
  runtime=root/'state/peritus/daemon';ack=(runtime/'workspace-applied-generation').read_text();config=sorted((root/'config/peritus').glob('peritus-*.toml'))[-1];text=config.read_text()
  listener=next(line for line in subprocess.check_output(['ss','-xlpn'],text=True).splitlines() if f"pid={before['pid']}," in line);endpoint=re.search(r'(/\S+\.sock)',listener).group(1)
  rejected=[]
  for index,(name,payload) in enumerate([
   ('runtime-policy',text.replace('automatic_provider_failover = false','automatic_provider_failover = true',1).encode()),
   ('unknown-field',(text+'\nunexpected_catalog_field = true\n').encode()),
   ('oversize',b'#'*(1024*1024+1)),
   ('symlink',None),
  ]):
   path=runtime/f'workspace-update-{int(ack.splitlines()[0].split("=",1)[1])+100+index:020}.toml'
   if payload is None:path.symlink_to(config)
   else:path.write_bytes(payload)
   log=runtime.parent/'logs/peritusd.log';offset=log.stat().st_size if log.exists() else 0
   try:
    deadline=time.monotonic()+3
    while time.monotonic()<deadline:
     if log.exists() and 'workspace update rejected:' in log.read_text()[offset:]:break
     time.sleep(.05)
    observed_rejection=log.exists() and 'workspace update rejected:' in log.read_text()[offset:]
    status=subprocess.run([str(a.binary.resolve()),'--endpoint',endpoint,'--json','--timeout-seconds','5','status'],env=env,capture_output=True,text=True,timeout=8)
    for w in windows:w.pump(.05)
    observed={'case':name,'actual_rejection_observed':observed_rejection,'status_available':status.returncode==0,'catalog_not_advanced':(runtime/'workspace-applied-generation').read_text()==ack,'daemon_unchanged':daemon()['pid']==before['pid'],'first_connected':one.ready()}
    rejected.append(observed)
   finally:path.unlink();time.sleep(.6)
  results['rejected_updates']=rejected
 # Reconnect the original client after the other window selected a newer global generation.
 os.write(one.master,b'\x12');one.pump(.3)
 deadline=time.monotonic()+12
 while time.monotonic()<deadline and not one.ready():one.pump(.1)
 results['first_reconnects']=one.ready() and daemon()['pid']==before['pid'];one.save('reconnected')
 two.close();one.pump(.3);results['first_survives_second_close']=one.ready()
 if a.unavailable_inactive:
  (root/'unavailable-held').rename(root/'remembered spare')
  restored=Window('restored-spare',root/'remembered spare')
  results['inactive_folder_restores']=restored.open() is not None and daemon()['pid']==before['pid'];restored.close()
 results['legacy_configurations_unchanged']=all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in legacy_hashes.items())
 results['satisfies_contract']=results.get('inactive_folder_restores',True) and ack_failure_preserved and same and results['first_process_alive'] and results['second_ready'] and (results.get('first_remains_connected',results['first_still_ready'])) and (not active_started or (results['active_task_completed'] and results['second_task_completed'] and results['read_only_task_left_files_unchanged'])) and permission_ok and results['first_reconnects'] and results['first_survives_second_close'] and results['legacy_configurations_unchanged'] and all(all(v for k,v in row.items() if k!='case') for row in results.get('rejected_updates',[]))
 (root/'result.json').write_text(json.dumps(results,indent=2));print(json.dumps(results),flush=True)
finally:
 for w in reversed(windows):w.close()
 current=daemon()
 if current:
  process=Path('/proc')/current['pid']/'cmdline'
  if not process.exists() or str(root) not in process.read_bytes().decode(errors='replace').replace('\0',' '):raise RuntimeError('Refusing cleanup of an unowned daemon')
  listeners=subprocess.check_output(['ss','-xlpn'],text=True)
  lines=[line for line in listeners.splitlines() if f"pid={current['pid']}," in line]
  if len(lines)!=1:raise RuntimeError('Cannot identify the owned daemon listener for cleanup')
  endpoint=re.search(r'(/\S+\.sock)',lines[0]).group(1)
  stop=subprocess.run([str(a.binary.resolve()),'--endpoint',str(endpoint),'--json','shutdown','--wait'],env=env,capture_output=True,text=True,timeout=40)
  (root/'shutdown.json').write_text(stop.stdout);print('fixture shutdown',stop.returncode,flush=True)
  if stop.returncode:raise RuntimeError('Owned daemon did not report clean shutdown')

if not results.get('satisfies_contract',False) and not a.expect_bug:raise SystemExit('Real multi-window contract failed; see retained screens and result.json')
