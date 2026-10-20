#!/usr/bin/env python3
"""Use the real IPC reader across external commits; never trust a cached corrupt state row."""
import argparse,json,os,sqlite3,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('base',type=Path);a=p.parse_args();b=a.base.resolve();root=b/'candidate-live';info=json.loads((root/'instance.json').read_text());env=os.environ.copy();env.update({f'XDG_{n.upper()}_HOME':str(root/n)for n in ('config','state','cache')});cli=b/'candidate-bin/peritus';daemon=b/'candidate-bin/peritusd';subprocess.run([str(cli),'--endpoint',info['endpoint'],'shutdown','--wait'],check=Path(info['endpoint']).exists(),capture_output=True,timeout=30)
log=(root/'invalidation-daemon.log').open('w');proc=subprocess.Popen([str(daemon),'serve','--config',info['config']],stdout=log,stderr=subprocess.STDOUT,env=env);results=[]
try:
 end=time.monotonic()+15
 while time.monotonic()<end:
  if proc.poll()is not None:raise RuntimeError('owned daemon exited')
  endpoint=next((line.split()[4]for line in subprocess.check_output(['ss','-xlpH'],text=True).splitlines()if f'pid={proc.pid},'in line),None)
  if endpoint:break
  time.sleep(.02)
 else:raise RuntimeError('owned daemon not ready')
 database=root/'state/peritus/daemon/workbench-v1/control.sqlite3';c=sqlite3.connect(database);key,value=c.execute('SELECT record_key,value FROM state_records WHERE namespace=3402 LIMIT 1').fetchone();conversation=bytes(key).hex()
 def probe(label):
  out=subprocess.run([str(b/'target/debug/invalidation-probe'),endpoint,info['workspace'],conversation,label],capture_output=True,text=True,timeout=20);assert out.returncode==0,(label,out.stderr);v=json.loads(out.stdout);results.append(v);return v
 probe('cold');probe('hit')
 c.execute('CREATE TABLE disposable_cache_observation(id INTEGER)');c.commit();probe('external-commit')
 c.execute('DROP TABLE disposable_cache_observation');c.commit();bad=bytearray(value);bad[-1]^=1;c.execute('UPDATE state_records SET value=? WHERE namespace=3402 AND record_key=?',(bytes(bad),key));c.commit();probe('corrupt')
 c.execute('UPDATE state_records SET value=? WHERE namespace=3402 AND record_key=?',(value,key));c.commit();probe('repaired');c.close()
 (root/'invalidation-results.json').write_text(json.dumps(results,indent=2));print(json.dumps(results))
finally:
 if proc.poll()is None:
  out=subprocess.run([str(cli),'--endpoint',endpoint,'shutdown','--wait'],capture_output=True,text=True,timeout=30);assert out.returncode==0,out.stderr;proc.wait(timeout=10)
 log.close()
