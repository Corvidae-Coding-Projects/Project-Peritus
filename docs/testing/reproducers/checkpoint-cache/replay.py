#!/usr/bin/env python3
"""Real ScopedBaseline consumer: expected state derives only from fixture edits here."""
import argparse,json,os,random,shutil,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--consumer',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--require-reduced-reads',action='store_true');a=p.parse_args()
b=a.output.resolve();b.mkdir(parents=True,exist_ok=False);root=b/'workspace';root.mkdir();state=b/'private';state.mkdir();journal=state/'scope.jsonl';results=[];expected=set();rng=random.Random(8419)
runtime=root/'.prescient/runtime';runtime.mkdir(parents=True)
for i in range(256):
    # Bounded 64 MiB payload, independent deterministic bytes. Ordinary filesystem data.
    (runtime/f'data-{i:04}.bin').write_bytes(rng.randbytes(256*1024))
source=root/'source';source.mkdir();(source/'alpha.txt').write_text('original-alpha\n');(source/'beta.txt').write_text('original-beta!\n');(source/'mode.txt').write_text('unchanged-mode\n');(source/'typed.txt').write_text('original-type\n')
class Consumer:
    def __init__(self):
        self.p=subprocess.Popen([str(a.consumer.resolve()),str(root),str(journal)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1)
        assert json.loads(self.p.stdout.readline())['ready']
    def call(self,op,**fields):
        self.p.stdin.write(json.dumps({'op':op,**fields})+'\n');self.p.stdin.flush();r=json.loads(self.p.stdout.readline());assert r.get('ok'),r;return r
    def close(self):
        self.p.stdin.close();self.p.wait(timeout=10);assert self.p.returncode==0,self.p.stderr.read()
def check(c,name):
    r=c.call('observe');actual=set(r['value']['paths']);assert actual==expected,(name,sorted(expected),sorted(actual));results.append({'case':name,**r});return r
c=Consumer()
try:
    c.call('enroll',path='.prescient');c.call('enroll',path='source');check(c,'initial-baseline')
    check(c,'warm-cache')
    unchanged=c.call('observe',repeat=8);assert unchanged['value']['paths']==[]
    results.append({'case':'eight-unchanged-observations',**unchanged})
    path=source/'alpha.txt';metadata=path.stat();path.write_text('modified-alpha\n');os.utime(path,ns=(metadata.st_atime_ns,metadata.st_mtime_ns));expected.add('source/alpha.txt');check(c,'same-size-same-mtime-external-edit')
    r=c.call('diff');assert '-original-alpha' in r['value']['diff'] and '+modified-alpha' in r['value']['diff'];results.append({'case':'exact-diff-before-and-after','ok':True})
    (source/'beta.next').write_text('replaced-beta!\n');os.replace(source/'beta.next',source/'beta.txt');expected.add('source/beta.txt');check(c,'atomic-file-replacement')
    before=(runtime/'data-0200.bin').stat();data=bytearray((runtime/'data-0200.bin').read_bytes());data[-1]^=0x80;(runtime/'data-0200.bin').write_bytes(data);os.utime(runtime/'data-0200.bin',ns=(before.st_atime_ns,before.st_mtime_ns));expected.add('.prescient/runtime/data-0200.bin');check(c,'large-file-last-byte-same-size-mtime')
    one_changed=c.call('observe',repeat=8);assert set(one_changed['value']['paths'])==expected;results.append({'case':'eight-stable-changed-observations',**one_changed})
    (source/'nested').mkdir();(source/'nested/added.txt').write_text('new descendant\n');expected.update(['source/nested','source/nested/added.txt']);check(c,'directory-and-file-addition')
    (source/'beta.txt').unlink();check(c,'baseline-file-removal')
    shutil.rmtree(source/'nested');expected.difference_update(['source/nested','source/nested/added.txt']);check(c,'new-subtree-removal')
    (source/'mode.txt').chmod(0o600);expected.add('source/mode.txt');check(c,'permission-only-change')
    typed=source/'typed.txt';typed.unlink();typed.mkdir();(typed/'child.txt').write_text('new directory descendant\n');expected.update(['source/typed.txt','source/typed.txt/child.txt']);check(c,'file-to-directory-kind-change')
    shutil.rmtree(typed);outside=b/'outside';outside.mkdir();(outside/'must-not-read.txt').write_text('outside-private-content\n');typed.symlink_to(outside,target_is_directory=True);expected.remove('source/typed.txt/child.txt');check(c,'directory-to-symlink-does-not-follow-target')
    typed.unlink();typed.write_text('original-type\n');expected.remove('source/typed.txt');check(c,'restored-original-content-clears-change')
    c.close();c=Consumer();check(c,'fresh-process-durable-baseline')
    parallel=c.call('parallel',readers=4);assert all(set(paths)==expected for paths in parallel['value']['results']);results.append({'case':'four-concurrent-observers',**parallel})
    assert c.call('wrong-root')['value']['rejected'];results.append({'case':'workspace-isolation','ok':True})
    # Structured generated action sequences: modified/read/restored/read; varied UTF-8/space names and sizes.
    for i,name in enumerate(['space file.txt','unicode-λ.txt','empty.txt','nested-second.txt']):
        pth=source/name;body=rng.randbytes([0,1,4096,16385][i]);pth.write_bytes(body);expected.add(str(pth.relative_to(root)));check(c,f'generated-add-{i}')
        pth.unlink();expected.remove(str(pth.relative_to(root)));check(c,f'generated-remove-{i}')
    read_limit=64*1024*1024
    reduced=unchanged['logical_read_bytes']<read_limit and one_changed['logical_read_bytes']<read_limit
    summary={'seed':8419,'cases':len(results),'payload_bytes':64*1024*1024,'unchanged_batch_read_bytes':unchanged['logical_read_bytes'],'stable_changed_batch_read_bytes':one_changed['logical_read_bytes'],'dramatically_reduced_reads':reduced,'results':results}
    (b/'result.json').write_text(json.dumps(summary,indent=2));print(json.dumps({k:v for k,v in summary.items() if k!='results'},indent=2))
    if a.require_reduced_reads: assert reduced,'8 unchanged observations must read less than one entire 64 MiB payload'
finally:
    if c.p.poll() is None:c.close()
