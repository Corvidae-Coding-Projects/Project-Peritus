import {indexText,normalizeFileText,textFormat,type TextFormat,type TextSnapshot} from './text';
import {createTextHistory,type TextHistory} from './history';
import {browserStorage,removeRecord,readRecord,visitRecords,reportStorageFailure} from '../storage.svelte';
import {adoptRestoredDraft,noteDraftMutation,restoreDraftRecord,writeDraft,type StoredDraft,type LegacyDraft} from './draft_store';
import type {Change} from './text';
import type {PendingOperation} from '../operations.svelte';

export interface TextEditorView {
  selectionStart:number; selectionEnd:number; selectionDirection:'forward'|'backward'|'none';
  windowStart?:number;
  scrollTop:number; scrollLeft:number; wrap:boolean; searching:boolean;
  needle:string; replacement:string; matchCase:boolean; searchNote:string; goLine:number;
}
export interface FileDraft {
  text:string; original:string; revision:string; bom:boolean; newline:string;
  editing:boolean; saving:boolean; error:string; history:TextHistory; view:TextEditorView;
  owner?:string; baselineKnown?:boolean; bytes?:number; version:number;
}
export const fileDrafts=$state<Record<string,FileDraft>>({});
export const fileKey=(project:string,path:string)=>JSON.stringify([browserStorage.workspace,project,path]);
let restoring:Promise<void>|undefined,restoredWorkspace='';
const pendingDraftWrites=new Map<string,{timer:ReturnType<typeof setTimeout>;project:string;path:string;draft:FileDraft;workspace:string}>();
type DraftWrite={draft:FileDraft;workspace:string};
const draftWriters=new Map<string,{pending:DraftWrite|undefined;running:boolean;epoch:number}>();
const DRAFT_SETTLE_MS=250;
const changeListeners=new WeakMap<FileDraft,Set<(before:string,change:Change)=>void>>();
export async function restoreFileDrafts():Promise<void> {
  const workspace=browserStorage.workspace;
  if(restoredWorkspace===workspace&&restoring)return restoring;
  restoredWorkspace=workspace;
  restoring=(async()=>{
    const complete=await visitRecords<StoredDraft|LegacyDraft>(
      'file-draft',async record=>{restoreDraft(await restoreDraftRecord(record,workspace),workspace);},workspace,
    );
    if(!complete&&workspace===browserStorage.workspace){restoredWorkspace='';restoring=undefined;}
  })();
  return restoring;
}
export async function restoreFileDraft(project:string,path:string):Promise<void>{
  const workspace=browserStorage.workspace,key=fileKey(project,path);
  if(fileDrafts[key])return;
  const record=await readRecord<StoredDraft|LegacyDraft>('file-draft',key,workspace);
  if(record&&record.key===key){
    try{restoreDraft(await restoreDraftRecord(record,workspace),workspace);}
    catch(error){reportStorageFailure(error);throw error;}
  }
}
function restoreDraft(record:{key:string;draft:FileDraft},workspace:string){
  if(workspace!==browserStorage.workspace||typeof record?.key!=='string'
    ||typeof record.draft?.text!=='string'||typeof record.draft.original!=='string'
    ||typeof record.draft.revision!=='string'||!ownedDraftKey(record.key,workspace))return;
  if(!fileDrafts[record.key]){
    fileDrafts[record.key]={
      ...record.draft,owner:workspace,saving:false,version:Number.isSafeInteger(record.draft.version)?record.draft.version:0,
      history:validHistory(record.draft.history)?record.draft.history:createTextHistory(),view:restoreEditorView(record.draft.view),
    };
    adoptRestoredDraft(record.draft,fileDrafts[record.key]!);
  }
}
function ownedDraftKey(key:string,workspace:string):boolean{
  try{
    const parts=JSON.parse(key);
    return Array.isArray(parts)&&parts.length===3&&parts[0]===workspace
      &&typeof parts[1]==='string'&&typeof parts[2]==='string'&&JSON.stringify(parts)===key;
  }catch{return false;}
}
export function persistFileDraft(project:string,path:string,draft:FileDraft) {
  if(draft.owner!==undefined&&draft.owner!==browserStorage.workspace)return;
  const key=fileKey(project,path),workspace=browserStorage.workspace,previous=pendingDraftWrites.get(key);
  if(previous)clearTimeout(previous.timer);
  const pending={project,path,draft,workspace,timer:setTimeout(()=>writeFileDraft(key),DRAFT_SETTLE_MS)};
  pendingDraftWrites.set(key,pending);
}
export function flushFileDraft(project:string,path:string) {
  const key=fileKey(project,path),pending=pendingDraftWrites.get(key);
  if(!pending)return;
  clearTimeout(pending.timer);writeFileDraft(key);
}
export function flushFileDrafts() {
  for(const [key,pending] of pendingDraftWrites){clearTimeout(pending.timer);writeFileDraft(key);}
}
export function touchFileDraft(draft:FileDraft) {
  draft.version=Number.isSafeInteger(draft.version)?draft.version+1:1;
}
export function noteFileDraftChange(draft:FileDraft,before:string,change:Change):void {
  noteDraftMutation(draft,before,change);
  for(const listener of changeListeners.get(draft)??[])listener(before,change);
}
export function observeFileDraftChanges(draft:FileDraft,listener:(before:string,change:Change)=>void):()=>void {
  let listeners=changeListeners.get(draft);
  if(!listeners){listeners=new Set();changeListeners.set(draft,listeners);}
  listeners.add(listener);
  return()=>{listeners!.delete(listener);if(!listeners!.size)changeListeners.delete(draft);};
}
export function discardFileDraft(project:string,path:string) {
  const key=fileKey(project,path),pending=pendingDraftWrites.get(key);
  if(pending){clearTimeout(pending.timer);pendingDraftWrites.delete(key);}
  const writer=draftWriters.get(key);if(writer){writer.epoch++;writer.pending=undefined;}
  delete fileDrafts[key];void removeRecord('file-draft',key);
}
export function retainFile(project:string,path:string,value:TextSnapshot) {
  const format=textFormat(value.text);
  const text=value.text.replace(/^\ufeff/,'').replaceAll('\r\n','\n');
  return retainPreparedFile(project,path,value,format,text);
}
export async function retainFileIncrementally(project:string,path:string,value:TextSnapshot,format:TextFormat,signal?:AbortSignal) {
  const workspace=browserStorage.workspace;
  const existing=fileDrafts[fileKey(project,path)];if(existing)return existing;
  const text=await normalizeFileText(value.text,signal);
  if(workspace!==browserStorage.workspace){const error=new Error('The file workspace changed during preparation');error.name='AbortError';throw error;}
  return retainPreparedFile(project,path,value,format,text);
}
export function dirtyFile(project:string,path:string) {
  const draft=fileDrafts[fileKey(project,path)];return !!draft&&(draft.baselineKnown===false||draft.text!==draft.original);
}
export function fileBytes(draft:FileDraft) {
  return (draft.bom?'\ufeff':'')+(draft.newline==='crlf'?draft.text.replaceAll('\n','\r\n'):draft.text);
}
export function forgetFile(project:string,path:string) {
  const key=fileKey(project,path),draft=fileDrafts[key];
  if(draft?.saving)return false;
  if(dirtyFile(project,path)&&!confirm(`Discard unsaved changes to ${path}?`))return false;
  discardFileDraft(project,path);return true;
}
export function protectFileDrafts(event:BeforeUnloadEvent) {
  if(Object.values(fileDrafts).some(draft=>draft.saving||draft.baselineKnown===false||draft.text!==draft.original)) {
    event.preventDefault();event.returnValue='';
  }
}

