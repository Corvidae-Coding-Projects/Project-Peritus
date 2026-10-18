#!/usr/bin/env python3
"""Actual native executable IPC consumer; host/model observations are explicit API inputs."""
import hashlib, json, os, pathlib, select, struct, subprocess, sys, time

EXE = pathlib.Path(sys.argv[1] if len(sys.argv)>1 else '/home/doll/Project-Peritus/target/release/peritus-deep-research')
ROOT = pathlib.Path(sys.argv[2] if len(sys.argv)>2 else f'/tmp/peritus-deep-research-rpc-{os.getpid()}')
if ROOT.exists(): raise SystemExit(f'use a fresh directory: {ROOT}')
ROOT.mkdir(parents=True)
checks, failures, events = 0, [], []

def check(label, good):
    global checks
    checks += 1
    if not good: failures.append(label); print('FAIL',label,flush=True)

class Native:
    def __init__(self):
        self.err = open(ROOT/f'plugin-{time.time_ns()}.stderr','wb')
        self.p = subprocess.Popen([str(EXE)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.err)
        self.n = 0
    def exact(self,n):
        output=b''; until=time.monotonic()+15
        while len(output)<n:
            wait=until-time.monotonic()
            if wait<=0 or not select.select([self.p.stdout],[],[],wait)[0]: raise RuntimeError('native reply deadline')
            part=os.read(self.p.stdout.fileno(),n-len(output))
            if not part: raise RuntimeError(f'native exited while reading reply: {self.p.poll()}')
            output+=part
        return output
    def request(self,method,params=None):
        self.n+=1; request_id=f'independent-{self.n}'
        request={'method':method}
        if params is not None: request['params']=params
        value={'protocol_version':1,'request_id':request_id,'request':request}
        data=json.dumps(value,ensure_ascii=False,separators=(',',':')).encode()
        self.p.stdin.write(struct.pack('>I',len(data))+data);self.p.stdin.flush()
        length=struct.unpack('>I',self.exact(4))[0]
        if length>32*1024*1024: raise RuntimeError('oversized native response')
        response=json.loads(self.exact(length))
        if response['request_id']!=request_id or response['protocol_version']!=1: raise RuntimeError('correlation mismatch')
        return response['response']
    def initialize(self,version=1):
        return self.request('initialize',{'protocol_version':1,'plugin_id':'peritus.deep-research','plugin_version':{'major':version,'minor':0,'patch':0},
            'quotas':{'concurrent_requests':1,'frame_bytes':32*1024*1024,'output_bytes':32*1024*1024,'invocation_millis':15000,'lifecycle_requests':10000,'protocol_violations':3}})
    def call(self,state,method,**params):
        response=self.request('invoke',{'capability':'module.workflow','input':{'method':method,'params_json':json.dumps({'state_directory':str(state),**params},ensure_ascii=False)},
            'context':{'session_id':'independent-native-consumer','actor_id':'human-fixture','role':'plugin','granted_capabilities':['module.workflow'],'authority_generation':1,'deadline_millis':15000}})
        if response['kind']!='success': return {'error':response}
        result=json.loads(response['body']['output']['json'])
        events.append({'method':method,'key':params.get('key'),'name':params.get('name'),'result_kind':result.get('kind'),'error':result.get('error')})
        return result
    def close(self):
        if self.p.poll() is None:
            response=self.request('shutdown')
            check('native orderly shutdown ack',response=={'kind':'status','body':{'status':'stopped'}})
            self.p.wait(timeout=5)
            check('native orderly process exit',self.p.returncode==0)
        self.err.close()

def model(action,phase):
    if action.get('kind')!='model': raise RuntimeError(f'expected {phase} agent request; got {action}')
    request=action['request'];check(f'{phase}: status declares phase',phase in request['status'])
    check(f'{phase}: provider role',request['role']==('reviewer' if phase in ('assess','fact-check') else 'writer'))
    if phase in ('plan','write'):check(f'{phase}: no web tools',request['tools']==[])
    return request['key']
def tool(native,state,key,name,args):
    return native.call(state,'tool',key=key,name=name,arguments_json=json.dumps(args,ensure_ascii=False))
def observed(native,state,key,page):
    return native.call(state,'observe',key=key,result_json=json.dumps(page,ensure_ascii=False),is_error=False)
def proposed(source,quote,offset,category='documentation',question=1):
    return {'question':question,'claim':'The source supports this narrow claim only.','relation':'supports','source':source,'offset':offset,'quote':quote,'category':category,
        'publisher':'Named original publisher','date':'2026-10-17','chain':'original-'+source,'limitations':'Explicit controlled host input; no HTTP inference.'}
def assessment(candidate,decision='admitted'):
    return {'candidate':candidate,'decision':decision,'claim':'According to the named publisher, this exact bounded statement holds.','rationale':'Exact passage inspected independently.',
        'authority':'Publisher is responsible only for this narrow statement.','methods':'As observed; other methods remain unknown.','period':'2026-10-17','independence':'One original chain; copied claims add no independent evidence.',
        'scope':'Attributed statement only.','limitations':'No extrapolation from this fixture.'}
def plan(case):
    return {'question':f'Native Rust module case {case}: café 🦀','questions':['What does the original record establish?','What does the publisher claim?'],
        'scope':f'Bounded to retained source passages; revision {case}.','period':'2026-10-17','strategy':'Original source reads, independent skeptic, admitted evidence only, fresh final check.',
        'output':'Qualified report with both questions and explicit limits.','clarifications':[]}

for case,unit in enumerate(['ASCII! ','"\\\n\t','🦀東京 café\n'],1):
    native=Native();state=ROOT/f'case-{case}';state.mkdir()
    try:
        check(f'{case}: invocation before initialize rejected','error' in native.call(state,'command',text='topic',revision=1))
        check(f'{case}: wrong native version handshake rejected',native.initialize(9)['kind']=='failure')
        check(f'{case}: exact native handshake accepted',native.initialize()=={'kind':'status','body':{'status':'ready'}})
        original=model(native.call(state,'command',text=f'Investigate bounded evidence {case}',revision=1),'plan')
        saved=tool(native,state,original,'research_plan',plan(case));old_digest=saved['plan_digest']
        waiting=native.call(state,'model_result',key=original,text='Saved the exact plan.')
        check(f'{case}: waits for explicit Start',waiting['kind']=='reply' and any(a['arguments']=='start '+old_digest for a in waiting['actions']))
        revised=model(native.call(state,'command',text='revise Use the observed population only',revision=2),'plan')
        check(f'{case}: revision gives new role key',revised!=original)
        check(f'{case}: stale role key refused','error' in tool(native,state,original,'research_plan',plan(case+1)))
        saved=tool(native,state,revised,'research_plan',plan(case+10));current_digest=saved['plan_digest']
        native.call(state,'model_result',key=revised,text='Saved revised scope.')
        stale=native.call(state,'command',text='start '+old_digest,revision=3)
        check(f'{case}: stale Start does not gather',stale['kind']=='reply' and 'not started' in stale['text'])
        check(f'{case}: resume before Start still waits',native.call(state,'command',text='resume',revision=4)['kind']=='reply')
        gather=model(native.call(state,'command',text='start '+current_digest,revision=5),'gather')
        source=f'{case:064x}'; raw=(unit*(1048576//len(unit.encode())+1)).encode()[:1048576]
        text=raw.decode('utf-8','ignore');start=57344
        while start<len(text.encode()) and (text.encode()[start]&0xc0)==0x80:start+=1
        end=start+16384
        while end>start and (text.encode()[end]&0xc0)==0x80:end-=1
        quote=text.encode()[start:end].decode();page={'source':source,'url':f'https://example.org/primary/{case}','title':'Controlled maximal host observation','fetched_at_unix':1792200000,
            'offset':101,'total_bytes':len(text.encode())+101,'text':text}
        check(f'{case}: maximal observation persists',observed(native,state,gather,page)=={'recorded':True})
        candidate=proposed(source,quote,start+101)
        wrong={**candidate,'quote':quote+' altered'}
        check(f'{case}: fabricated quotation refused','error' in tool(native,state,gather,'research_candidate',wrong))
        support=tool(native,state,gather,'research_candidate',candidate)['candidate']
        first_source=f'{case+100:064x}';first_quote='Publisher says café 🦀 is available only in its stated scope.'
        first_page={**page,'source':first_source,'text':first_quote,'offset':0,'total_bytes':len(first_quote.encode()),'url':f'https://example.org/publisher/{case}'}
        observed(native,state,gather,first_page)
        first=tool(native,state,gather,'research_candidate',proposed(first_source,first_quote,0,'first-party',2))['candidate']
        blog_source=f'{case+200:064x}';blog_quote='A discovery-only unsupported opinion.'
        blog_page={**first_page,'source':blog_source,'text':blog_quote,'total_bytes':len(blog_quote),'url':f'https://example.org/blog/{case}'}
        observed(native,state,gather,blog_page)
        blog=tool(native,state,gather,'research_candidate',proposed(blog_source,blog_quote,0,'discovery-only'))['candidate']
        assess=model(native.call(state,'model_result',key=gather,text='Retained exact candidates for each question.'),'assess')
        check(f'{case}: gather key cannot mutate assessor','error' in tool(native,state,gather,'research_admit',assessment(support)))
        check(f'{case}: independent assessor read required','error' in tool(native,state,assess,'research_admit',assessment(support)))
        observed(native,state,assess,page);support_admission=tool(native,state,assess,'research_admit',assessment(support))['admission']
        observed(native,state,assess,first_page)
        check(f'{case}: first-party cannot be unqualified','error' in tool(native,state,assess,'research_admit',assessment(first)))
        first_admission=tool(native,state,assess,'research_admit',assessment(first,'qualified'))['admission']
        check(f'{case}: discovery-only cannot be qualified','error' in tool(native,state,assess,'research_admit',assessment(blog,'qualified')))
        rejected=tool(native,state,assess,'research_admit',assessment(blog,'rejected'))['admission']
        write=model(native.call(state,'model_result',key=assess,text='All candidates have explicit decisions.'),'write')
        admitted=tool(native,state,write,'research_inspect',{'kind':'admissions','offset':0,'max_items':32})
        check(f'{case}: synthesis excludes rejected passage',len(admitted['records'])==2 and rejected not in json.dumps(admitted))
        check(f'{case}: synthesis cannot inspect raw candidates','error' in tool(native,state,write,'research_inspect',{'kind':'candidates','offset':0,'max_items':32}))
        references=[support_admission,first_admission]
        report={'title':f'Native report {case}: café 🦀','synthesis':'Attributed, bounded observations only; uncertainty remains.','findings':[{'admission':i,'interpretation':'Interpretation is confined to the scoped admitted statement.'} for i in references],
            'questions':[{'question':1,'status':'supported','explanation':'The exact original passage supports only the scoped finding.'},{'question':2,'status':'supported','explanation':'Attributed publisher statement, not independent proof.'}],
            'limitations':['Controlled API observations; no conclusion beyond this fixture.']}
        incomplete={**report,'questions':report['questions'][:1]}
        check(f'{case}: every plan question required','error' in tool(native,state,write,'research_report',incomplete))
        draft=tool(native,state,write,'research_report',report)['report_digest']
        fact=model(native.call(state,'model_result',key=write,text='Saved the exact report for final checking.'),'fact-check')
        review={'report_digest':draft,'approved':True,'reviewed':references,'corrections':[]}
        check(f'{case}: unread actual draft cannot be approved','error' in tool(native,state,fact,'research_review',review))
        tool(native,state,fact,'research_inspect',{'kind':'report','offset':0,'max_items':32})
        observed(native,state,fact,page)
        check(f'{case}: every reviewer source read required','error' in tool(native,state,fact,'research_review',review))
        observed(native,state,fact,first_page)
        correction={**review,'approved':False,'corrections':['Keep the synthesis explicitly limited to the observed publisher scope.']}
        tool(native,state,fact,'research_review',correction)
        rewrite=model(native.call(state,'model_result',key=fact,text='Concrete correction saved; return this draft to synthesis.'),'write')
        check(f'{case}: corrections create isolated synthesis key',rewrite!=write)
        report['synthesis']='Corrected: these observations are qualified to the named publisher scope only; no generalized conclusion follows.'
        corrected=tool(native,state,rewrite,'research_report',report)['report_digest']
        check(f'{case}: changed report digest',corrected!=draft)
        final=model(native.call(state,'model_result',key=rewrite,text='Substantively corrected scope and saved new report.'),'fact-check')
        check(f'{case}: corrected final-check key isolated',final!=fact)
        check(f'{case}: stale final role key refused','error' in tool(native,state,fact,'research_review',review))
        review['report_digest']=corrected
        check(f'{case}: changed actual report must be inspected','error' in tool(native,state,final,'research_review',review))
        tool(native,state,final,'research_inspect',{'kind':'report','offset':0,'max_items':32})
        observed(native,state,final,page);observed(native,state,final,first_page)
        check(f'{case}: exact final approval accepted',tool(native,state,final,'research_review',review)['approved'] is True)
        complete=native.call(state,'model_result',key=final,text='Checked the actual corrected report against all cited original passages.')
        check(f'{case}: completed report preserves exact Unicode quote',complete['kind']=='reply' and quote.replace('\n','\n> ') in complete['text'] and 'qualified' in complete['text'])
        episode=json.loads((state/'selected.json').read_text())['episode']
        stored=json.loads((state/'jobs'/episode/'research/candidates'/f'{support}.json').read_text())
        check(f'{case}: retained exact quote bytes preserved',stored['quote']==quote and stored['offset']==start+101)
        expected=complete['text'];native.close();native=Native();native.initialize()
        reopened=native.call(state,'command',text='resume',revision=99)
        check(f'{case}: fresh native process durable report',reopened['kind']=='reply' and reopened['text']==expected)
        attachment=native.call(state,'command',text='use-in-design',revision=100)['attachment']
        path=(state/attachment['path']).resolve()
        check(f'{case}: actual Design attachment exact checked report',path.is_relative_to(state.resolve()) and path.is_file() and path.read_text()==expected)
        print('PASS native framed workflow journey',case,flush=True)
    except Exception as error:
        failures.append(f'case-{case}: {error}');print('FAIL native journey',case,repr(error),flush=True)
    finally:
        try:native.close()
        except Exception as error:
            failures.append(f'cleanup-{case}: {error}')
            native.p.kill();native.p.wait();native.err.close()

receipt={'schema':1,'driver':'native-deep-research-framed-v1','native_executable':str(EXE),'sha256':hashlib.sha256(EXE.read_bytes()).hexdigest(),
    'native_workflow_journeys':3,'maximal_observation_shapes':['ascii','escape-heavy','unicode'],'checks':checks,'failures':failures,
    'scope':'Actual native executable framed IPC, durable process restart and module-owned Rust state machine. Host observations/model results explicitly supplied as API inputs; no HTTP/provider/UI execution claimed.','directory':str(ROOT)}
(ROOT/'receipt.json').write_text(json.dumps(receipt,indent=2));(ROOT/'events.json').write_text(json.dumps(events,indent=2))
print(json.dumps(receipt,indent=2));sys.exit(1 if failures else 0)
