#!/usr/bin/env python3
"""Real Linux PTY/daemon/provider recovery from a JavaScript page that never settles."""
from pathlib import Path
import os,time,pexpect,pyte,json,http.server,threading,subprocess,argparse,shutil,struct
parser=argparse.ArgumentParser(description='Real Peritus terminal/daemon stalled-reader recovery replay; uses a signed-in provider')
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--provider-state',type=Path,required=True,help='Existing nonsecret product-state JSON with a working provider selection')
parser.add_argument('--automation-python',required=True)
parser.add_argument('--browser-cache',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
BASE=args.output.resolve();BASE.mkdir(parents=True,exist_ok=False);ROOT=BASE/'instance';BIN=args.binary.resolve()
(BASE/'workspace').mkdir();(BASE/'workspace/README.md').write_text('Disposable web-reader recovery workspace.\n')
state=ROOT/'state/peritus/product-state';state.mkdir(parents=True);shutil.copy2(args.provider_state,state/args.provider_state.name)

class Page(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  body=b'''<!doctype html><title>Compass Observatory</title><h1>Harbor route</h1><p id="result">Waiting for script</p><script>setTimeout(()=>{document.getElementById('result').textContent='Compass 7821: JavaScript arrived';},120);</script>'''
  if self.path == '/hang':body=b'<title>Wedged page</title><p>Before loop</p><script>setTimeout(()=>{while(true){}},100)</script>'
  self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
 def log_message(self,*a):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Page);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
e=dict(os.environ);e.update({f'XDG_{key}_HOME':str(ROOT/folder) for key,folder in [('CONFIG','config'),('STATE','state'),('CACHE','cache'),('DATA','data')]});e['PLAYWRIGHT_BROWSERS_PATH']=str(args.browser_cache.resolve());e['PERITUS_AUTOMATION_PYTHON']=args.automation_python
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
 prompt=f"Read-only resilience acceptance: use web_open render=true for http://127.0.0.1:{server.server_port}/hang with wait_ms=0. The page intentionally wedges after navigation; allow the host to return its bounded error. Then use web_open render=true on http://127.0.0.1:{server.server_port}/ and report its actual Compass status. Do not give up after the expected first failure; do not run shell commands or change files. End with WEB JOURNEY COMPLETE after reading the healthy page."
 p.send(prompt+'\r');end=time.monotonic()+300
 while time.monotonic()<end:
  view=pump()
  if 'WEB JOURNEY COMPLETE' in view and 'Compass 7821' in view and 'working' not in view.lower() and 'ready for your next message' in view:break
 else:raise TimeoutError(view)
 (BASE/'tui-watchdog-final.txt').write_text(view)
 observations=[]
 for trace in (ROOT/'state/peritus/daemon/product-runs').glob('*.trace'):
  data=trace.read_bytes();offset=0
  while offset<len(data):
   tag=data[offset];length=struct.unpack_from('<Q',data,offset+1)[0];payload=data[offset+9:offset+9+length];offset+=9+length
   if tag in (2,7):
    item=json.loads(payload)
    if item['name']=='web_open':observations.append({'arguments':json.loads(item['arguments']),'is_error':item['is_error'],'output':json.loads(item['output'])})
 assert any(item['arguments']['url'].endswith('/hang') and item['is_error'] for item in observations),observations
 assert any(not item['is_error'] and item['output'].get('method')=='browser' and 'Compass 7821: JavaScript arrived' in item['output'].get('text','') for item in observations),observations
 (BASE/'evidence.json').write_text(json.dumps(observations,indent=2))
 print('Actual wedged-reader error followed by successful fresh browser read',flush=True)
finally:
 if p.isalive():
  p.sendcontrol('q')
  try:p.expect(pexpect.EOF,timeout=8)
  except pexpect.TIMEOUT:p.terminate(force=True)
 p.close();raw.close();server.shutdown();server.server_close();thread.join();print('TUI closed',p.exitstatus,p.signalstatus,flush=True)

 # Stop only this disposable instance, using its exact persisted PID/socket identity.
 instance=ROOT/'state/peritus/daemon/daemon.instance'
 if instance.exists():
  info=dict(line.split('=',1) for line in instance.read_text().splitlines())
  for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines():
   if f"pid={info['pid']}," in line:
    subprocess.run([str(BIN),'--endpoint',line.split()[4],'shutdown','--wait'],check=True,timeout=35)
    break
