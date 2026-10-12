#!/usr/bin/env python3
"""Compile a normal public-API consumer against the already-built Peritus copy."""
import argparse, pathlib, subprocess
p=argparse.ArgumentParser();p.add_argument('--target',required=True,type=pathlib.Path);p.add_argument('--output',required=True,type=pathlib.Path);a=p.parse_args()
source=pathlib.Path(__file__).with_name('replay.rs')
args=['rustc','-C','panic=abort','--edition=2024',str(source),'--crate-name','real_command_replay','-L','dependency='+str(a.target/'deps'),'-o',str(a.output)]
for name in ['peritus_product_runner','peritus_process','peritus_types']:
    libraries=list((a.target/'deps').glob('lib'+name+'-*.rlib'))
    if not libraries:raise SystemExit('Build the Peritus copy first; missing '+name)
    library=max(libraries,key=lambda f:f.stat().st_mtime)
    args.extend(['--extern',name+'='+str(library)])
for directory in (a.target/'build').glob('*/out'):
    if directory.is_dir():args.extend(['-L','native='+str(directory)])
subprocess.run(args,check=True)
