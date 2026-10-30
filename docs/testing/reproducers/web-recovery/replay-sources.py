#!/usr/bin/env python3
"""Operate the real web-source library consumer against bounded independent HTTP inputs."""
import argparse, hashlib, http.server, json, pathlib, random, subprocess, threading, time
p=argparse.ArgumentParser();p.add_argument('--consumer',required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
rng=random.Random(7319);requests=[]
class Site(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  requests.append((self.server.server_port,self.path));status=200;headers={};media='text/html; charset=utf-8'
  if self.path=='/deny':status=403;body='<p>Access denied</p>'
  elif self.path=='/login':status=401;body='<p>Sign in</p>'
  elif self.path=='/challenge':body='<title>Just a moment...</title><script src="/cdn-cgi/challenge-platform/x"></script>'
  elif self.path=='/shell':body='<title>Script only</title><script>document.body.textContent="loaded"</script>'
  elif self.path=='/rate':status=429;headers['Retry-After']='3';body='<p>Slow down</p>'
  elif self.path=='/challenge-rate':status=429;headers['cf-mitigated']='challenge';headers['Retry-After']='1';body='<title>Just a moment...</title>'
  elif self.path=='/api-throttle':status=400;media='application/json';body=json.dumps({'error_name':'throttle_violation','backoff':2,'error_message':'Wait before this API method'})
  elif self.path=='/pdf':media='application/pdf';body='%PDF-1.7 test resource, not an HTML page'
  else:
   label=self.path.strip('/');body=f'<title>Source {label}</title><head><base href="/references/"></head><body><h1>Start {label}</h1><pre>  code μ λ 🦉\n    retained indentation</pre><a href="manual">Manual</a><p>'+('αβ🙂 readable text\n'*2400)+f'</p><p>Tail {label}</p><script>hidden_script_6921</script></body>'
  data=body.encode();self.send_response(status);self.send_header('Content-Type',media);self.send_header('Content-Length',str(len(data)))
  for k,v in headers.items():self.send_header(k,v)
  self.end_headers();self.wfile.write(data)
 def log_message(self,*args):pass
servers=[http.server.ThreadingHTTPServer(('127.0.0.1',0),Site) for _ in range(3)]
for s in servers:threading.Thread(target=s.serve_forever,daemon=True).start()
proc=subprocess.Popen([a.consumer,str(a.output/'sources')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
records=[]
def call(v):
 proc.stdin.write(json.dumps(v)+'\n');proc.stdin.flush();line=proc.stdout.readline();assert line,'Consumer stopped';return json.loads(line)
def url(path,i=0):return f'http://127.0.0.1:{servers[i].server_port}{path}'
try:
 for i in range(8):
  label=f'case-{rng.randrange(100000,999999)}';v=call({'url':url('/'+label),'max_bytes':128})
  assert v['title']=='Source '+label,v
  chunks=[v['text']];n=v['next_offset']
  while n is not None:
   page=call({'action':'read','source':v['source'],'offset':n,'max_bytes':rng.choice([127,257,1031])});chunks.append(page['text']);n=page['next_offset']
  text=''.join(chunks)
  assert f'Start {label}' in text and f'Tail {label}' in text and '  code μ λ 🦉\n    retained indentation' in text
  assert 'hidden_script_6921' not in text
  assert len(text.encode())==v['total_bytes'] and hashlib.sha256(text.encode()).hexdigest()==v['text_sha256']
  assert any(link['url']==url('/references/manual') for link in v['links'])
  found=call({'action':'find','source':v['source'],'query':'Tail '+label});assert found['matches'][0]['offset']>32768,found
  cached=call({'url':url('/'+label)});assert cached['cached'] and cached['source']==v['source']
  # Independently reopen the same immutable source through a fresh process.
  fresh=subprocess.run([a.consumer,str(a.output/'sources')],input=json.dumps({'action':'read','source':v['source'],'offset':0,'max_bytes':128})+'\n',text=True,capture_output=True,check=True)
  assert json.loads(fresh.stdout)['text']==v['text']
  records.append({'case':label,'bytes':len(text.encode()),'paging_digest_and_restart':'pass'})
 for path,kind in [('/deny','forbidden'),('/login','authentication_required'),('/challenge','challenge_required'),('/shell','needs_rendering')]:
  v=call({'url':url(path)});assert v.get('success') is False and v['failure']==kind,v;records.append({'case':path,'failure':kind})
 v=call({'url':url('/pdf')});assert v['success'] is False and 'unsupported page media' in v['error'];records.append({'case':'/pdf','unsupported_media':'pass'})
 for path,seconds,i in [('/rate',3,1),('/api-throttle',2,2)]:
  before=len(requests);v=call({'url':url(path,i)});assert v.get('failure')=='rate_limited',v
  v=call({'url':url('/should-not-fetch',i)});assert v.get('failure')=='rate_limited' and len(requests)==before+1,v
  healthy=call({'url':url('/unrelated-'+str(i))});assert healthy.get('http_status')==200
  time.sleep(seconds+.15)
  healthy=call({'url':url('/after-cooldown',i)});assert healthy.get('http_status')==200,healthy
  records.append({'case':path,'cooldown_requests':1,'unrelated_origin':'allowed','expiry':'pass'})
 v=call({'url':url('/challenge-rate',1)});assert v.get('failure')=='rate_limited',v;records.append({'case':'/challenge-rate','rate_limit_over_challenge':'pass'})
 (a.output/'results.json').write_text(json.dumps({'seed':7319,'cases':records,'network_requests':len(requests)},indent=2));print(json.dumps({'cases':len(records),'network_requests':len(requests),'result':'pass'}))
finally:
 proc.stdin.close();proc.wait(timeout=5)
 for s in servers:s.shutdown();s.server_close()
