#!/usr/bin/env python3
"""Launch an ordinary offline TUI in isolated XDG roots, then leave its owned daemon available."""
import argparse,fcntl,json,os,pty,select,struct,subprocess,termios,time,tomllib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('binary',type=Path);p.add_argument('root',type=Path);a=p.parse_args()
r=a.root.resolve();r.mkdir(parents=True,exist_ok=False);workspace=r/'workspace';workspace.mkdir();(workspace/'canary.txt').write_text('ORIGINAL café 🦀\n')
e=os.environ.copy();e.update({f'XDG_{n.upper()}_HOME':str(r/n)for n in ('config','state','cache')});e['TERM']='xterm-256color'
m,s=pty.openpty();fcntl.ioctl(s,termios.TIOCSWINSZ,struct.pack('HHHH',42,160,0,0));proc=subprocess.Popen([str(a.binary.resolve()),'open',str(workspace)],cwd=workspace,env=e,stdin=s,stdout=s,stderr=s,start_new_session=True);os.close(s);raw=bytearray();answered=set();started=time.monotonic()
try:
 while time.monotonic()-started<55 and proc.poll()is None:
  if select.select([m],[],[],.05)[0]:
   chunk=os.read(m,65536);raw.extend(chunk)
   if b'\x1b[6n'in chunk:os.write(m,b'\x1b[1;1R')
   if b'\x1b[?u'in chunk or b'\x1b[c'in chunk:os.write(m,b'\x1b[?1;2c')
  for prompt,answer in [(b'Trust this workspace?',b'y\r'),(b'Providers (comma-separated',b'0\r')]:
   if prompt in raw and prompt not in answered:answered.add(prompt);os.write(m,answer)
  if b'Enter sends'in raw:break
 else:raise RuntimeError(raw[-5000:].decode(errors='replace'))
 os.write(m,b'\x11');proc.wait(timeout=10)
finally:
 (r/'bootstrap.ansi').write_bytes(raw)
 if proc.poll()is None:proc.terminate();proc.wait(timeout=10)
 os.close(m)
state=r/'state/peritus/daemon';info=dict(line.split('=',1)for line in(state/'daemon.instance').read_text().splitlines()if'='in line);pid=int(info['pid']);endpoint=next(line.split()[4]for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines()if f'pid={pid},'in line)
config=sorted((r/'config/peritus').glob('peritus-*.toml'))[-1];value=tomllib.loads(config.read_text());ids=[f['workspace_id']for f in value.get('folders',[])];assert len(ids)==1,value.keys()
result={'root':str(r),'endpoint':endpoint,'pid':pid,'workspace':ids[0],'config':str(config),'launch_seconds':time.monotonic()-started};(r/'instance.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
