#!/usr/bin/env python3
"""Create only owned input data and copy actual production binaries for replay."""
import argparse,hashlib,json,shutil
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--binary-dir',type=Path,required=True);p.add_argument('--runtime-mib',type=int,choices=[1,16],default=1);a=p.parse_args()
r=a.root.resolve();source=a.binary_dir.resolve();r.mkdir(mode=0o700,parents=True,exist_ok=False);(r/'bin').mkdir(mode=0o700)
sha={}
for name in ('peritus','peritusd','peritus-tui'):
 shutil.copy2(source/name,r/'bin'/name);sha[name]=hashlib.sha256((r/'bin'/name).read_bytes()).hexdigest()
project=r/'project';(project/'.prescient/postgres').mkdir(parents=True)
(project/'main.py').write_text('print("BASELINE")\n')
(project/'AGENTS.md').write_text('Keep source changes minimal. Runtime state under .prescient/postgres is a running database fixture and is not source. Do not stop its writer. Use real commands, no unit tests, formal verification, Cargo, Clippy or lint gates. Work only in this disposable folder.\n')
with (project/'.prescient/postgres/state').open('wb') as f:
 for _ in range(a.runtime_mib):f.write(b'0'*1048576)
(r/'owned-fixture.json').write_text(json.dumps({'owned_disposable_fixture':True,'runtime_mib':a.runtime_mib,'binary_dir':str(source),'copied_binary_sha256':sha},indent=2))
print(r)
