#!/usr/bin/env python3
"""Deterministic timing exploration of the same actual TUI workflow."""
import argparse,json,pathlib,subprocess,sys
p=argparse.ArgumentParser();p.add_argument('--bin',type=pathlib.Path,required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
a.output.mkdir(mode=0o700,parents=True)
r=[]
for name,extra in [('original',[]),('delay6',['--cases','delayed','--pause-seconds','6']),('delay11',['--cases','delayed','--pause-seconds','11']),('delay29',['--cases','delayed','--pause-seconds','29'])]:
 args=[sys.executable,str(pathlib.Path(__file__).with_name('replay.py')),'--bin',str(a.bin),'--output',str(a.output/name)]+extra
 code=subprocess.run(args).returncode
 record={'name':name,'command':args,'exit_code':code,'results':json.loads((a.output/name/'summary.json').read_text())}
 r.append(record);print(json.dumps({'name':name,'exit_code':code}),flush=True)
(a.output/'campaign.json').write_text(json.dumps(r,indent=2)+'\n')
raise SystemExit(0 if all(x['exit_code']==0 for x in r) else 1)
