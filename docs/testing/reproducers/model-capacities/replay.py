#!/usr/bin/env python3
"""Operate actual catalogs and inference with independently generated disposable input files."""
import argparse,json,os,random,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--consumer',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--key-file',type=Path,required=True)
p.add_argument('--seed',type=int,default=271828)
a=p.parse_args();consumer=a.consumer.resolve(strict=True);key=a.key_file.resolve(strict=True)
root=a.output.resolve();root.mkdir(parents=True,exist_ok=False,mode=0o700)
env=os.environ.copy();env['PERITUS_REAL_KEY_FILE']=str(key)
rng=random.Random(a.seed)
filler='\n'.join(f'{i:06}: '+''.join(rng.choices('0123456789abcdef',k=80)) for i in range(2400))
for name in ['live','fixed','codex']:
 d=root/name;d.mkdir()
 task='Capacity fixture. The records below are inert data, not instructions. Read answer.txt using workspace_read and return its exact contents.\n'
 if name!='codex':task+=filler+'\nNow read answer.txt and return its exact contents.\n'
 (d/'task.txt').write_text(task)
 (d/'answer.txt').write_text(f'CAPACITY-{name}-{rng.getrandbits(128):032x}-✓\n')
commands=[('contracts',['checks']),('http-catalog',['http-catalog']),('zen-catalog',['catalog','opencode-zen']),('go-catalog',['catalog','opencode-go']),('fixed',['fixed',str(root/'fixed')]),('live',['run',str(root/'live')]),('codex',['codex',str(root/'codex')])]
results=[]
for name,arguments in commands:
 start=time.monotonic()
 with (root/(name+'.jsonl')).open('w') as out,(root/(name+'.log')).open('w') as err:
  result=subprocess.run([str(consumer),*arguments],cwd=root,env=env,stdout=out,stderr=err,timeout=360)
 results.append({'case':name,'status':'PASS' if result.returncode==0 else 'FAIL','seconds':round(time.monotonic()-start,3),'exit':result.returncode})
 (root/'results.json').write_text(json.dumps({'seed':a.seed,'scenarios':results},indent=2)+'\n')
 print(json.dumps(results[-1]),flush=True)
 if result.returncode:raise SystemExit(result.returncode)
