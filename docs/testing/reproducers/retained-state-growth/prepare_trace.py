#!/usr/bin/env python3
"""Prepare a complete >1GiB trace using valid metadata JSON plus real whitespace."""
import argparse,hashlib,json,subprocess,sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--fixture',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--host-setup',required=True,type=Path);p.add_argument('--run',required=True);a=p.parse_args();source=a.fixture.resolve();out=a.output.resolve()
if not(source/'owned-fixture.json').is_file():raise SystemExit('Source must be a CLOSED disposable fixture')
record=json.loads((source/'state/peritus/daemon/workbench-v1/runs'/f'{a.run}.json').read_bytes());assert record['phase']==7
view=out.with_name(out.name+'-backup-view');view.mkdir(exist_ok=False);(view/'daemon').symlink_to(source/'state/peritus/daemon',target_is_directory=True);(view/'config').symlink_to(source/'config/peritus',target_is_directory=True)
workspace=out.with_name(out.name+'-workspace');subprocess.run(['cp','-a','--reflink=auto',str(source/'project'),str(workspace)],check=True)
subprocess.run([sys.executable,str(a.host_setup),str(view),str(out),'--run',a.run,'--workspace-copy',str(workspace)],check=True)
trace=out/'replay.trace';original=trace.stat().st_size
metadata={'role':'writer','cycle':1,'previous_profile':'a1000000000000000000000000000001','next_profile':'a1000000000000000000000000000001','reason':'Retained historical metadata fixture; no provider switch or effect is dispatched.'}
prefix=json.dumps(metadata,separators=(',',':')).encode();payload_bytes=(1<<30)+65536
with trace.open('ab')as f:
 f.write(bytes([5])+payload_bytes.to_bytes(8,'little'));f.write(prefix);left=payload_bytes-len(prefix);chunk=b' '*1048576
 while left:n=min(left,len(chunk));f.write(chunk[:n]);left-=n
 f.flush()
# Validate frame boundaries and every trailing byte with bounded memory.
with trace.open('rb')as f:
 f.seek(original);header=f.read(9);assert header[0]==5 and int.from_bytes(header[1:],'little')==payload_bytes
 assert f.read(len(prefix))==prefix;left=payload_bytes-len(prefix)
 while left:
  data=f.read(min(left,1048576));assert data and not data.strip(b' ');left-=len(data)
 assert not f.read(1)
assert trace.stat().st_size>1<<30
manifest={'owned_disposable_fixture':True,'source_fixture':str(source),'run':a.run,'original_trace_bytes':original,'trace_bytes':trace.stat().st_size,'appended_frame_tag':5,'appended_payload_bytes':payload_bytes,'payload_prefix':metadata,'trailing_bytes':'physical JSON whitespace, validated in1MiB chunks; no sparse NUL holes','contract':'Production LocalContextHandle recovery must skip complete legacy metadata without a whole-trace length ceiling; no completed effect can be redispatched.','provider_requests_authorized':False,'effects_authorized':False}
(out/'trace-fixture.json').write_text(json.dumps(manifest,indent=2));print(json.dumps({'output':str(out),'trace_bytes':trace.stat().st_size,'payload_bytes':payload_bytes,'frame_valid':True,'provider_requests':0,'effects_dispatched':0}))
