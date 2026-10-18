"""Actual native workflow replay: first-party evidence remains valid for stated positions."""
from pathlib import Path
import json,sys,hashlib
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--native',type=Path,required=True)
parser.add_argument('--archive-state',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
exe=args.native.resolve();root=args.output.resolve()
sys.argv=[sys.argv[0],str(exe),str(root)]
prefix=(Path(__file__).resolve().parent.parent/'native-research-rpc.py').read_text().split('for case,unit in enumerate')[0];exec(compile(prefix,'actual-native-consumer','exec'))
archive=next(args.archive_state.resolve().glob('jobs/*/research'))
candidate=next(json.loads(p.read_text())for p in (archive/'candidates').glob('*.json')if json.loads(p.read_text())['category']=='first-party'and json.loads(p.read_text())['question']==1)
pages=[json.loads(p.read_text())for p in (archive/'reads/gather').rglob('*.json')]
page=next(p for p in pages if p['source']==candidate['source'] and p['offset']<=candidate['offset'] and p['text'].encode()[candidate['offset']-p['offset']:candidate['offset']-p['offset']+len(candidate['quote'].encode())]==candidate['quote'].encode())
search=json.loads(next((archive/'searches/assess').glob('*.json')).read_text());recorded_search={'query':search['query'],'results':[{'url':u}for u in search['urls']]}
state=root/'state';state.mkdir();n=Native()
def init():
 r=n.request('initialize',{'protocol_version':1,'plugin_id':'peritus.deep-research','plugin_version':{'major':1,'minor':4,'patch':0},'quotas':{'concurrent_requests':1,'frame_bytes':33554432,'output_bytes':33554432,'invocation_millis':15000,'lifecycle_requests':10000,'protocol_violations':3}});assert r['kind']=='status',r
try:
 init();key=n.call(state,'command',text='What position does the recorded publisher state?',revision=1)['request']['key']
 brief={'question':'What position does the recorded publisher state?','questions':['What position is explicitly stated in the original passage?'],'scope':'Only attributed self-description; do not infer conduct or independent effectiveness.','period':'The exact retained historical source version.','strategy':'Replay actual retained observations; mechanical attribution and workflow only, no new research or HTTP claimed.','output':'A narrow attributed statement and exact evidence.','clarifications':[],'investigation':[{'question':1,'evidence_need':'independent_assessment','criteria':['Attribute the position actually present in the retained passage without validating its empirical truth.']}]}
 saved=tool(n,state,key,'research_plan',brief);assert 'error'not in saved,saved;n.call(state,'model_result',key=key,text='')
 key=n.call(state,'command',text='start '+saved['plan_digest'],revision=2)['request']['key'];observed(n,state,key,page)
 c={**candidate,'question':1};created=tool(n,state,key,'research_candidate',c);assert 'candidate'in created,created
 assert tool(n,state,key,'research_finish',{})['request_complete'];specialist=n.call(state,'model_result',key=key,text='');assert 'controversies' in specialist['request']['key'];skey=specialist['request']['key'];assert set(specialist['request']['tools'])=={'web_search','web_open','web_read','web_find'}
 observed(n,state,skey,page);observed(n,state,skey,recorded_search)
 own=tool(n,state,skey,'research_candidate',{**c,'origin_stage':'gather'});assert 'candidate'in own,own
 data=next(state.glob('jobs/*/research'));assert json.loads((data/f"candidates/{own['candidate']}.json").read_text())['origin_stage']=='controversies'
 key=n.call(state,'model_result',key=skey,text='This narrow declared-position replay does not investigate controversies; no absence claim is made. An actual archived first-party passage tests specialist provenance custody.')['request']['key'];observed(n,state,key,page);observed(n,state,key,recorded_search)
 handoff=tool(n,state,key,'research_inspect',{'kind':'controversies','offset':0,'max_items':32});assert own['candidate']in handoff['handoff']['candidates'] and handoff['handoff']['searches']
 detail=tool(n,state,key,'research_item',{'id':own['candidate']});assert detail['record']['origin_stage']=='controversies'

 a={'candidate':own['candidate'],'decision':'qualified','claim':candidate['claim'],'rationale':'This controlled case establishes only the publisher\'s stated position.','authority':'The subject can state its own position, not independently establish outcomes.','methods':'Exact retained passage; no effectiveness inference.','period':candidate['date'],'independence':'Interested first-party account; no independent corroboration claimed.','scope':'Declared position only.','limitations':'No behavioral or efficacy inference.','provenance':{'relationship':'subject','account':'attributed_statement','basis':'The retained passage identifies the subject\'s own position.','dependencies':[candidate['chain']],'checked_passages':[{'source':candidate['source'],'offset':candidate['offset'],'quote':candidate['quote'],'establishes':'The exact original publisher statement, establishing attribution only, not empirical truth.'}]}}
 admitted=tool(n,state,key,'research_admit',a);assert 'admission'in admitted,admitted;aid=admitted['admission']
 q={'question':1,'assessment':'An attributed position is directly established by its actual source.','answers':[{'criterion':1,'status':'answered','explanation':'The actual source states the position; empirical truth and conduct remain outside this question.','supports':[aid],'contradicts':[],'gap':'','next_action':''}]}
 covered=tool(n,state,key,'research_coverage',{'questions':[q],'conclusion':'Declared-position criterion answered without claiming independent validation.'});assert covered.get('saved'),covered;key=n.call(state,'model_result',key=key,text='')['request']['key']
 report={'controversies':'This narrow replay investigates only the attributed position. Controversies were not investigated; no claim of their absence is made.','title':'Recorded publisher position','synthesis':'According to the recorded publisher: '+candidate['claim']+' [1]','findings':[{'admission':aid,'interpretation':'Only the stated position is established.'}],'questions':[{'question':1,'status':'supported','explanation':'This inquiry asks what is stated, not whether the subject behaves accordingly.'}],'limitations':['No independent empirical claim is established.'],'conclusions':[{'question':1,'claim':'The publisher states this position.','reasoning':'Its exact retained passage establishes what it says; no behavioral inference follows.','supports':[aid],'contradicts':[],'alternatives':[],'confidence':'high','uncertainties':['Truth and implementation are not assessed.']}]}
 saved=tool(n,state,key,'research_report',report);assert 'report_digest'in saved,saved;rid=saved['report_digest'];key=n.call(state,'model_result',key=key,text='')['request']['key'];tool(n,state,key,'research_inspect',{'kind':'report','offset':0,'max_items':32});observed(n,state,key,page)
 checked=tool(n,state,key,'research_check',{'report_digest':rid,'admission':aid,'approved':True,'reason':'Exact attributed position only; controlled workflow readback.','corrections':[]});assert checked['saved'],checked
 reviewed=tool(n,state,key,'research_review',{'report_digest':rid,'approved':True,'reviewed':[aid],'corrections':[],'inquiry_assessment':'The sole criterion asks for declared position and the exact subject passage establishes it. No independent conduct or outcome inference is made.'});assert reviewed.get('approved'),reviewed;reply=n.call(state,'model_result',key=key,text='');assert 'Source-checked report'in reply['text'] and "attribution only"in reply['text'];original=reply['text']
 n.close();n=Native();init();reopened=n.call(state,'command',text='show',revision=3);assert reopened['text']==original
 attachment=n.call(state,'command',text='use-in-design',revision=4);assert attachment['attachment']and attachment['attachment']['path'].endswith(rid+'-checked.md'),attachment
 n.close();n=None
 receipt={'status':'PASS','scope':'Actual native workflow using real archived observations; no live inference/HTTP/semantic deep-research pass claimed.','native_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'qualified_first_party_establishes_original_statement':True,'independent_truth_not_claimed':True,'full_native_stage_handoffs':True,'current_digest_review':True,'fresh_process_report_exact':True,'source_role_visible':True,'checked_design_attachment_preserved':True,'dedicated_specialist_has_real_web_tool_wiring':True,'specialist_candidate_host_owned_origin_and_exact_receipt_readback':True,'durable_specialist_handoff_readable_by_skeptic':True,'authenticated_attribution_can_answer_attribution_criterion_inside_independent_question':True}
 (root/'receipt.json').write_text(json.dumps(receipt,indent=2));print(json.dumps(receipt,indent=2))
finally:
 if n is not None:n.close()
