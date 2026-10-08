import {readRecord,storeRecordChecked} from '../storage.svelte';
import type {FileDraft} from './drafts.svelte';
import type {Change} from './text';
import type {TextHistory} from './history';
import {draftText,editDraftText,restoreDraftText,textUnits} from './draft_rope';

// These are serialization windows, never draft-length or history admission limits.
const PAGE_UNITS=32_000;
const CONTENT='file-draft-content';
const CHANGES='file-draft-history';
const DELTAS='file-draft-delta';

interface PageRef {kind:'pages';generation:string;pages:number;units:number}
interface DeltaRef {kind:'delta';head:string;units:number}
type TextRef=PageRef|DeltaRef;
interface HistoryRef {head:string|null;count:number}
interface TextPage {key:string;generation:string;index:number;text:string;digest:string}
interface HistoryNode {key:string;id:string;previous:string|null;at:number;removed:PageRef;inserted:PageRef}
interface StoredDelta {key:string;id:string;previous:TextRef;at:number;removed:PageRef;inserted:PageRef;units:number}
interface Delta {previous:Delta|undefined;at:number;removed:string;inserted:string}
interface Mutations {base:string;current:string;tail:Delta|undefined}
type Metadata=Omit<FileDraft,'text'|'original'|'history'>;
export interface StoredDraft {
  schemaVersion:2;key:string;draft:Metadata;text:TextRef;original:TextRef;
  history:{undo:HistoryRef;redo:HistoryRef};
}
export interface LegacyDraft {key:string;draft:FileDraft}
interface RetainedText {text:string;reference:TextRef}
interface Cache {
  text?:RetainedText;original?:RetainedText;
  nodes:WeakMap<Change,Map<Change|undefined,HistoryRef>>;
  bodies:WeakMap<Delta,TextRef>;
}
const caches=new WeakMap<FileDraft,Cache>();
const mutations=new WeakMap<FileDraft,Mutations>();

function cache(draft:FileDraft):Cache {
  let value=caches.get(draft);
  if(!value){value={nodes:new WeakMap(),bodies:new WeakMap()};caches.set(draft,value);}
  return value;
}
function identity(key:string,generation:string,index:number):string {
  return JSON.stringify([key,generation,index]);
}
function whole(value:number):boolean {return Number.isSafeInteger(value)&&value>=0;}
function checkText(value:TextRef):void {
  if(value?.kind==='delta'){
    if(typeof value.head!=='string'||!value.head||!whole(value.units))throw new Error('The retained draft has an invalid edit frontier.');
    return;
  }
  if(value?.kind!=='pages'||typeof value.generation!=='string'||!value.generation||!whole(value.pages)||!whole(value.units)
    ||value.pages!==Math.ceil(value.units/PAGE_UNITS))throw new Error('The retained draft has an invalid content frontier.');
}
async function textDigest(text:string):Promise<string> {
  // Persist exact UTF-16 units, including an intermediate unpaired surrogate.
  const bytes=new Uint8Array(text.length*2);
  for(let at=0;at<text.length;at++){
    const value=text.charCodeAt(at);bytes[at*2]=value&255;bytes[at*2+1]=value>>>8;
  }
  const digest=new Uint8Array(await crypto.subtle.digest('SHA-256',bytes));
  return Array.from(digest,value=>value.toString(16).padStart(2,'0')).join('');
}
async function writeText(key:string,workspace:string,text:string):Promise<PageRef> {
  const generation=crypto.randomUUID(),pages=Math.ceil(text.length/PAGE_UNITS);
  for(let index=0;index<pages;index++){
    const page=text.slice(index*PAGE_UNITS,(index+1)*PAGE_UNITS);
    const value:TextPage={key,generation,index,text:page,digest:await textDigest(page)};
    await storeRecordChecked(CONTENT,identity(key,generation,index),value,workspace);
  }
  return {kind:'pages',generation,pages,units:text.length};
}
async function readPages(key:string,workspace:string,reference:PageRef):Promise<string> {
  if(reference?.kind!=='pages')throw new Error('The retained draft content has an invalid page reference.');
  checkText(reference);const parts:string[]=[];
  for(let index=0;index<reference.pages;index++){
    const value=await readRecord<TextPage>(CONTENT,identity(key,reference.generation,index),workspace);
    const units=Math.min(PAGE_UNITS,reference.units-index*PAGE_UNITS);
    if(!value||value.key!==key||value.generation!==reference.generation||value.index!==index
      ||typeof value.text!=='string'||value.text.length!==units||await textDigest(value.text)!==value.digest)
      throw new Error('The retained draft content failed its exact page check. Its previous recovery data was preserved.');
    parts.push(value.text);
  }
  return parts.join('');
}

