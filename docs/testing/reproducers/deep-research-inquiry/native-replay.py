"""Real native IPC replay on copies of actual retained research; no inference claimed."""
from pathlib import Path
import sys,json,shutil,hashlib
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--native',type=Path,required=True)
parser.add_argument('--archive-state',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
exe=args.native.resolve();root=args.output.resolve()
sys.argv=[sys.argv[0],str(exe),str(root)]
prefix=(Path(__file__).resolve().parent.parent/'native-research-rpc.py').read_text().split('for case,unit in enumerate')[0]
exec(compile(prefix,'actual-native-ipc-consumer','exec'))
def init(n):
 r=n.request('initialize',{'protocol_version':1,'plugin_id':'peritus.deep-research','plugin_version':{'major':1,'minor':4,'patch':0},'quotas':{'concurrent_requests':1,'frame_bytes':33554432,'output_bytes':33554432,'invocation_millis':15000,'lifecycle_requests':10000,'protocol_violations':3}})
 assert r['kind']=='status',r
original_state=args.archive_state.resolve()
copied=root/'copied';shutil.copytree(original_state,copied);job=next(copied.glob('jobs/*/episode.json'));data=job.parent/'research'
original_plan=json.loads((data/'approved.json').read_text())['plan_digest'];original_candidates={p.name:hashlib.sha256(p.read_bytes()).hexdigest()for p in (data/'candidates').glob('*.json')}
results=[];n=Native()
try:
 init(n)
 legacy=n.call(copied,'command',text='resume',revision=1);assert legacy['kind']=='reply' and 'earlier' in legacy['text']
 shown=n.call(copied,'command',text='show',revision=2);assert 'LEGACY RESEARCH OUTPUT' in shown['text']
 action=n.call(copied,'command',text='reassess',revision=3);key=action['request']['key'];assert 'assess'in action['request']['status']
 plan_read=tool(n,copied,key,'research_inspect',{'kind':'plan','offset':0,'max_items':32});assert plan_read['approved'] and plan_read['digest']==original_plan
 progress=tool(n,copied,key,'research_inspect',{'kind':'progress','offset':0,'max_items':32});assert len(progress['pending_assessments'])==21 and not progress['coverage_current']
 results.append({'legacy_quality_approval_not_inherited':True,'prior_report_readable':True,'approved_plan_digest_preserved':True,'legacy_assessments_pending':21})
 upgraded={}
 for pointer in sorted((data/'decisions').glob('*.json')):
  old=json.loads((data/'admissions'/f"{json.loads(pointer.read_text())['admission']}.json").read_text())
  c=json.loads((data/'candidates'/f"{old['candidate']}.json").read_text())
  a={**old,'decision':'qualified','rationale':'Controlled provenance/binding replay on real retained observations; not a new factual approval. '+old['rationale'],'provenance':{'relationship':'subject'if c['category']=='first-party'else'unclear','account':'self_report','basis':'Controlled conservative classification for API rejection/recovery; no independent corroboration asserted.','dependencies':['Original evidence chain: '+c['chain']],'checked_passages':[]}}
  r=tool(n,copied,key,'research_admit',a);assert 'error'not in r,r
  upgraded[old['candidate']]={'id':r['admission'],'candidate':c,'assessment':a}
 index_counts={}
 for kind in ['admissions','admission_index']:
  offset=0;entries=[];page_count=0;size=0
  while True:
   page=tool(n,copied,key,'research_inspect',{'kind':kind,'offset':offset,'max_items':32});assert 'error'not in page,page
   page_count+=1;entries+=page['records'];size+=len(json.dumps(page).encode())
   if page['next_offset'] is None:break
   offset=page['next_offset']
  index_counts[kind]={'pages':page_count,'bytes':size,'ids':[v['admission']for v in entries]}
 assert index_counts['admissions']['ids']==index_counts['admission_index']['ids']
 assert index_counts['admission_index']['bytes']<index_counts['admissions']['bytes']/2,index_counts
 assert index_counts['admission_index']['pages']<index_counts['admissions']['pages'],index_counts
 selected=tool(n,copied,key,'research_item',{'id':index_counts['admission_index']['ids'][0]});assert selected['candidate']['quote'] and selected['read_arguments']
 results.append({'compact_index_preserves_all_current_ids_and_selected_exact_detail':True,'inspection_measurement':index_counts})
 first=next(v for v in upgraded.values()if v['candidate']['question']==1 and v['candidate']['category']=='first-party')
 answer={'question':1,'assessment':'Controlled rejection probe: subject self-report alone must not answer an independent inquiry.','answers':[{'criterion':1,'status':'answered','explanation':'This candidate is only the subject account.','supports':[first['id']],'contradicts':[],'gap':'','next_action':''}]}
 refused=tool(n,copied,key,'research_inquiry',answer);assert 'requires independent assessment'in refused['error'],refused
 coverage=tool(n,copied,key,'research_coverage',{'questions':[answer],'conclusion':'Controlled invalid closure, not research success.'});assert 'requires independent assessment'in coverage['error'],coverage
 bad={**first['assessment'],'provenance':{'relationship':'external','account':'observation','basis':'Controlled invalid reclassification probe.','dependencies':[],'checked_passages':[{'source':first['candidate']['source'],'offset':first['candidate']['offset'],'quote':first['candidate']['quote'],'establishes':'This is the actual subject passage, not external corroboration.'}]}}
 refused=tool(n,copied,key,'research_admit',bad);assert 'First-party claims remain'in refused['error'],refused
 answer['answers'][0].update(status='open',next_action='Seek a claim-relevant external observation or corroborating record before judging this criterion.')
 saved=tool(n,copied,key,'research_inquiry',answer);assert saved['saved'] and not saved.get('request_complete',False)
 results.append({'self_report_cannot_close_independent_inquiry':True,'self_report_cannot_close_coverage':True,'external_host_cannot_launder_first_party':True,'evolving_open_inquiry_saved_without_stage_completion':True})
 n.close();n=Native();init(n);key=n.call(copied,'command',text='resume',revision=4)['request']['key']
 reopened=tool(n,copied,key,'research_inspect',{'kind':'inquiry','offset':0,'max_items':32});assert reopened['records'][0]['state']['answers'][0]['status']=='open'
 follow=tool(n,copied,key,'research_followup',{'questions':[1],'reason':'Controlled API routing probe for the saved independent evidence gap; no new factual claim.','leads':[]});assert follow['request_complete'];gather=n.call(copied,'model_result',key=key,text='')['request']['key']
 missing=tool(n,copied,gather,'research_gap',{'question':1,'reason':'Controlled gap lacking observations.','attempts':[]});assert 'actual alternative search'in missing['error'],missing
 false=tool(n,copied,gather,'research_gap',{'question':1,'reason':'Controlled invented-query rejection probe.','attempts':['UNEXECUTED-NATIVE-REPLAY-QUERY']});assert 'actual saved web_search'in false['error'],false
 assert original_candidates=={p.name:hashlib.sha256(p.read_bytes()).hexdigest()for p in (data/'candidates').glob('*.json')}
 results.append({'fresh_native_process_recovers_open_inquiry':True,'followup_routes_material_question_to_gather':True,'uninvestigated_gap_rejected':True,'fabricated_search_attempt_rejected':True,'original_candidates_unchanged':True})
 early=root/'early-rejection';shutil.copytree(original_state,early);e=next(early.glob('jobs/*/episode.json'));state=json.loads(e.read_text());state['phase']=4;e.write_text(json.dumps(state));key=n.call(early,'command',text='resume',revision=8)['request']['key'];report=tool(n,early,key,'research_inspect',{'kind':'report','offset':0,'max_items':32})
 rejected=tool(n,early,key,'research_review',{'report_digest':report['report_digest'],'approved':False,'reviewed':[],'corrections':['Investigate consequential assertions independently; the legacy source checklist does not establish substantive adequacy.'],'inquiry_assessment':'Controlled early-rejection API case grounded in the user-reported legacy quality failure; no unchecked citation is marked approved.'});assert rejected['approved']is False and rejected['request_complete'],rejected
 next_action=n.call(early,'model_result',key=key,text='');assert 'write'in next_action['request']['status']
 results.append({'early_overall_rejection_needs_no_fake_completed_citations':True,'concrete_correction_routes_to_synthesis':True})
 n.close();n=None
 receipt={'status':'PASS','scope':'Actual framed native IPC on copies of real research. Structural integrity/recovery only; no live inference, HTTP or semantic quality pass claimed.','native_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'results':results}
 (root/'receipt.json').write_text(json.dumps(receipt,indent=2));print(json.dumps(receipt,indent=2))
finally:
 if n is not None:n.close()
