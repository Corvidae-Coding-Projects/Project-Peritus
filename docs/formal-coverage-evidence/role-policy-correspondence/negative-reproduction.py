from pathlib import Path
import subprocess,os,json,hashlib
root=Path('/tmp/peritus-parent-obligations-proof.6ib_z8mo')
p=root/'crates/orchestration/peritus-role/src/context_policy/tables.rs'
original=p.read_bytes();source=original.decode();a=source.index('fn reviewer_policy()');b=source.index('fn fixer_policy()',a);part=source[a:b]
needle='            ContextClass::AgentProgress,\n        ]),';assert part.count(needle)==2
part=part.replace(needle,'            ContextClass::AgentProgress,\n            ContextClass::HiddenReasoning,\n        ]),',1)
mutated=(source[:a]+part+source[b:]).encode()
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='2',CCACHE_DISABLE='1',CARGO_TARGET_DIR='/home/doll/Project-Peritus/.worktrees/formal-coverage/target/formal-obligations-parent')
cmd=['cargo','verus','verify','--package','peritus-role','--all-features','--locked','--check-toolchain','--fwd-verus-args-to','roots','--','--no-cheating','--rlimit','20']
result={'mutation':'actual reviewer visible table admits HiddenReasoning','path':str(p),'original_sha256':hashlib.sha256(original).hexdigest(),'mutated_sha256':hashlib.sha256(mutated).hexdigest(),'command':cmd,'cwd':str(root)}
try:
 p.write_bytes(mutated)
 log=Path('/tmp/peritus-parent-role-policy-negative-hidden-reasoning2.log')
 with log.open('w') as f:run=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 result.update(exit_code=run.returncode,log=str(log));print(json.dumps(result),flush=True)
finally:
 p.write_bytes(original)
 assert p.read_bytes()==original
 for line in Path('/tmp/peritus-parent-role-policy-full-package-02.sha256').read_text().splitlines():
  h,n=line.split(maxsplit=1);assert hashlib.sha256((root/'crates/orchestration/peritus-role'/n).read_bytes()).hexdigest()==h,n
 result['restored_all17_source_hashes']=True
 log=Path('/tmp/peritus-parent-role-policy-verus14-restored.log')
 with log.open('w') as f:restored=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 result.update(restored_exit_code=restored.returncode,restored_log=str(log))
 Path('/tmp/peritus-parent-role-policy-negative-hidden-reasoning2.json').write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result),flush=True)
assert result['exit_code']!=0 and result['restored_exit_code']==0