async function readText(key:string,workspace:string,reference:TextRef):Promise<string> {
  const pending:StoredDelta[]=[],seen=new Set<string>();let frontier=reference;
  checkText(frontier);
  while(frontier.kind==='delta'){
    if(seen.has(frontier.head))throw new Error('The retained draft edits contain a cycle.');
    seen.add(frontier.head);
    const value=await readRecord<StoredDelta>(DELTAS,identity(key,frontier.head,0),workspace);
    if(!value||value.key!==key||value.id!==frontier.head||!whole(value.at)||value.units!==frontier.units
      ||value.removed?.kind!=='pages'||value.inserted?.kind!=='pages')throw new Error('The retained draft edits are missing exact original input.');
    checkText(value.previous);checkText(value.removed);checkText(value.inserted);
    if(value.at+value.removed.units>value.previous.units
      ||value.units!==value.previous.units-value.removed.units+value.inserted.units)
      throw new Error('The retained draft edit has an inconsistent preimage.');
    pending.push(value);frontier=value.previous;
  }
  let text=draftText(await readPages(key,workspace,frontier));
  for(let index=pending.length-1;index>=0;index--){
    const value=pending[index]!;
    const removed=await readPages(key,workspace,value.removed),inserted=await readPages(key,workspace,value.inserted);
    text=await editDraftText(text,value.at,removed,inserted);
    if(textUnits(text)!==value.units)throw new Error('The retained draft edit has an inconsistent result.');
  }
  return restoreDraftText(text);
}

/** Capture the already computed edit, rather than scanning the complete draft to rediscover it. */
export function noteDraftMutation(draft:FileDraft,before:string,change:Change):void {
  if(before===draft.text)return;
  let retained=mutations.get(draft);
  if(!retained||retained.current!==before){retained={base:before,current:before,tail:undefined};mutations.set(draft,retained);}
  retained.tail={previous:retained.tail,at:change.at,removed:change.removed,inserted:change.inserted};
  retained.current=draft.text;
}
async function writeDraftText(key:string,workspace:string,text:string,captured:Mutations|undefined,retained:Cache):Promise<TextRef> {
  if(retained.text?.text===text)return retained.text.reference;
  if(!captured||captured.current!==text)return writeText(key,workspace,text);
  const pending:Delta[]=[];let node=captured.tail,reference:TextRef|undefined;
  while(node){
    const known=retained.bodies.get(node);if(known){reference=known;break;}
    pending.push(node);node=node.previous;
  }
  reference??=retained.text?.text===captured.base?retained.text.reference
    :retained.original?.text===captured.base?retained.original.reference:await writeText(key,workspace,captured.base);
  for(let index=pending.length-1;index>=0;index--){
    const delta=pending[index]!,id=crypto.randomUUID();
    const removed=await writeText(key,workspace,delta.removed),inserted=await writeText(key,workspace,delta.inserted);
    const units=reference.units-removed.units+inserted.units;
    if(!whole(delta.at)||delta.at+removed.units>reference.units||!whole(units))throw new Error('The draft edit cannot be published with its original preimage.');
    const value:StoredDelta={key,id,previous:reference,at:delta.at,removed,inserted,units};
    await storeRecordChecked(DELTAS,identity(key,id,0),value,workspace);
    reference={kind:'delta',head:id,units};retained.bodies.set(delta,reference);
  }
  if(reference.units!==text.length)throw new Error('The draft edit frontier differs from its retained text.');
  return reference;
}

interface PendingNode {change:Change;previous:Change|undefined;at:number;removed:string;inserted:string;count:number}
interface CapturedHistory {known:HistoryRef;pending:PendingNode[]}
function captureHistory(changes:Change[],retained:Cache):CapturedHistory {
  // Only uncaptured tail changes are traversed. Cursor-only persistence does not
  // snapshot or serialize the entire undo/redo history again.
  const pending:PendingNode[]=[];
  for(let index=changes.length-1;index>=0;index--){
    const change=changes[index]!,previous=changes[index-1];
    const known=retained.nodes.get(change)?.get(previous);
    if(known?.count===index+1)return {known,pending};
    pending.push({change,previous,at:change.at,removed:change.removed,inserted:change.inserted,count:index+1});
  }
  return {known:{head:null,count:0},pending};
}
async function writeHistory(key:string,workspace:string,captured:CapturedHistory,retained:Cache):Promise<HistoryRef> {
  let previous=captured.known;
  for(let index=captured.pending.length-1;index>=0;index--){
    const change=captured.pending[index]!,id=crypto.randomUUID();
    const removed=await writeText(key,workspace,change.removed);
    const inserted=await writeText(key,workspace,change.inserted);
    const value:HistoryNode={key,id,previous:previous.head,at:change.at,removed,inserted};
    await storeRecordChecked(CHANGES,identity(key,id,0),value,workspace);
    previous={head:id,count:change.count};
    let variants=retained.nodes.get(change.change);
    if(!variants){variants=new Map();retained.nodes.set(change.change,variants);}
    variants.set(change.previous,previous);
  }
  return previous;
}
async function readHistory(key:string,workspace:string,reference:HistoryRef,retained:Cache):Promise<Change[]> {
  if(!reference||!whole(reference.count)||!(reference.head===null||typeof reference.head==='string')
    ||(reference.count===0)!==(reference.head===null))throw new Error('The retained draft has an invalid history frontier.');
  const reversed:{id:string;change:Change}[]=[],seen=new Set<string>();
  let head=reference.head;
  while(head!==null){
    if(seen.has(head)||reversed.length>=reference.count)throw new Error('The retained draft history contains an inconsistent chain.');
    seen.add(head);
    const node=await readRecord<HistoryNode>(CHANGES,identity(key,head,0),workspace);
    if(!node||node.key!==key||node.id!==head||!whole(node.at)
      ||!(node.previous===null||typeof node.previous==='string'))throw new Error('The retained draft history is missing its exact original change.');
    const removed=await readPages(key,workspace,node.removed),inserted=await readPages(key,workspace,node.inserted);
    reversed.push({id:head,change:{at:node.at,removed,inserted}});head=node.previous;
  }
  if(reversed.length!==reference.count)throw new Error('The retained draft history is incomplete.');
  const changes:Change[]=[];
  for(let index=reversed.length-1;index>=0;index--){
    const value=reversed[index]!,previous=changes.at(-1);
    retained.nodes.set(value.change,new Map([[previous,{head:value.id,count:changes.length+1}]]));
    changes.push(value.change);
  }
  return changes;
}

