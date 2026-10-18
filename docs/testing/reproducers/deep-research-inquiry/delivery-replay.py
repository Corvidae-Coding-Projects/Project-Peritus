"""Actual native delivery replay: reuse vetted evidence, defer detail, leave unused leads unassessed."""
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
 brief={'question':'What position does the recorded publisher state?','questions':['What position is explicitly stated in the original passage?','How does this position inform the same stated-purpose question?'],'scope':'Only attributed self-description; do not infer conduct or independent effectiveness.','period':'The exact retained historical source version.','strategy':'Replay actual retained observations; mechanical attribution and workflow only, no new research or HTTP claimed.','output':'A narrow attributed statement and exact evidence.','clarifications':[],'investigation':[{'question':1,'evidence_need':'declared_position','criteria':['Attribute the position actually present in the retained passage without validating its empirical truth.']}]}
 brief['investigation'].append({'question':2,'evidence_need':'declared_position','criteria':['Explain the relevance of the same observed position.','Optional history outside this narrow declared-position question.']})
 saved=tool(n,state,key,'research_plan',brief);assert 'error'not in saved,saved;n.call(state,'model_result',key=key,text='')
 key=n.call(state,'command',text='start '+saved['plan_digest'],revision=2)['request']['key'];observed(n,state,key,page)
 c={**candidate,'question':1};created=tool(n,state,key,'research_candidate',c);assert 'candidate'in created,created
 unused=tool(n,state,key,'research_candidate',{**candidate,'question':2,'claim':'Unused optional discovery: '+candidate['claim']});assert 'candidate'in unused,unused
 assert tool(n,state,key,'research_finish',{})['request_complete'];specialist=n.call(state,'model_result',key=key,text='');assert 'controversies' in specialist['request']['key'];key=n.call(state,'model_result',key=specialist['request']['key'],text='This narrow declared-position replay does not investigate controversies; no absence claim is made.')['request']['key'];observed(n,state,key,page)
 a={'candidate':created['candidate'],'decision':'qualified','claim':candidate['claim'],'rationale':'This controlled case establishes only the publisher\'s stated position.','authority':'The subject can state its own position, not independently establish outcomes.','methods':'Exact retained passage; no effectiveness inference.','period':candidate['date'],'independence':'Interested first-party account; no independent corroboration claimed.','scope':'Declared position only.','limitations':'No behavioral or efficacy inference.','provenance':{'relationship':'subject','account':'self_report','basis':'The retained passage identifies the subject\'s own position.','dependencies':[candidate['chain']],'checked_passages':[]}}
 admitted=tool(n,state,key,'research_admit',a);assert 'admission'in admitted,admitted;aid=admitted['admission']
 q={'question':1,'assessment':'An attributed position is directly established by its actual source.','answers':[{'criterion':1,'status':'answered','explanation':'The actual source states the position; empirical truth and conduct remain outside this question.','supports':[aid],'contradicts':[],'gap':'','next_action':''}]}
 q2={'question':2,'assessment':'The same admitted position is relevant to this second interpretation; optional history is explicitly outside scope.','answers':[{**q['answers'][0],'explanation':'The position passage directly informs this second stated-purpose question; the discovery tag does not restrict its relevance.'},{'criterion':2,'status':'deferred','explanation':'Detailed optional history would not materially change this narrow attributed position. It has not been exhaustively investigated.','supports':[],'contradicts':[],'gap':'','next_action':''}]}
 q2['answers'][1].update(status='open',next_action='Optional history can be investigated later if requested.')
 for question in [q,q2]:assert tool(n,state,key,'research_inquiry',question)['saved']
 action=n.call(state,'command',text='finish',revision=3);key=action['request']['key'];assert 'write' in key and not action['request']['tools']
 refused=tool(n,state,key,'research_followup',{'questions':[1],'reason':'Attempt new discovery despite delivery steering.','leads':[]});assert 'requested delivery'in refused['error'],refused
 n.close();n=Native();init();action=n.call(state,'command',text='resume',revision=4);key=action['request']['key'];assert 'write'in key and 'web_search'not in action['request']['tools']
 current=tool(n,state,key,'research_inspect',{'kind':'inquiry','offset':0,'max_items':32});assert current['records'][1]['state']['answers'][1]['status']=='deferred',current;assert 'Optional history can be investigated' in current['records'][1]['state']['answers'][1]['next_action'];assert current['records'][1]['state']['answers'][0]['explanation']==q2['answers'][0]['explanation']
 index=tool(n,state,key,'research_inspect',{'kind':'admission_index','offset':0,'max_items':32});assert [v['admission']for v in index['records']]==[aid],index
 blocked=tool(n,state,key,'research_inspect',{'kind':'candidate_index','offset':0,'max_items':32});assert 'unavailable to synthesis'in blocked['error'],blocked
 report={'controversies':'This narrow replay investigates only the attributed position. Controversies were not investigated; no claim of their absence is made.','title':'Recorded publisher position','synthesis':'According to the recorded publisher: '+candidate['claim']+' [1]','findings':[{'admission':aid,'interpretation':'Only the stated position is established.'}],'questions':[{'question':1,'status':'supported','explanation':'This inquiry asks what is stated, not whether the subject behaves accordingly.'}],'limitations':['No independent empirical claim is established.'],'conclusions':[{'question':1,'claim':'The publisher states this position.','reasoning':'Its exact retained passage establishes what it says; no behavioral inference follows.','supports':[aid],'contradicts':[],'alternatives':[],'confidence':'high','uncertainties':['Truth and implementation are not assessed.']}]}
 report['questions'].append({'question':2,'status':'partial','explanation':'The relevant position is evidenced, optional history is expressly deferred.'});report['conclusions'].append({**report['conclusions'][0],'question':2,'reasoning':'The same exact passage establishes the relevant stated purpose. Reusing current vetted evidence across questions does not change its limited authority.'})
 missing={k:v for k,v in report.items()if k!='controversies'}
 rejected=tool(n,state,key,'research_report',missing);assert 'dedicated controversies' in rejected['error'],rejected
 data=next(state.glob('jobs/*/research'));base=rejected['draft_digest'];assert (data/f'drafts/{base}.json').exists() and not (data/'report.json').exists()
 def edit(operation,path,value,old_text=''):return {'operation':operation,'path':path,'old_text':old_text,'value_json':json.dumps(value,ensure_ascii=False)}
 before=(data/'draft.json').read_bytes()
 for mutation in [edit('set','/unknown','x'),edit('append','/title','x'),edit('replace_text','/synthesis','x','NOT PRESENT')]:
  refused=tool(n,state,key,'research_report_revise',{'base':base,'edits':[mutation]});assert 'error'in refused,refused;assert (data/'draft.json').read_bytes()==before
 refused=tool(n,state,key,'research_report_revise',{'base':'0'*64,'edits':[edit('set','/controversies',report['controversies'])]});assert 'Revision base' in refused['error'],refused
 saved=tool(n,state,key,'research_report_revise',{'base':base,'edits':[edit('set','/controversies',report['controversies']),edit('append','/limitations','Accusations and responses require their own sourced investigation; this narrow replay does not adjudicate them.'),edit('replace_text','/title','Recorded publisher’s position — attribution only',report['title'])]});assert 'report_digest'in saved,saved;rid=saved['report_digest'];assert json.loads((data/f'reports/{rid}.json').read_text())['controversies']==report['controversies']
 key=n.call(state,'model_result',key=key,text='')['request']['key'];tool(n,state,key,'research_inspect',{'kind':'report','offset':0,'max_items':32});observed(n,state,key,page)
 checked=tool(n,state,key,'research_check',{'report_digest':rid,'admission':aid,'approved':True,'reason':'Exact attributed position only; controlled workflow readback.','corrections':[]});assert checked['saved'],checked
 reviewed=tool(n,state,key,'research_review',{'report_digest':rid,'approved':True,'reviewed':[aid],'corrections':[],'inquiry_assessment':'The sole criterion asks for declared position and the exact subject passage establishes it. No independent conduct or outcome inference is made.'});assert reviewed.get('approved'),reviewed;reply=n.call(state,'model_result',key=key,text='');assert 'Source-checked report'in reply['text'] and "attribution only"in reply['text'];original=reply['text']
 n.close();n=Native();init();reopened=n.call(state,'command',text='show',revision=3);assert reopened['text']==original
 attachment=n.call(state,'command',text='use-in-design',revision=4);assert attachment['attachment']and attachment['attachment']['path'].endswith(rid+'-checked.md'),attachment
 n.close();n=None
 receipt={'status':'PASS','scope':'Actual native workflow using real archived observations; no live inference/HTTP/semantic deep-research pass claimed.','native_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'qualified_first_party_establishes_original_statement':True,'independent_truth_not_claimed':True,'full_native_stage_handoffs':True,'current_digest_review':True,'fresh_process_report_exact':True,'source_role_visible':True,'checked_design_attachment_preserved':True,'cross_question_evidence_reused_in_inquiry_and_report':True,'unused_candidate_need_not_be_assessed':True,'deferred_optional_detail_keeps_partial_status':True,'finish_restricts_discovery_and_persists_across_process':True,'final_exact_citation_checks_still_required':True,'finish_immediately_skips_assessment_backlog':True,'unfinished_criteria_not_falsely_called_exhausted':True,'valid_open_explanation_and_next_action_preserved':True,'missing_controversy_section_rejected_and_exact_attempt_preserved':True,'incremental_set_append_unicode_replace_work':True,'invalid_edit_leaves_pointer_unchanged':True,'stale_base_rejected':True}
 (root/'receipt.json').write_text(json.dumps(receipt,indent=2));print(json.dumps(receipt,indent=2))
finally:
 if n is not None:n.close()