function validHistory(history:TextHistory|undefined):history is TextHistory {
  if(!history||!Array.isArray(history.undo)||!Array.isArray(history.redo))return false;
  return history.undo.every(validChange)&&history.redo.every(validChange);
}
export async function settleFileSave(operation:PendingOperation,result:Record<string,unknown>):Promise<void> {
  if(result.error||typeof result.revision!=='string'||!operation.project||!operation.path||typeof operation.payload?.text!=='string')return;
  const workspace=browserStorage.workspace;
  if(operation.workspace!==undefined&&operation.workspace!==workspace)return;
  await restoreFileDraft(operation.project,operation.path);
  if(workspace!==browserStorage.workspace)return;
  const key=fileKey(operation.project,operation.path);
  const submitted=operation.payload.text;
  const [normalized,indexed]=await Promise.all([normalizeFileText(submitted),indexText(submitted)]);
  if(workspace!==browserStorage.workspace)return;
  const draft=fileDrafts[key]??retainPreparedFile(operation.project,operation.path,{text:submitted,revision:result.revision,bytes:typeof result.bytes==='number'?result.bytes:submitted.length},indexed.format,normalized);
  if(draft.revision!==operation.payload.revision&&draft.revision!==result.revision)return;
  draft.original=normalized;
  draft.revision=result.revision;draft.baselineKnown=true;draft.saving=false;draft.error='';
  if(typeof result.bytes==='number')draft.bytes=result.bytes;touchFileDraft(draft);
  persistFileDraft(operation.project,operation.path,draft);
}
export async function restoreUnresolvedSave(operation:PendingOperation):Promise<void> {
  if(!operation.project||!operation.path||typeof operation.payload?.text!=='string'||typeof operation.payload.revision!=='string')return;
  const workspace=browserStorage.workspace;
  if(operation.workspace!==undefined&&operation.workspace!==workspace)return;
  await restoreFileDraft(operation.project,operation.path);
  if(workspace!==browserStorage.workspace)return;
  const key=fileKey(operation.project,operation.path);
  if(fileDrafts[key])return;
  const text=operation.payload.text;
  const [indexed,normalized]=await Promise.all([indexText(text),normalizeFileText(text)]);
  if(workspace!==browserStorage.workspace)return;
  const draft=retainPreparedFile(operation.project,operation.path,{text,revision:operation.payload.revision,bytes:text.length},indexed.format,normalized);
  draft.original='';draft.baselineKnown=false;draft.editing=true;
  touchFileDraft(draft);
  persistFileDraft(operation.project,operation.path,draft);
}