/** Publish immutable pages and history first; the small head is the sole adoption point. */
export async function writeDraft(
  key:string,workspace:string,draft:FileDraft,current:()=>boolean,
):Promise<void> {
  const retained=cache(draft),text=draft.text,original=draft.original;
  const mutation=mutations.get(draft),captured=mutation&&{...mutation};
  const undo=captureHistory(draft.history.undo,retained),redo=captureHistory(draft.history.redo,retained);
  const metadata:Metadata={
    revision:draft.revision,bom:draft.bom,newline:draft.newline,editing:draft.editing,saving:draft.saving,
    error:draft.error,owner:workspace,baselineKnown:draft.baselineKnown,bytes:draft.bytes,version:draft.version,
    view:{...draft.view},
  };
  const textRef=await writeDraftText(key,workspace,text,captured,retained);
  if(!current())return;
  retained.text={text,reference:textRef};
  const originalRef=text===original?textRef:retained.original?.text===original
    ?retained.original.reference:await writeText(key,workspace,original);
  if(!current())return;
  retained.original={text:original,reference:originalRef};
  const history={undo:await writeHistory(key,workspace,undo,retained),redo:await writeHistory(key,workspace,redo,retained)};
  if(!current())return;
  const record:StoredDraft={schemaVersion:2,key,draft:metadata,text:textRef,original:originalRef,history};
  await storeRecordChecked('file-draft',key,record,workspace);
  if(mutation&&mutation.tail===captured?.tail&&mutation.current===text){mutation.base=text;mutation.tail=undefined;}
}

/** Legacy heads remain readable; migration adopts no incomplete generation. */
export async function restoreDraftRecord(value:StoredDraft|LegacyDraft,workspace:string):Promise<LegacyDraft> {
  if(!value||typeof value!=='object')throw new Error('The retained draft record is invalid.');
  if(!('schemaVersion' in value))return value;
  if(value.schemaVersion!==2||typeof value.key!=='string'||!value.draft||!value.history)
    throw new Error('The retained draft format is not recognized.');
  const parts=JSON.parse(value.key);
  if(!Array.isArray(parts)||parts.length!==3||parts[0]!==workspace||JSON.stringify(parts)!==value.key
    ||typeof parts[1]!=='string'||typeof parts[2]!=='string'||value.draft.owner!==workspace)
    throw new Error('The retained draft belongs to another workspace.');
  const retained:Cache={nodes:new WeakMap(),bodies:new WeakMap()};
  const text=await readText(value.key,workspace,value.text);
  const same=value.text.kind===value.original.kind&&value.text.units===value.original.units
    &&(value.text.kind==='pages'&&value.original.kind==='pages'
      ?value.text.generation===value.original.generation&&value.text.pages===value.original.pages
      :value.text.kind==='delta'&&value.original.kind==='delta'&&value.text.head===value.original.head);
  const original=same?text:await readText(value.key,workspace,value.original);
  const history:TextHistory={undo:await readHistory(value.key,workspace,value.history.undo,retained),redo:await readHistory(value.key,workspace,value.history.redo,retained)};
  const draft:FileDraft={...value.draft,text,original,history};
  retained.text={text,reference:value.text};retained.original={text:original,reference:value.original};caches.set(draft,retained);
  return {key:value.key,draft};
}

/** Svelte wraps restored objects; retain the same durable references under their new proxies. */
export function adoptRestoredDraft(source:FileDraft,target:FileDraft):void {
  const prior=caches.get(source);if(!prior)return;
  const next:Cache={text:prior.text,original:prior.original,nodes:new WeakMap(),bodies:new WeakMap()};
  for(const direction of ['undo','redo'] as const){
    const previous=source.history[direction],current=target.history[direction];
    for(let index=0;index<previous.length;index++){
      const known=prior.nodes.get(previous[index]!)?.get(previous[index-1]);
      if(known)next.nodes.set(current[index]!,new Map([[current[index-1],known]]));
    }
  }
  caches.set(target,next);
}
