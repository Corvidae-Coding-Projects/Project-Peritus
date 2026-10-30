import sys,json,pathlib,http.server,threading,argparse
p=argparse.ArgumentParser();p.add_argument('--source',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);args=p.parse_args()
src=args.source.resolve()/'crates/tools/peritus-tools-computer/src';sys.path.insert(0,str(src))
from browser import Browsers
class Site(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  route=self.path; status=200;extra={};body='<title>Independent local article</title><h1>Navigation receipt 7319</h1>'
  if route=='/challenge':
   status=403;extra['cf-mitigated']='challenge';body='<title>Just a moment...</title><script>setTimeout(()=>location.replace("/article"),100)</script>'
  elif route=='/deny':status=403;body='<title>Denied</title><p>Permission denied</p>'
  elif route=='/login':status=401;body='<title>Login</title><p>Sign in</p>'
  elif route=='/rate':status=429;extra['Retry-After']='2';body='<title>Slow down</title><p>Rate limit</p>'
  elif route=='/shell':body='<title>Script app</title><script>setTimeout(()=>document.body.innerHTML="<p>Runtime content 4168</p>",100)</script>'
  elif route=='/challenge200':body='<title>Just a moment...</title><script src="/cdn-cgi/challenge-platform/noop.js"></script><p>Verifying your connection</p>'
  elif route.endswith('noop.js'):
   self.send_response(200);self.send_header('Content-Type','application/javascript');self.end_headers();self.wfile.write(b'void 0;');return
  data=body.encode();self.send_response(status)
  for key,value in extra.items():self.send_header(key,value)
  self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def log_message(self,*a):pass
root=args.output.resolve();root.mkdir(parents=True,exist_ok=True)
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Site);threading.Thread(target=server.serve_forever,daemon=True).start()
b=Browsers(root,root,lambda *a:None);results=[]
try:
 for route,expected in [('/challenge',200),('/deny','forbidden'),('/login','authentication_required'),('/rate','rate_limited'),('/shell',200),('/challenge200','challenge_required')]:
  value=b.read_web({'url':f'http://127.0.0.1:{server.server_port}{route}'})
  actual=value.get('status') if value['success'] else value.get('failure')
  assert actual==expected,(route,value)
  if route=='/challenge':assert 'Navigation receipt 7319' in pathlib.Path(value['path']).read_text()
  if route=='/shell':assert 'Runtime content 4168' in pathlib.Path(value['path']).read_text()
  if route=='/rate':assert value['retry_after']=='2'
  results.append({'route':route,'expected':expected,'actual':actual});print(json.dumps(results[-1]),flush=True)
finally:b.shutdown();server.shutdown()
(root/'results.json').write_text(json.dumps(results,indent=2))