function createEditorView():TextEditorView {
  return {selectionStart:0,selectionEnd:0,selectionDirection:'none',windowStart:0,scrollTop:0,scrollLeft:0,wrap:true,searching:false,needle:'',replacement:'',matchCase:false,searchNote:'',goLine:1};
}
function restoreEditorView(value:TextEditorView|undefined):TextEditorView {
  const base=createEditorView();if(!value)return base;
  return {
    selectionStart:safeNumber(value.selectionStart),selectionEnd:safeNumber(value.selectionEnd),
    windowStart:safeNumber(value.windowStart),
    selectionDirection:['forward','backward','none'].includes(value.selectionDirection)?value.selectionDirection:'none',
    scrollTop:safeNumber(value.scrollTop),scrollLeft:safeNumber(value.scrollLeft),wrap:value.wrap!==false,searching:value.searching===true,
    needle:typeof value.needle==='string'?value.needle:'',replacement:typeof value.replacement==='string'?value.replacement:'',
    matchCase:value.matchCase===true,searchNote:typeof value.searchNote==='string'?value.searchNote:'',goLine:Math.max(1,safeNumber(value.goLine)||1),
  };
}
function retainPreparedFile(project:string,path:string,value:TextSnapshot,format:TextFormat,text:string) {
  const key=fileKey(project,path);if(fileDrafts[key])return fileDrafts[key]!;
  fileDrafts[key]={
    text,original:text,revision:value.revision,bom:format.bom,newline:format.newline,bytes:value.bytes,
    editing:false,saving:false,error:'',history:createTextHistory(),view:createEditorView(),owner:browserStorage.workspace,version:0,
  };
  return fileDrafts[key]!;
}
function writeFileDraft(key:string) {
  const pending=pendingDraftWrites.get(key);if(!pending)return;
  pendingDraftWrites.delete(key);
  if(pending.draft.owner!==undefined&&pending.draft.owner!==pending.workspace)return;
  let writer=draftWriters.get(key);
  if(!writer){writer={pending:undefined,running:false,epoch:0};draftWriters.set(key,writer);}
  writer.pending={draft:pending.draft,workspace:pending.workspace};
  if(!writer.running)void drainDraftWriter(key,writer);
}
async function drainDraftWriter(key:string,writer:{pending:DraftWrite|undefined;running:boolean;epoch:number}):Promise<void> {
  writer.running=true;
  try{
    while(writer.pending){
      const pending=writer.pending,epoch=writer.epoch;writer.pending=undefined;
      try{
        await writeDraft(key,pending.workspace,pending.draft,
          ()=>writer.epoch===epoch&&fileDrafts[key]===pending.draft);
      }catch(error){reportStorageFailure(error);}
    }
  }finally{
    writer.running=false;if(!writer.pending&&draftWriters.get(key)===writer)draftWriters.delete(key);
  }
}
function safeNumber(value:unknown) {return typeof value==='number'&&Number.isFinite(value)&&value>=0?value:0;}
function validChange(change:unknown):boolean {
  if(!change||typeof change!=='object')return false;
  const value=change as {at?:unknown;removed?:unknown;inserted?:unknown};
  return typeof value.at==='number'&&Number.isSafeInteger(value.at)&&value.at>=0&&typeof value.removed==='string'&&typeof value.inserted==='string';
}
