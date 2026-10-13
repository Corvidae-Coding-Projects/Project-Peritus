#!/usr/bin/env python3
"""Operate real packaged-binary discovery against symlinked command paths.

The consumer links production peritus-launcher. All version probes execute real
Peritus binaries. This portable filesystem replay does not emulate macOS.
"""
import argparse, hashlib, json, os, shutil, subprocess
from pathlib import Path

p=argparse.ArgumentParser()
p.add_argument('--consumer',type=Path,required=True)
p.add_argument('--cli',type=Path,required=True)
p.add_argument('--daemon',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args();root=a.output.resolve();root.mkdir(parents=True,exist_ok=True)
package=root/'Library/Application Support/Peritus/bin'
package.mkdir(parents=True,exist_ok=True)
app=package/'peritus';daemon=package/'peritusd'
shutil.copy2(a.cli,app);shutil.copy2(a.daemon,daemon)
commands=root/'.local/bin';commands.mkdir(parents=True,exist_ok=True)
results=[]

def run(name,application,good=True):
 r=subprocess.run([str(a.consumer.resolve()),str(application)],capture_output=True,text=True,timeout=10)
 expected=[str(app),str(daemon)]
 passed=r.returncode==0 and r.stdout.splitlines()==expected if good else r.returncode!=0
 results.append({'case':name,'input':str(application),'expected':expected if good else 'reject unavailable, invalid or mismatched package','observed':r.stdout,'error':r.stderr,'exit':r.returncode,'pass':passed})

run('direct',app)
for i,label in enumerate(['peritus','peritus with spaces','peritus-café','peritus-漢字','peritus-$literal','peritus-quote\'name']):
 alias=commands/label
 alias.symlink_to(app if i%2==0 else Path('../../Library/Application Support/Peritus/bin/peritus'))
 run('command-alias-'+str(i),alias)
chain=commands/'chained';chain.symlink_to(commands/'peritus')
run('two-link-chain',chain)
chain2=commands/'long-chain';chain2.symlink_to(Path('chained'))
run('three-link-chain',chain2)
directory=root/'linked-package';directory.symlink_to(package,target_is_directory=True)
run('symlinked-package-directory',directory/'peritus')
# A real wrong-role binary next to the public shortcut must not override the
# genuine packaged daemon next to the shortcut's actual executable target.
shutil.copy2(a.cli,commands/'peritusd')
run('ignore-command-directory-daemon',commands/'peritus')
missing=commands/'dangling';missing.symlink_to(root/'not-present')
run('dangling-application',missing,False)
loop=commands/'loop';loop.symlink_to(loop)
run('symlink-cycle',loop,False)
run('directory-application',package,False)
run('missing-application',root/'missing',False)
saved=package/'peritusd.saved';daemon.rename(saved)
try:
 run('missing-real-daemon',commands/'peritus',False)
 shutil.copy2(a.cli,daemon)
 run('wrong-role-daemon',commands/'peritus',False)
 daemon.unlink();shutil.copy2(saved,daemon);daemon.chmod(0o600)
 run('non-executable-daemon',commands/'peritus',False)
 daemon.unlink();daemon.mkdir()
 run('directory-daemon',commands/'peritus',False)
 daemon.rmdir()
finally:
 if daemon.exists() and daemon.is_file():daemon.unlink()
 if not daemon.exists():saved.rename(daemon)
run('recovery-after-rejection',commands/'peritus')
report={'cli_sha256':hashlib.sha256(a.cli.read_bytes()).hexdigest(),'daemon_sha256':hashlib.sha256(a.daemon.read_bytes()).hexdigest(),'consumer_sha256':hashlib.sha256(a.consumer.read_bytes()).hexdigest(),'results':results}
(root/'results.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'cases':len(results),'passed':sum(r['pass'] for r in results),'failures':[r['case'] for r in results if not r['pass']]}))
if not all(r['pass'] for r in results):raise SystemExit(1)
