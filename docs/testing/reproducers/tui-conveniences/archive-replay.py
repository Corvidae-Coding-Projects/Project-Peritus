#!/usr/bin/env python3
"""Build a real consumer of the production transcript module and reopen its disk data."""
import argparse,json,os,subprocess,sys
from pathlib import Path
here=Path(__file__).resolve().parent
sys.path.insert(0,str(here.parent/'checkpoint-cache'))
from cohort import select
p=argparse.ArgumentParser();p.add_argument('--deps',required=True,type=Path);p.add_argument('--output',required=True,type=Path);a=p.parse_args();a.output.mkdir(mode=0o700,parents=True,exist_ok=False)
command=['rustc','--edition=2024',str(here/'archive-consumer.rs'),'-L','dependency='+str(a.deps),'-o',str(a.output/'consumer')]
# Anchor to the TUI cohort; the daemon also links an older nix without uid support.
fingerprints=a.deps.parent/'.fingerprint';index={};roots=[]
for metadata in fingerprints.glob('*/lib-*.json'):
 try:
  stamp=int.from_bytes(bytes.fromhex(metadata.with_suffix('').read_text().strip()),'little');data=json.loads(metadata.read_text())
 except (OSError,ValueError):continue
 name=metadata.stem.removeprefix('lib-');rlib=a.deps/('lib'+name+'-'+metadata.parent.name.rsplit('-',1)[1]+'.rlib')
 if rlib.exists():
  index[(name,stamp)]=(data,rlib)
  if name=='peritus_tui':roots.append((metadata.stat().st_mtime,data))
selected={};pending=[max(roots,key=lambda root:root[0])[1]]
while pending:
 for _,name,_,stamp in pending.pop()['deps']:
  item=index.get((name,stamp))
  if item and name not in selected:selected[name]=item[1];pending.append(item[0])
for name in ['peritus_app_protocol','sha2','regex','nix','tokio','libc']:command+=['--extern',name+'='+str(selected[name])]
subprocess.run(command,check=True)
(a.output/'empty-path').mkdir();results=[]
for name,mode,cache,missing in [('rg-seed','seed','cache-rg',False),('rg-restart','restart','cache-rg',False),('fallback-restart','restart','cache-rg',True),('fallback-seed','seed','cache-fallback',True),('fallback-second-restart','restart','cache-fallback',True)]:
 env=dict(os.environ,XDG_CACHE_HOME=str((a.output/cache).resolve()))
 if missing:env['PATH']=str((a.output/'empty-path').resolve())
 run=subprocess.run([str((a.output/'consumer').resolve()),mode],env=env,capture_output=True,text=True,timeout=20)
 results.append({'name':name,'exit_code':run.returncode,'stdout':run.stdout,'stderr':run.stderr})
 (a.output/'results.json').write_text(json.dumps({'compiler':command,'scenarios':results},indent=2))
 if run.returncode:raise SystemExit(run.stderr or run.stdout)
print(json.dumps({'passed':True,'process_runs':len(results),'output':str(a.output)}))
