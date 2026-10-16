#!/usr/bin/env python3
"""Owned stable binary page + source inputs; copy actual sibling production executables."""
import argparse,hashlib,json,shutil
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--binary-dir',type=Path,required=True);a=p.parse_args()
r=a.root.resolve();r.mkdir(mode=0o700,parents=True,exist_ok=False);(r/'bin').mkdir(mode=0o700);sha={}
for name in ('peritus','peritusd','peritus-tui'):
 shutil.copy2(a.binary_dir/name,r/'bin'/name);sha[name]=hashlib.sha256((r/'bin'/name).read_bytes()).hexdigest()
project=r/'project';(project/'.prescient').mkdir(parents=True)
(project/'main.py').write_text('print("BASELINE")\n')
(project/'pyproject.toml').write_text('[project]\nname = "binary-evidence-fixture"\nversion = "0.0.0"\nrequires-python = ">=3.11"\n')
(project/'.prescient/page.bin').write_bytes(bytes(8192))
(project/'AGENTS.md').write_text('This disposable fixture has one CLI source file and one owned binary data page. The requested edit must preserve the binary nature of page.bin; NUL bytes are intentional. Use only actual CLI/file operations for testing, no unit suites, mocks, Clippy, lint or formal verification. Complete the normal Peritus pipeline through independent review for this small two-file request. Do not initialize Git or add packages.\n')
(r/'owned-fixture.json').write_text(json.dumps({'owned_disposable_fixture':True,'kind':'stable-nul-page-source-pipeline','copied_binary_sha256':sha},indent=2));print(r)
