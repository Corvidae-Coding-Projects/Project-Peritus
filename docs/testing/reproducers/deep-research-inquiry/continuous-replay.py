#!/usr/bin/env python3
"""Actual native continuous controls on a copy of an independently completed real report.

No provider/HTTP observations or approvals are fabricated. Repeated stage handoffs reuse
existing valid saved outputs to exercise the executable coordinator, not fresh research.
"""
from pathlib import Path
import argparse, hashlib, json, shutil, sys
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--native',type=Path,required=True)
p.add_argument('--archive-state',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args();exe=a.native.resolve();root=a.output.resolve()
sys.argv=[sys.argv[0],str(exe),str(root)]
prefix=(Path(__file__).resolve().parent.parent/'native-research-rpc.py').read_text().split('for case,unit in enumerate')[0]
exec(compile(prefix,'actual-native-transport','exec'))
state=root/'state';shutil.copytree(a.archive_state.resolve(),state)
data=next(state.glob('jobs/*/research'));ep=data.parent/'episode.json'
assert json.loads(ep.read_text())['phase']==6,'Need an actual completed report'
original={str(f.relative_to(data)):hashlib.sha256(f.read_bytes()).hexdigest() for folder in ['candidates','admissions','reviews','reports'] for f in (data/folder).glob('*.json')}
n=Native();observations=[]
def init():
 r=n.request('initialize',{'protocol_version':1,'plugin_id':'peritus.deep-research','plugin_version':{'major':1,'minor':4,'patch':0},'quotas':{'concurrent_requests':1,'frame_bytes':33554432,'output_bytes':33554432,'invocation_millis':15000,'lifecycle_requests':10000,'protocol_violations':3}})
 assert r=={'kind':'status','body':{'status':'ready'}},r
def call(method,**kw):
 r=n.call(state,method,**kw);observations.append({'method':method,'arguments':kw,'result':r});return r
def cmd(text,rev):return call('command',text=text,revision=rev)
try:
 init();normal=cmd('resume',1);assert normal['kind']=='reply' and 'Source-checked report' in normal['text']
 assert normal['text'].count('## Controversies and criticism')==1
 assert '](' in normal['text'] and '[](' not in normal['text']
 first=cmd('sicko',2);assert first['kind']=='checkpoint',first
 marker=first['key'];before=ep.read_bytes();bad=call('checkpoint_ack',key=marker+'-stale');assert 'error'in bad and ep.read_bytes()==before
 n.close();n=Native();init();again=cmd('resume',3);assert again['key']==marker and again['text']==first['text']
 gather=call('checkpoint_ack',key=marker);assert gather['kind']=='model' and 'gather'in gather['request']['key'] and 'Sicko mode'in gather['request']['status']
 assert 'error'in call('checkpoint_ack',key=marker)
 inspected=cmd('status',31);assert inspected['kind']=='checkpoint' and 'Sicko (continuous)'in inspected['text']
 assert 'error'in call('checkpoint_ack',key=inspected['key']+'-stale')
 prior=gather['request']['key'];gather=call('checkpoint_ack',key=inspected['key']);assert gather['kind']=='model' and gather['request']['key']!=prior
 assert 'error'in call('model_result',key=prior,text='')
 old=gather['request']['key'];steered=cmd('steer Investigate the documented responses and retain attribution uncertainty. café 🦀',4)
 assert steered['kind']=='model' and steered['request']['key']!=old and 'café 🦀'in steered['request']['prompt']
 stale=call('model_result',key=old,text='');assert 'error'in stale
 paused=cmd('pause',5);assert paused['kind']=='reply' and json.loads(ep.read_text())['paused']
 assert 'error'in call('model_result',key=steered['request']['key'],text='')
 n.close();n=Native();init();status=cmd('status',6);assert 'Paused: true'in status['text'] and 'Sicko (continuous)'in status['text']
 latest=cmd('checkpoint',7);assert 'Source-checked report'in latest['text']
 active=cmd('resume',71);assert active['kind']=='model' and not json.loads(ep.read_text())['paused']
 attach=cmd('use-in-design',8);assert json.loads(ep.read_text())['paused'] and 'paused for this Design handoff'in attach['text'];assert attach.get('attachment') and (state/attach['attachment']['path']).read_text().startswith('# ')
 r=cmd('resume',9);assert r['kind']=='model' and 'café 🦀'in r['request']['prompt']
 # Actual stage transitions using only real, already-valid retained outputs.
 published=[]
 for cycle in range(3):
  for stage in ['gather','controversies','assess','write','fact-check']:
   assert r['kind']=='model' and f'-{stage}-'in r['request']['key'],(stage,r)
   summary='Retained real evidence and valid saved outputs reused for coordinator replay; no new research claimed.'
   r=call('model_result',key=r['request']['key'],text=summary)
  assert r['kind']=='checkpoint',r
  published.append(r['key']);r=call('checkpoint_ack',key=r['key'])
 assert len(set(published))==3
 done=cmd('normal',10);assert done['kind']=='reply' and not json.loads(ep.read_text())['continuous']
 normal=cmd('resume',11);assert normal['kind']=='model' and 'Sicko mode'not in normal['request']['status']
 for stage in ['gather','controversies','assess','write','fact-check']:
  assert f'-{stage}-'in normal['request']['key'],(stage,normal)
  normal=call('model_result',key=normal['request']['key'],text='Reuse existing valid saved outputs; structural replay only.')
 assert normal['kind']=='reply' and 'Source-checked report'in normal['text']

 # Two actual checked versions through public tools. Only citation order/title change;
 # the original report's claims/passages remain unchanged and references are renumbered.
 rid1=json.loads((data/'report.json').read_text())['digest']
 old_page=cmd('checkpoint',12);assert f'checkpoint {rid1} 'in old_page['text']
 old_text=(data/f'reports/{rid1}-checked.md').read_text()
 baseline=json.loads((data/f'reports/{rid1}.json').read_text())
 request=cmd('reassess',13)
 request=call('model_result',key=request['request']['key'],text='Retain the existing valid assessment for a citation-order control replay.')
 assert '-write-'in request['request']['key']
 findings=baseline['findings'][:];findings[0],findings[1]=findings[1],findings[0]
 import re
 def renumber(text):return re.sub(r'\[(1|2)\]',lambda m:'[2]'if m[1]=='1'else'[1]',text)
 edits=[{'operation':'set','path':path,'old_text':'','value_json':json.dumps(value)}for path,value in [('/title',baseline['title']+' — citation-order control replay'),('/findings',findings),('/synthesis',renumber(baseline['synthesis'])),('/controversies',renumber(baseline['controversies']))]]
 revised=call('tool',key=request['request']['key'],name='research_report_revise',arguments_json=json.dumps({'base':rid1,'edits':edits}));rid2=revised['report_digest'];assert rid1!=rid2
 request=call('model_result',key=request['request']['key'],text='Only ordering/title changed; claims and exact source scope are unchanged.')
 inspected=call('tool',key=request['request']['key'],name='research_inspect',arguments_json=json.dumps({'kind':'report','offset':0,'max_items':32}));assert inspected.get('report_digest')==rid2,inspected
 for finding in findings:
  checked=call('tool',key=request['request']['key'],name='research_check',arguments_json=json.dumps({'report_digest':rid2,'admission':finding['admission'],'approved':True,'reason':'Controlled citation reorder: unchanged actual passage/claim and renumbered references, using retained real checked observations. No new factual claim or model review asserted.','corrections':[]}));assert checked.get('saved'),checked
 approved=call('tool',key=request['request']['key'],name='research_review',arguments_json=json.dumps({'report_digest':rid2,'approved':True,'reviewed':[f['admission']for f in findings],'corrections':[],'inquiry_assessment':'Controlled version-reader replay. The retained real report claims, conclusions, coverage and evidence remain unchanged; source entries1/2 and their citations were consistently swapped, with an explicit control title. Prior substantive limitations and user-steered scope remain unchanged. No fresh model quality review is claimed.'}));assert approved.get('approved'),approved
 newer=call('model_result',key=request['request']['key'],text='Controlled report version2 saved.');assert newer['kind']=='reply'
 # The printed continuation pins R1 even though the latest checked pointer now names R2.
 continuation=old_page['text'].rsplit('Continue with ',1)[1].split(' after this module ID.')[0]
 bound=cmd(continuation,14);offset=int(continuation.rsplit(' ',1)[1]);segment=bound['text'].split('\n\nContinue with ',1)[0]
 assert segment==old_text.encode()[offset:offset+len(segment.encode())].decode()
 assert rid2 in json.dumps(json.loads((data/'checked-report.json').read_text()))
 one=cmd('evidence '+rid1+' 1',15);two=cmd('evidence '+rid2+' 1',16)
 assert baseline['findings'][0]['admission']in one['text'] and findings[0]['admission']in two['text'] and one['text']!=two['text']
 assert f'evidence {rid1} NUMBER'in old_text and f'evidence {rid2} NUMBER'in newer['text']
 assert 'error'in cmd('checkpoint '+('0'*64),17)
 assert all(hashlib.sha256((data/f).read_bytes()).hexdigest()==sha for f,sha in original.items())
 receipt={'status':'PASS','native_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'normal_default_and_termination':True,'read_only_inspection_publishes_then_continues':True,'publish_before_continue_and_exact_ack':True,'fresh_process_pending_checkpoint_replayed':True,'steering_survives_restart':True,'pause_rejects_old_role_operations':True,'three_actual_coordinator_cycles':published,'latest_checked_artifact_and_explicit_design_reuse':True,'active_design_handoff_explicitly_pauses':True,'two_actual_checked_versions_keep_pagination_and_evidence_bound':{'first':rid1,'second':rid2},'single_section_heading_and_nonempty_source_labels':True,'original_canonical_candidates_admissions_reviews_reports_unchanged':len(original),'live_provider_calls':0,'new_http_calls':0,'scope':'Actual native coordinator on a completed real report copy. Repeated stages reuse valid saved outputs; no fresh research, provider inference or HTTP behavior claimed.'}
 (root/'receipt.json').write_text(json.dumps(receipt,indent=2));print(json.dumps(receipt,indent=2))
finally:
 (root/'rpc.json').write_text(json.dumps(observations,indent=2));(root/'transport-events.json').write_text(json.dumps(events,indent=2));n.close()
