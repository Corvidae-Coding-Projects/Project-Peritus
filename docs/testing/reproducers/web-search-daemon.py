#!/usr/bin/env python3
"""Real Linux terminal/daemon/provider discovery-to-primary-source search with no setup."""
from pathlib import Path
import os,time,pexpect,pyte,json,subprocess,argparse,shutil,struct
parser=argparse.ArgumentParser(description='Real Peritus terminal/daemon built-in search journey; uses a signed-in provider')
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--provider-state',type=Path,required=True,help='Existing nonsecret product-state JSON with a working provider selection')
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
BASE=args.output.resolve();BASE.mkdir(parents=True,exist_ok=False);ROOT=BASE/'instance';BIN=args.binary.resolve()
(BASE/'workspace').mkdir();(BASE/'workspace/README.md').write_text('Disposable native-search workspace.\n')
state=ROOT/'state/peritus/product-state';state.mkdir(parents=True);shutil.copy2(args.provider_state,state/args.provider_state.name)

e=dict(os.environ);e.update({f'XDG_{key}_HOME':str(ROOT/folder) for key,folder in [('CONFIG','config'),('STATE','state'),('CACHE','cache'),('DATA','data')]})
e.pop('PERITUS_WEB_SEARCH_URL',None)
screen=pyte.Screen(180,48);stream=pyte.Stream(screen);transcript=''
p=pexpect.spawn(str(BIN),['open',str(BASE/'workspace')],cwd=str(BASE/'workspace'),env=e,encoding='utf-8',timeout=1,dimensions=(48,180));raw=(BASE/'tui-watchdog.ansi').open('w')
def pump(seconds=.3):
 global transcript
 try:
  t=p.read_nonblocking(65536,timeout=seconds);raw.write(t);raw.flush();transcript+=t;stream.feed(t)
  if '\x1b[6n' in t:p.send('\x1b[1;1R')
  if '\x1b[c' in t:p.send('\x1b[?1;2c')
 except pexpect.TIMEOUT:pass
 except pexpect.EOF:raise RuntimeError('TUI exited: '+transcript[-2400:])
 view='\n'.join(screen.display);(BASE/'tui-watchdog-screen.txt').write_text(view);return view
try:
 trusted=False;end=time.monotonic()+120
 while time.monotonic()<end:
  view=pump()
  if not trusted and 'Trust this' in transcript and '[Y/n]' in transcript:p.sendline('y');trusted=True
  if 'Enter sends' in view and 'Ready' in view:break
 else:raise TimeoutError(view)
 print('TUI connected',flush=True)
 prompt="Read-only acceptance: use web_search to find official Rust documentation explaining ownership. Then use web_open on an actual official documentation URL returned by that search, read it, explain one ownership rule and cite the URL. Use the built-in web tools directly, no shell commands, no files, no setup, no rendering. End with NATIVE SEARCH JOURNEY COMPLETE."
 p.send(prompt+'\r');end=time.monotonic()+300
 while time.monotonic()<end:
  view=pump()
  if 'NATIVE SEARCH JOURNEY COMPLETE' in view and 'doc.rust-lang.org' in view and 'working' not in view.lower() and 'ready for your next message' in view:break
 else:raise TimeoutError(view)
 (BASE/'tui-watchdog-final.txt').write_text(view)
 observations=[]
 for trace in (ROOT/'state/peritus/daemon/product-runs').glob('*.trace'):
  data=trace.read_bytes();offset=0
  while offset<len(data):
   tag=data[offset];length=struct.unpack_from('<Q',data,offset+1)[0];payload=data[offset+9:offset+9+length];offset+=9+length
   if tag in (2,7):
    item=json.loads(payload)
    if item['name'] in ('web_search','web_open'):observations.append({'arguments':json.loads(item['arguments']),'is_error':item['is_error'],'output':json.loads(item['output'])})
 assert any(item['arguments'].get('query') and not item['is_error'] and item['output'].get('search_backend') == 'built-in-rust' and item['output'].get('results') for item in observations),observations
 assert any(not item['is_error'] and item['output'].get('method') == 'http' and 'https://doc.rust-lang.org/' in item['output'].get('url','') and item['output'].get('text') for item in observations),observations
 (BASE/'evidence.json').write_text(json.dumps(observations,indent=2))
 print('Actual built-in Rust search followed by primary documentation read',flush=True)
finally:
 if p.isalive():
  p.sendcontrol('q')
  try:p.expect(pexpect.EOF,timeout=8)
  except pexpect.TIMEOUT:p.terminate(force=True)
 p.close();raw.close();print('TUI closed',p.exitstatus,p.signalstatus,flush=True)

 # Stop only this disposable instance, using its exact persisted PID/socket identity.
 instance=ROOT/'state/peritus/daemon/daemon.instance'
 if instance.exists():
  info=dict(line.split('=',1) for line in instance.read_text().splitlines())
  for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines():
   if f"pid={info['pid']}," in line:
    subprocess.run([str(BIN),'--endpoint',line.split()[4],'shutdown','--wait'],check=True,timeout=35)
    break
