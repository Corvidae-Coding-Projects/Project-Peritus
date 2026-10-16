#!/usr/bin/env python3
"""Ordinary native launcher/TUI and scriptable runs-show against owned copied state."""
import argparse,codecs,fcntl,hashlib,json,os,pty,select,signal,subprocess,termios,time,struct
from pathlib import Path
import pyte
p=argparse.ArgumentParser();p.add_argument('--fixture',required=True,type=Path);p.add_argument('--final',action='store_true');a=p.parse_args();root=a.fixture.resolve();manifest=json.loads((root/'fixture.json').read_text());binary=root/'bin/peritus';project=Path(manifest['project_read_only']);run=manifest['run']
env=os.environ.copy();env.update({f'XDG_{key.upper()}_HOME':str(root/key)for key in ('config','state','cache')});env['TERM']='xterm-256color'
master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,150,0,0));proc=subprocess.Popen([str(binary),'open',str(project),'--run',run],cwd=project,env=env,stdin=slave,stdout=slave,stderr=slave,start_new_session=True);os.close(slave)
screen=pyte.Screen(150,40);stream=pyte.Stream(screen);decode=codecs.getincrementaldecoder('utf8')('replace');raw=bytearray();start=time.monotonic();shown=None;shutdown=None;contract=False;original_boundary=False;result={"contract_satisfied":False,"harness_error":None}
def endpoint():
 # Launcher endpoints may be short-path Unix sockets outside the state directory.
 # Resolve only socket FDs of the daemon whose exact executable/config belong to this fixture.
 instance=root/'state/peritus/daemon/daemon.instance'
 if not instance.exists():return None
 try:
  data=dict(line.split('=',1)for line in instance.read_text().splitlines()if'='in line);pid=int(data['pid']);process=Path('/proc')/str(pid)
  cmd=(process/'cmdline').read_bytes().split(b'\0')
  if cmd[0]!=str(root/'bin/peritusd').encode()or not any(str(root/'config').encode()in part for part in cmd):return None
  inodes=set()
  for fd in(process/'fd').iterdir():
   try:target=os.readlink(fd)
   except OSError:continue
   if target.startswith('socket:['):inodes.add(target[8:-1])
  for line in Path('/proc/net/unix').read_text().splitlines()[1:]:
   fields=line.split()
   if len(fields)>=8 and fields[6]in inodes and fields[7].endswith('/daemon.sock'):return fields[7]
 except(OSError,KeyError,ValueError):return None
 return None
try:
 while time.monotonic()-start<35:
  if select.select([master],[],[],.05)[0]:
   try:data=os.read(master,65536)
   except OSError:break
   raw.extend(data);stream.feed(decode.decode(data))
   if b'\x1b[?u'in data:os.write(master,b'\x1b[?1;2c')
   if b'\x1b[6n'in data:os.write(master,f'\x1b[{screen.cursor.y+1};{screen.cursor.x+1}R'.encode())
  socket=endpoint()
  if socket and shown is None:
   result=subprocess.run([str(binary),'--endpoint',str(socket),'--json','runs','show','--run',run],capture_output=True,text=True,env=env,timeout=8)
   try:payload=json.loads(result.stdout)
   except ValueError:payload={'stdout':result.stdout,'stderr':result.stderr}
   shown={'exit':result.returncode,'payload':payload,'seconds':round(time.monotonic()-start,3)};(root/'runs-show.json').write_text(json.dumps(shown,indent=2))
  text='\n'.join(screen.display)
  if shown is not None and((('Completed in place'in text)if a.final else('Peritus ·'in text))or proc.poll()is not None):break
  if proc.poll()is not None:break
 (root/'terminal.raw').write_bytes(raw);(root/'terminal.screen.txt').write_text('\n'.join(screen.display))
 logs=[]
 for f in (root/'state/peritus/logs').glob('*.log'):
  if f.stat().st_size<8*1048576:logs.append(f.read_text(errors='replace'))
 daemon_text='\n'.join(logs)
 tui_recovered='Peritus ·'in'\n'.join(screen.display)and'Completed in place'in'\n'.join(screen.display)
 contract=bool(tui_recovered and shown and shown['exit']==0 and shown['payload'].get('ok')and shown['payload']['result']['run_id']==run and shown['payload']['result']['phase']=='Complete'and'SOURCE-REPAIRED'in shown['payload']['result']['summary'])
 quarantines=list((root/'state/peritus/daemon/workbench-v1/runs/.quarantine').glob('*'))
 original_boundary=not contract and any(run in q.name for q in quarantines)and'exceeds its file bounds'in daemon_text
 shown_summary=None
 if shown is not None:
  value=shown['payload'].get('result',{});shown_summary={'exit':shown['exit'],'seconds':shown['seconds'],'ok':shown['payload'].get('ok',False),'run_id':value.get('run_id'),'phase':value.get('phase'),'status':value.get('status'),'summary_sha256':hashlib.sha256(value.get('summary','').encode()).hexdigest()}
 result={'contract_satisfied':contract,'original_boundary_reproduced':original_boundary,'mode':'final'if a.final else'original','runs_show':shown_summary,'quarantine_files':[q.name for q in quarantines],'seconds':round(time.monotonic()-start,3),'tui_connected':'Peritus ·'in'\n'.join(screen.display),'tui_completed_run_visible':tui_recovered,'provider_dispatch':False,'effects_dispatched':False}
except Exception as error:
 result['harness_error']=repr(error)
 (root/'terminal.raw').write_bytes(raw);(root/'terminal.screen.txt').write_text('\n'.join(screen.display))
finally:
 if proc.poll()is None:
  os.write(master,b'\x11')
  try:proc.wait(timeout=10)
  except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGTERM);proc.wait(timeout=10)
 os.close(master)
 socket=endpoint()
 if socket:
  shutdown=subprocess.run([str(binary),'--endpoint',str(socket),'shutdown','--wait'],capture_output=True,text=True,env=env,timeout=30);(root/'shutdown.txt').write_text(shutdown.stdout+shutdown.stderr)
 after={str(f.relative_to(project)):hashlib.sha256(f.read_bytes()).hexdigest()for f in project.rglob('*')if f.is_file()};assert after==manifest['source_project_sha256'],'Read-only source fixture was changed'
 result['project_unchanged']=True;result['shutdown_exit']=shutdown.returncode if shutdown else None;(root/'result.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
if a.final and not contract:raise SystemExit(1)
if not a.final and not original_boundary:raise SystemExit('Original failure was not the expected metadata-size boundary; inspect evidence')
