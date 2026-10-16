#!/usr/bin/env python3
"""Prepare a fresh nongit project and private XDG roots; never launch a provider."""
import argparse,hashlib,json,shutil,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--fixture',required=True,type=Path);p.add_argument('--output',required=True,type=Path);p.add_argument('--binary-dir',required=True,type=Path);a=p.parse_args();source=a.fixture.resolve();root=a.output.resolve();assert(source/'owned-fixture.json').is_file();root.mkdir(parents=True,exist_ok=False)
subprocess.run(['cp','-a','--reflink=auto',str(source/'config'),str(root/'config')],check=True);(root/'cache').mkdir();(root/'state/peritus').mkdir(parents=True)
for name in ('product-state','workspace-registrations'):
 subprocess.run(['cp','-a','--reflink=auto',str(source/'state/peritus'/name),str(root/'state/peritus'/name)],check=True)
shutil.copy2(source/'state/peritus/approval-registry.bin',root/'state/peritus/approval-registry.bin')
for f in(root/'config/peritus').glob('peritus-*.toml'):
 text=f.read_text()
 for name in('config','state','cache'):text=text.replace(str(source/name),str(root/name))
 f.write_text(text)
(root/'bin').mkdir();hashes={}
for name in('peritus','peritusd','peritus-tui'):
 shutil.copy2(a.binary_dir/name,root/'bin'/name);hashes[name]=hashlib.sha256((root/'bin'/name).read_bytes()).hexdigest()
project=root/'project';(project/'sources').mkdir(parents=True)
large='UNICODE-BEGIN\n'+('🦀 café 日本語 e\u0301\n'*128)+'UNICODE-END';assert len(large.encode())>2048
operations=[]
for n in range(33):
 text=large if n==0 else f'KEEP-WORKING-ENTRY-{n:03}'
 operations.append({'id':f'entry:{n+1:032x}','kind':'observation','text':text,'supports':[f'REPLACE_SOURCE_HANDLE_{i:02}'for i in(range(40)if n==0 else[n])],'contradicts':[],'depends_on':[],'status':'open','validity':'task','files':[],'supersedes':None})
for n in range(40):
 text=large if n==0 else(f'KEEP-WORKING-ENTRY-{n:03}'if n<33 else f'KEEP-WORKING-SUPPORT-{n:03}')
 (project/'sources'/f'source-{n:02}.txt').write_text(f'Actual observed source {n:02}:\n{text}\n')
(project/'operations-template.json').write_text(json.dumps({'base_revision':'REPLACE_CURRENT_REVISION','operations':operations},ensure_ascii=False,indent=2))
(project/'AGENTS.md').write_text('This is an owned disposable read-only project used to exercise the actual Peritus memory tools. The requested memory updates are explicitly authorized. Do not change project files, execute shell commands, invoke run_pipeline, or generate tests. Use the genuine host workspace and context tools.\n')
request='''This is a direct Chat task testing actual memory retention and exact retrieval, not a coding task. Do the requested operations; a prose description is insufficient.

1. Use workspace_list to ground this directory, then read operations-template.json and read EACH sources/source-00.txt through sources/source-39.txt using workspace_read. All40 must be real completed host reads with their actual obs source handles. Do not fabricate handles or cite memory read receipts as original sources.
2. Submit ONE context_update containing exactly33 operations from the template. Replace base_revision with the latest current host model revision. Replace supports placeholders with the actual corresponding source handles. Retain every text byte from the template; the first Unicode text is intentionally larger than2048bytes. The first operation must support all40 distinct actual source reads; the other32 cite their corresponding sources. Keep all other template fields exact. If a real revision conflict occurs, read the current revision and retry the same complete atomic33-operation proposal.
3. Call context_read with observation_ids=[], query=null, cursor=null, offset=0, max_bytes=2048. Follow every returned cursor with the same empty handles/query/offset and2048limit until cursor is null. The first entry's40references are intentionally larger than one page; expect entry_json_fragment and concatenate its data by exact UTF8 byte offsets. Do not skip this simply because some memory appeared inline. Read all33 entries fully, including every fragment of the first entry. Do not repeat context_update after acceptance.
4. Finish with KEEP-WORKING-MEMORY-PASS only if the actual update was accepted and exact pagination completed; otherwise report the exact failure. Do not modify project files or run commands, builds, unit tests, verification suites or pipeline work. Do not access another workspace.
'''
(project/'memory_request.md').write_text(request)
expected={'operations':operations,'large_text_bytes':len(large.encode()),'source_count':40,'batch_operations':33,'page_max_bytes':2048};(root/'expected.json').write_text(json.dumps(expected,ensure_ascii=False,indent=2))
manifest={'owned_disposable_fixture':True,'source_fixture':str(source),'project':str(project),'binary_sha256':hashes,'project_files_sha256':{str(f.relative_to(project)):hashlib.sha256(f.read_bytes()).hexdigest()for f in project.rglob('*')if f.is_file()},'provider_dispatch_requires_explicit_driver_flag':True,'copied_daemon_state':False,'contract':'One real33operation atomic context update, Unicode text above2KiB,40actual sourcehandles, exact2048BJSONfragment cursor pagination, no project effects.'};(root/'owned-fixture.json').write_text(json.dumps(manifest,indent=2));print(json.dumps({'root':str(root),'source_files':40,'atomic_operations':33,'large_text_bytes':len(large.encode()),'provider_requests':0,'effects_dispatched':0,'binary_sha256':hashes}))
