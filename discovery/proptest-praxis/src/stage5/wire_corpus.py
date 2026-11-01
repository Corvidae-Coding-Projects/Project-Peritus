"""Freeze checked-in compatibility bytes and independently assembled additive grammar seeds.

This preparation script never imports or executes a production encoder/decoder. Runtime checking
uses only the frozen JSON. Supplemental seeds express documented wire fields as ordinary bytes.
"""
from pathlib import Path
import hashlib,json,re,struct,tomllib

ROOT=Path(__file__).resolve().parents[4]
HERE=Path(__file__).resolve().parent
def u16(n):return struct.pack('>H',n)
def u32(n):return struct.pack('>I',n)
def u64(n):return struct.pack('>Q',n)
def ident(n):return bytes([n])*16
def opaque(b):return u32(len(b))+b
def text(s):return opaque(s.encode())
def frame(family,payload):return b'PRTS'+u16(1)+u16(family)+u16(1)+u16(0)+u32(len(payload))+payload

rows=[];raw={}
for path in sorted((ROOT/'compat/app-protocol/v1').glob('*/fixture.toml')):
 manifest=tomllib.loads(path.read_text());expected=tomllib.loads((path.parent/'expectation.toml').read_text())
 for item in manifest['files']:
  assert hashlib.sha256((path.parent/item['path']).read_bytes()).hexdigest()==item['sha256']
 payload=bytes.fromhex((path.parent/'payload.hex').read_text());raw[path.parent.name]=payload
 rows.append(dict(name=path.parent.name,hex=payload.hex(),accepted=expected['accepted'],family=expected.get('expected_family'),error=expected.get('expected_error'),origin=str(path.parent.relative_to(ROOT)),manifest_sha256=hashlib.sha256(path.read_bytes()).hexdigest()))

# Freeze reviewed outer discriminant assignments. Dynamic legacy forms share their typed variant.
table={}
for family,file,enum in [(96,'request','AppRequestPayload'),(97,'response','AppResponsePayload')]:
 source=(ROOT/f'crates/app/peritus-app-protocol/src/wire/{file}.rs').read_text()
 start=source.index('fn payload_tag(');end=source.index('\nimpl CanonicalDecode',start)
 for name,tag in re.findall(enum+r'::(\w+)\([^\n]*?\)\s*=>\s*(\d+)',source[start:end]):table[(family,int(tag))]=name
table[(96,12)]='DaemonStatus'
for tag in [44,46]:table[(96,tag)]='ContinueWorkbenchExecution'
for tag in [25,27,61]:table[(96,tag)]='UpdateModels'
for tag in [15,17]:table[(97,tag)]='Interaction'
for tag,name in enumerate(['DomainEvent','SubscriptionGap','Backpressure','ArtifactMetadata','ArtifactChunk','ArtifactComplete','PromptRequested','TerminalOutput','TerminalExited','ReadinessChanged','Diagnostic','Heartbeat','ShutdownProgress','ShutdownComplete','TerminalUnavailable'],1):table[(98,tag)]=name
for tag,name in enumerate(['Acknowledge','CancelSubscription','CancelArtifact','CancelPrompt','CancelTerminal','Subscription','HeartbeatReply'],1):table[(99,tag)]=name

context=raw['minimal-daemon-status-request'][16:52]
request=raw['minimal-daemon-status-request'][16:84]
correlation=request[-16:];request_id=request[-32:-16]
def mk(family,tag,body,name=None):
 prefix=request if family in [96,97] else context+(correlation if family==99 else b'')
 payload=frame(family,prefix+u16(tag)+body)
 rows.append(dict(name=name or f'constructed-{family}-{tag}',hex=payload.hex(),accepted=True,family=family,error=None,origin='independent scalar grammar seed',manifest_sha256=None))
def body(name):
 b=raw[name];family=int.from_bytes(b[6:8],'big');return b[{96:86,97:86,98:54,99:70}[family]:]
revision=ident(31)+ident(32)+ident(33)+u64(1)+u64(2)+ident(34)+ident(35)
prompt=request_id+ident(44)+context[-16:]+ident(45)+revision+bytes([46])*32+u64(1)
terminal=ident(51)+ident(52)+request_id
cancel_art=ident(53)+ident(54)+correlation
metadata=body('realistic-artifact-event')
chunk=ident(53)+ident(54)+u64(1)+u64(0)+opaque(b'\x00native\xff')
completion=ident(53)+ident(54)+u64(8)+bytes([55])*32
shutdown=request_id+correlation
query=body('minimal-workbench-query')
review_query=query+ident(63)+u64(1)+u32(0)
diff_query=review_query[:-4]+u32(0)*3
diff_bytes_query=review_query[:-4]+bytes([61])*32+bytes([62])*32+u32(0)+u32(256)
for tag,payload in {
 3:ident(53)+ident(54),4:cancel_art,5:prompt+bytes([2,1])+text('é native'),6:prompt+correlation,
 7:terminal,8:terminal+opaque(b'\x00paste\xff'),9:terminal+u16(81)+u16(25),10:terminal+correlation,11:terminal+correlation,
 13:shutdown,14:metadata,15:chunk,16:completion,18:ident(56)+u16(1),23:body('minimal-interaction-binding-query'),
 61:ident(56)+ident(57)+ident(58)+ident(59)+bytes([0])+b''.join(text(model)+bytes([1]) for model in ['writer-native','reviewer-native','fixer-native']),
 105:diff_query,107:diff_bytes_query,109:review_query,180:ident(60)+u16(1),
}.items():mk(96,tag,payload)

