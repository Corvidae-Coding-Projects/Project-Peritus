#!/usr/bin/env python3
"""Linux fault injection into one owned official-client process, never a user process."""
import argparse,json,os,signal,subprocess,time
from pathlib import Path

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args();root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
fault=None
with (root/'consumer.log').open('w') as log:
 p=subprocess.Popen([str(args.binary.resolve()),str(root/'run')],stdout=log,stderr=log)
 try:
  started=time.monotonic()
  while p.poll() is None:
   if fault is None:
    for path in Path('/proc').iterdir():
     if not path.name.isdigit():continue
     try:
      raw=(path/'stat').read_text();stat=raw[raw.rfind(')')+2:].split()
      argv=(path/'cmdline').read_bytes().split(b'\0')
      if int(stat[1])!=p.pid or b'exec'not in argv[1:]:continue
      child=int(path.name);os.kill(child,signal.SIGSTOP)
      fault={'consumer_pid':p.pid,'stopped_official_exec_pid':child,'start_ticks':int(stat[19]),'at_monotonic':time.monotonic(),
       'fault':'SIGSTOP of first owned official Codex exec; real runtime deadline 125 seconds. No auth probe, other request, or user process selected.'}
      (root/'fault.json').write_text(json.dumps(fault,indent=2));print('Injected owned process stall',child,flush=True);break
     except (OSError,ValueError):continue
   if time.monotonic()-started>300:raise TimeoutError('External observation deadline; see retained evidence')
   time.sleep(.1)
  if p.returncode:raise RuntimeError(f'Actual consumer exited {p.returncode}: see {root}/consumer.log')
  receipt=json.loads((root/'run/receipt.json').read_text());assert fault and receipt['status']=='PASS'
  assert not Path('/proc',str(fault['stopped_official_exec_pid'])).exists()
  (root/'receipt.json').write_text(json.dumps({**receipt,'fault':fault,'stopped_process_cleaned':True},indent=2))
  print(json.dumps(receipt,indent=2))
 finally:
  if fault:
   child_path=Path('/proc',str(fault['stopped_official_exec_pid']))
   try:
    raw=(child_path/'stat').read_text();stat=raw[raw.rfind(')')+2:].split()
    if int(stat[19])==fault['start_ticks']:os.kill(int(child_path.name),signal.SIGKILL)
   except (OSError,ValueError):pass
  if p.poll() is None:
   p.terminate()
   try:p.wait(timeout=15)
   except subprocess.TimeoutExpired:p.kill();p.wait(timeout=15)
