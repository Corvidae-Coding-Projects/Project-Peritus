#!/usr/bin/env python3
"""Copy a CLOSED disposable completed fixture; pad only copied valid run metadata."""
import argparse,hashlib,json,shutil,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--fixture',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--binary-dir',required=True,type=Path);p.add_argument('--run',required=True);a=p.parse_args()
source=a.fixture.resolve();out=a.output.resolve();record_path=source/'state/peritus/daemon/workbench-v1/runs'/f'{a.run}.json'
if not(source/'owned-fixture.json').is_file():raise SystemExit('Source must be an explicitly owned disposable fixture')
record=json.loads(record_path.read_bytes())
if record['phase']!=7:raise SystemExit('Only a CLOSED Completed fixture may be used; never a live run')
project=source/'project';before={str(f.relative_to(project)):hashlib.sha256(f.read_bytes()).hexdigest()for f in project.rglob('*')if f.is_file()}
out.mkdir(parents=True,exist_ok=False)
for directory in ('config','state','cache'):
 subprocess.run(['cp','-a','--reflink=auto',str(source/directory),str(out/directory)],check=True)
# Config generation is immutable: retain exact bytes apart from the three copied private roots.
for config in (out/'config/peritus').glob('peritus-*.toml'):
 text=config.read_text()
 for directory in ('config','state','cache'):text=text.replace(str(source/directory),str(out/directory))
 config.write_text(text)
for stale in (out/'state/peritus/daemon').glob('*.sock'):stale.unlink()
for stale in (out/'state/peritus/daemon').glob('daemon.instance*'):stale.unlink()
(out/'bin').mkdir();hashes={}
for name in ('peritus','peritusd','peritus-tui'):
 shutil.copy2(a.binary_dir/name,out/'bin'/name);hashes[name]=hashlib.sha256((out/'bin'/name).read_bytes()).hexdigest()
target=out/'state/peritus/daemon/workbench-v1/runs'/f'{a.run}.json'
canonical=json.dumps(record,sort_keys=True,separators=(',',':')).encode();logical_sha=hashlib.sha256(canonical).hexdigest()
with target.open('ab')as f:
 for _ in range(128):f.write(b' '*1048576)
 f.write(b'\n')
assert target.stat().st_size>128*1048576
assert hashlib.sha256(json.dumps(json.loads(target.read_bytes()),sort_keys=True,separators=(',',':')).encode()).hexdigest()==logical_sha
manifest={'owned_disposable_fixture':True,'source_fixture':str(source),'project_read_only':str(project),'run':a.run,'workspace':record['workspace_id'],'logical_record_sha256':logical_sha,'padded_record_bytes':target.stat().st_size,'source_project_sha256':before,'binary_sha256':hashes,'contract':'Exact logically unchanged Completed run remains available through daemon runs show and normal TUI reattachment after restart; padding alone cannot quarantine valid user state.','provider_requests_authorized':False,'effects_authorized':False}
(out/'fixture.json').write_text(json.dumps(manifest,indent=2));print(json.dumps({'output':str(out),'record_bytes':target.stat().st_size,'logical_record_sha256':logical_sha,'project_files':len(before),'copied_binary_sha256':hashes}))
