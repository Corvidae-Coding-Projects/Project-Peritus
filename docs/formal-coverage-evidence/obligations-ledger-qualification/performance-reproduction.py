from pathlib import Path
import shutil,subprocess,os,hashlib,json,re
root=Path('/tmp/peritus-parent-obligations-proof.6ib_z8mo');rel=Path('crates/orchestration/peritus-obligations');active=root/rel
backup=Path('/tmp/peritus-parent-obligations-live-before-perf03');assert not backup.exists()
targetroot=Path('/home/doll/Project-Peritus/.worktrees/formal-coverage/target')
variants=[('foundation',Path('/tmp/peritus-parent-obligations-foundations-source')/rel,Path('/tmp/peritus-parent-obligations-foundations-full-package.sha256')),('candidate325',Path('/tmp/peritus-sol-obligations-ledger-source-01')/rel,Path('/tmp/peritus-sol-obligations-ledger-source-final.sha256')),('fixed356',Path('/tmp/peritus-sol-obligations-performance-source-01')/rel,Path('/tmp/peritus-sol-obligations-performance-full-package.sha256'))]
harness=Path('/tmp/peritus-parent-obligations-scaling-probe.rs').read_text().replace('[64,128,256]','[256,512]')
assert '[256,512]' in harness
saved={str(p.relative_to(active)):hashlib.sha256(p.read_bytes()).hexdigest() for p in active.rglob('*') if p.is_file()}
active.rename(backup)
results=[]
try:
 for name,source,manifest in variants:
  assert source.is_dir(),str(source)
  for line in manifest.read_text().splitlines():
   h,n=line.split(maxsplit=1);p=source/n
   if not p.exists() and n.startswith(str(rel)):p=source/Path(n).relative_to(rel)
   assert p.exists() and hashlib.sha256(p.read_bytes()).hexdigest()==h,n
  if active.exists():shutil.rmtree(active)
  shutil.copytree(source,active,copy_function=shutil.copyfile)
  (active/'tests/review_scaling_probe.rs').write_text(harness)
  target=targetroot/('formal-obligations-scaling-parent-v3-'+name);assert not target.exists(),str(target)
  env=os.environ.copy();env.update(CARGO_BUILD_JOBS='2',CCACHE_DISABLE='1',CARGO_TARGET_DIR=str(target))
  command=['cargo','test','--release','--package','peritus-obligations','--locked','--test','review_scaling_probe','--','--nocapture']
  log=Path('/tmp/peritus-parent-obligations-scaling-independent3-'+name+'.log')
  with log.open('w') as out:run=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=subprocess.STDOUT,timeout=240)
  measurements=[l for l in log.read_text().splitlines() if l.startswith('SCALING')]
  record={'name':name,'source':str(source),'manifest':str(manifest),'manifest_sha256':hashlib.sha256(manifest.read_bytes()).hexdigest(),'command':command,'cwd':str(root),'target':str(target),'log':str(log),'exit_code':run.returncode,'measurements':measurements};results.append(record);print(json.dumps(record),flush=True)
  assert run.returncode==0
finally:
 if active.exists():shutil.rmtree(active)
 backup.rename(active)
 assert saved=={str(p.relative_to(active)):hashlib.sha256(p.read_bytes()).hexdigest() for p in active.rglob('*') if p.is_file()}
 Path('/tmp/peritus-parent-obligations-scaling-independent3.json').write_text(json.dumps({'results':results,'restored_original_package_hashes':True,'fixture_sha256':hashlib.sha256(harness.encode()).hexdigest(),'fixture':'original parent late-failure two-branch fixture,3 repeats,256/512; fresh per-variant Cargo targets'},indent=2)+'\n')
 print('Restored original parent obligations package and all saved hashes',flush=True)