error=u16(5)+bytes([1,1,0])
operation=u16(1)+u16(1)+text('native-run')+text('known')+text('')+bytes(7)
snapshot=ident(63)+ident(64)+ident(57)+ident(58)+ident(59)+u16(1)+u32(0)+b''.join(text(s) for s in ['task','queued','','','',''])+operation+bytes([0])
model_choices=b''.join(text(s)+bytes([1])+u16(2) for s in ['writer-native','reviewer-native','fixer-native'])
interaction=bytes([0])+snapshot+u16(1)+model_choices+u64(0)+u64(0)+u32(0)
anchor=ident(63)+query[16:32]+bytes([61])*32+bytes([62])*32+bytes([65])*32+bytes([66])*32+bytes([67])*32+text('src/native.rs')+u16(1)+u32(0)*4
diff_page=diff_query+bytes([61])*32+bytes([62])*32+anchor+bytes([0])+u16(0)+u32(1)+u64(0)+u64(0)+bytes([0])
summary=review_query+bytes([61])*32+bytes([62])*32+u32(0)+u64(0)+u64(0)+bytes([0])+u16(0)+u32(0)+u16(0)
for tag,payload in {1:request_id+bytes([1])+u64(3)+u64(5),2:ident(68)+u64(0)+u32(2),3:metadata,4:ident(44),5:terminal,6:request_id,8:shutdown,9:error,10:snapshot,17:interaction,106:diff_page,108:diff_bytes_query+u32(3)+opaque(b'abc'),110:summary,180:ident(60)+u16(0)}.items():mk(97,tag,payload)
event_frame=frame(3,b'opaque-native-event')
for tag,payload in {1:ident(68)+ident(69)+u64(3)+ident(70)+u32(1)+opaque(event_frame),2:ident(68)+u64(0)+u64(2)+u64(3),3:ident(68)+u64(2)+u64(1)+u32(1),5:chunk,6:completion,9:terminal+u64(3)+u64(9)+bytes([1])+struct.pack('>i',-17),10:body('realistic-daemon-response'),12:ident(71)+u64(7)+body('realistic-daemon-response'),14:shutdown+bytes([1])+u32(0)}.items():mk(98,tag,payload)
for tag,payload in {1:ident(68)+u64(2),2:ident(68)+correlation+bytes([1]),3:cancel_art,4:prompt+correlation,5:terminal+correlation,6:bytes([1])+ident(68)+bytes([2])}.items():mk(99,tag,payload)
mk(99,6,bytes([2])+ident(68),'constructed-subscription-resume')
command_prefix=body('minimal-workbench-create')[:56]
mk(96,29,command_prefix+u16(13)+u16(1)+ident(72)+bytes([73])*32,'constructed-brief-proposal-acceptance')
mk(96,29,command_prefix+u16(14)+u16(3)+u32(2)+u16(3)+bytes([1])+u16(1),'constructed-context-preference')

intent_source=(ROOT/'crates/app/peritus-app-protocol/src/wire/workbench/intent.rs').read_text().split('pub(super) fn write')[0]
intents={int(tag):name for name,tag in re.findall(r'WorkbenchIntent::(\w+)(?:\([^\n]*\)|\s*\{[^\n]*\})\s*=>\s*(\d+)',intent_source)}

offsets={96:84,97:84,98:52,99:68}
for row in rows:
 b=bytes.fromhex(row['hex']);f=row['family'];tag=int.from_bytes(b[offsets[f]:offsets[f]+2],'big') if f in offsets and row['accepted'] else 0
 row['tag']=tag;row['variant']=table.get((f,tag),'ClientHello' if f==94 else 'ServerHello' if f==95 else '')
 if row['accepted']:assert row['variant'],row['name']
 row['intent_tag']=int.from_bytes(b[142:144],'big') if row['accepted'] and f==96 and tag in [29,31] else None
 row['intent_variant']=intents.get(row['intent_tag'])
seen={(row['family'],row['tag']) for row in rows if row['accepted']}
missing=sorted(set(table)-seen)
assert not missing,missing
assert set(intents)<=set(row['intent_tag'] for row in rows)
(HERE/'wire_cases.json').write_text(json.dumps(rows,indent=2,sort_keys=True)+'\n')
(HERE/'wire_tags.json').write_text(json.dumps([dict(family=f,tag=t,variant=n) for (f,t),n in sorted(table.items())],indent=2,sort_keys=True)+'\n')
labels=['use peritus_app_protocol::{AppRequestPayload,AppResponsePayload,AppEventPayload,ControlPayload,WorkbenchIntent};']
for enum,fn,file in [('AppRequestPayload','request','envelope/request.rs'),('AppResponsePayload','response','envelope/response.rs'),('AppEventPayload','event','envelope/event.rs'),('ControlPayload','control','envelope/control.rs'),('WorkbenchIntent','intent','workbench/intent.rs')]:
 source=(ROOT/f'crates/app/peritus-app-protocol/src/{file}').read_text();start=source.index(f'pub enum {enum}');begin=source.index('{',start);depth=1;end=begin+1
 while depth:
  if source[end]=='{':depth+=1
  elif source[end]=='}':depth-=1
  end+=1
 variants=re.findall(r'^    ([A-Z]\w*)(\(|\s*\{|,)',source[begin+1:end-1],re.M)
 labels.append(f'pub(super) fn {fn}(value: &{enum})->&\'static str {{ match value {{')
 for name,shape in variants:
  pattern='(..)' if shape=='(' else '{..}' if '{' in shape else ''
  labels.append(f'{enum}::{name}{pattern}=>"{name}",')
 labels.append('}}')
(HERE/'wire_labels.rs').write_text('\n'.join(labels)+'\n')
print(json.dumps({'retained_cases':len(raw),'all_cases':len(rows),'wire_tags':len(table),'workbench_intents':len(intents),'missing_tags':missing}))
