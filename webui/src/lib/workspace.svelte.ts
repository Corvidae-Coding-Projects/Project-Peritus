import * as api from './api';
import {latestConversation} from './conversations';
import { commands } from './commands/catalog';
import { parseSlash } from './commands/slash';
import { gitCommand } from './commands/git';
import { resolveAlias } from './commands/aliases';
import {forgetFile,restoreUnresolvedSave,settleFileSave} from './files/drafts.svelte';
import {recovery,restoreOperations,beginSettlement,endSettlement} from './operations.svelte';
import type { PendingOperation, SettingsDraftSnapshot } from './operations.svelte';
import {browserStorage} from './storage.svelte';
import type { Attachment,Bootstrap, ConsoleSession, Conversation, Facts, FileTab, GitEffectResult, GitInventoryPage, GitOutputPage, GitStatus, ImprovementEvaluation, Mode, Preferences, Project, Run, RunLegalControls, RunPage, Session, Workspace } from './types';

export const defaults: Preferences = { theme:'nixie',density:'comfortable',motion:true,sound:false,font_size:14,font_family:'Barlow, sans-serif',mono_family:'ui-monospace, monospace',explorer_width:248,controls_visible:true,explorer_visible:true,word_wrap:true,markdown_preview:true,shortcuts:{commands:'Mod+k',files:'Mod+Shift+e',git:'Mod+Shift+g',new:'Mod+Alt+n',settings:'Mod+,'},tokens:{},aliases:{} };
export function clonePreferences(value:Preferences):Preferences {
  return {...value,shortcuts:{...value.shortcuts},tokens:{...value.tokens},aliases:{...value.aliases}};
}
export const ui = $state({
  workspace: {projects:[],sessions:[]} as Workspace, preferences:clonePreferences(defaults),savedPreferences:clonePreferences(defaults),loading:true,fatal:'',
  startupActive:false,startupMessage:'Connecting your workspace…',
  projectId:'',sessionId:'',files:[] as FileTab[],activeFile:'',drafts:{} as Record<string,string>,modes:{} as Record<string,Mode>,
  conversations:{} as Record<string,Conversation>,facts:null as Facts|null, factsError:'',
  connected:false,ready:false,readiness:'Checking',connectionMessage:'Checking daemon connection',
  attachments:{} as Record<string,Attachment[]>,attaching:{} as Record<string,number>,
  git:null as GitStatus|null,gitError:'',gitBusy:false,gitRevision:0,
  observing:0,observationsPaused:false,
  panel:'conversation', drawer:'files',overlay:'',palette:'', notice:'',noticeError:false,
  pending:{} as Record<string,boolean>,details:false,config:'',configPath:'',configError:'',
  settings:{view:'controls' as 'controls'|'dotfile',source:'',initialized:false,preferencesRevision:0,preferencesBaselineRevision:0,sourceRevision:0,sourceBaselineRevision:0,activeOperation:'',requestOperation:''},
  consoles:[] as ConsoleSession[],consoleId:'',
  reportTitle:'',reportText:'',reportOutput:null as GitOutputPage|null,reportOperation:'',runs:[] as Run[],runCursor:null as string|null,runStore:'',runLoading:false,
});
export function openProjects(): Project[] { return ui.workspace.projects.filter(p=>!p.closed); }
export function project(): Project|undefined { return openProjects().find(p=>p.id===ui.projectId); }
export function session(): Session|undefined { return ui.workspace.sessions.find(s=>s.id===ui.sessionId); }
export function notify(text:string,error=false) { ui.notice=text;ui.noticeError=error; }
export function runActive(run:Run|undefined):boolean { return run?.operation.state==='Running'; }
export function runInspection(run:Run):string {
  const legal=(Object.entries(run.operation.legalControls) as [keyof RunLegalControls,boolean][]).filter(([,allowed])=>allowed).map(([name])=>name).join(', ')||'none';
  return [`Operation: ${run.operation.kind} / ${run.operation.state}`,`Identity: ${run.operation.identity}`,`Known: ${run.operation.known}`,`Uncertain: ${run.operation.uncertainty||'nothing material'}`,`Legal controls: ${legal}`,run.status,run.summary,run.gates,run.review].filter(Boolean).join('\n\n');
}
export async function attempt(work:()=>Promise<unknown>) {
  try { await work(); } catch(error) { notify(error instanceof Error?error.message:String(error),true); }
}
let workspaceRefreshGeneration=0;
export async function refresh(options:ReadOptions={}) {
  const generation=++workspaceRefreshGeneration;
  cancelObservation('pending-operations');
  const value:Bootstrap=await api.bootstrap(options);
  const workspace=value.identity??'';
  if(generation!==workspaceRefreshGeneration||workspace!==browserStorage.workspace)return;
  ui.workspace=value.workspace;ui.consoles=value.consoles??[];ui.configPath=value.configPath;
  restoreOperations(value.pendingOperations??[],workspace);
  const preferences=clonePreferences(value.preferences??defaults);
  const preferencesClean=ui.settings.preferencesRevision===ui.settings.preferencesBaselineRevision&&samePreferences(ui.preferences,ui.savedPreferences);
  const sourceClean=ui.settings.sourceRevision===ui.settings.sourceBaselineRevision&&ui.settings.source===ui.config;
  if(!ui.settings.initialized){ui.preferences=clonePreferences(preferences);ui.settings.source=value.config;ui.settings.initialized=true;}
  else{
    if(preferencesClean)ui.preferences=clonePreferences(preferences);
    if(sourceClean)ui.settings.source=value.config;
  }
  ui.savedPreferences=clonePreferences(preferences);ui.config=value.config;
  ui.configError=value.configError ?? '';
  if(!project()) ui.projectId=openProjects()[0]?.id ?? '';
  if(!session() || session()?.closed || session()?.project!==ui.projectId) ui.sessionId=ui.workspace.sessions.find(s=>s.project===ui.projectId&&!s.closed)?.id ?? '';
  const snapshot=value.pendingOperationsSnapshot??'';
  let cursor=value.pendingOperationsCursor??null;
  if(cursor)observe('pending-operations',async signal=>{
    while(cursor){
      const page=await api.query<{operations:PendingOperation[];cursor:string|null;snapshot:string}>(
        'pending-operations',{cursor,snapshot},{signal},
      );
      if(signal.aborted||generation!==workspaceRefreshGeneration||workspace!==browserStorage.workspace)return;
      if(page.snapshot!==snapshot)throw new Error('Pending operations changed during inspection; refresh the workspace again.');
      restoreOperations(page.operations,workspace);cursor=page.cursor;
      poll();
    }
  });
}
export function settingsSnapshot():SettingsDraftSnapshot {
  return {
    preferences:clonePreferences(ui.preferences),
    source:ui.settings.source,
    preferencesClean:ui.settings.preferencesRevision===ui.settings.preferencesBaselineRevision&&samePreferences(ui.preferences,ui.savedPreferences),
    sourceClean:ui.settings.sourceRevision===ui.settings.sourceBaselineRevision&&ui.settings.source===ui.config,
    preferencesRevision:ui.settings.preferencesRevision,
    preferencesBaselineRevision:ui.settings.preferencesBaselineRevision,
    sourceRevision:ui.settings.sourceRevision,
    sourceBaselineRevision:ui.settings.sourceBaselineRevision,
  };
}
export function editSettingsSource(source:string){
  if(source===ui.settings.source)return;
  ui.settings.source=source;ui.settings.sourceRevision++;
}
export function editSettingsPreferences(){ui.settings.preferencesRevision++;}
function sameRecord(left:Record<string,string>,right:Record<string,string>|undefined):boolean{
  if(!right)return false;
  const keys=Object.keys(left);return keys.length===Object.keys(right).length&&keys.every(key=>left[key]===right[key]);
}
function samePreferences(left:Preferences,right:Preferences|undefined):boolean{
  return !!right&&left.theme===right.theme&&left.density===right.density&&left.motion===right.motion&&left.sound===right.sound
    &&left.font_size===right.font_size&&left.font_family===right.font_family&&left.mono_family===right.mono_family
    &&left.explorer_width===right.explorer_width&&left.controls_visible===right.controls_visible
    &&left.explorer_visible===right.explorer_visible&&left.word_wrap===right.word_wrap
    &&left.markdown_preview===right.markdown_preview&&sameRecord(left.shortcuts,right.shortcuts)
    &&sameRecord(left.tokens,right.tokens)&&sameRecord(left.aliases,right.aliases);
}
export function reconcileConfiguration(command:string,input:Record<string,unknown>,result:Record<string,unknown>,snapshot?:SettingsDraftSnapshot){
  if(result.error)return;
  const preferences=(command==='config'?result:result.preferences) as Preferences|undefined;
  const config=command==='config'?input.text:result.config;
  if(!preferences||typeof config!=='string')return;
  ui.savedPreferences=clonePreferences(preferences);ui.config=config;ui.configError='';
  if(!snapshot)return;
  const preferencesSubmitted=command==='preferences';
  const preferencesClean=snapshot.preferencesClean===true;
  const preferencesUnchanged=ui.settings.preferencesRevision===snapshot.preferencesRevision&&samePreferences(ui.preferences,snapshot.preferences);
  if(preferencesUnchanged&&(preferencesSubmitted||preferencesClean)){
    ui.preferences=clonePreferences(preferences);ui.settings.preferencesBaselineRevision=ui.settings.preferencesRevision;
  }
  const sourceSubmitted=command==='config';
  const sourceClean=snapshot.sourceClean===true;
  const sourceUnchanged=typeof snapshot.source==='string'&&ui.settings.sourceRevision===snapshot.sourceRevision&&ui.settings.source===snapshot.source;
  if(sourceUnchanged&&(sourceSubmitted||sourceClean)){
    ui.settings.source=config;ui.settings.sourceBaselineRevision=ui.settings.sourceRevision;
  }
}

interface ReadOptions {signal?:AbortSignal}
const observations=new Map<string,AbortController>();
let startupController:AbortController|undefined;
let runPageController:AbortController|undefined;
let runPageGeneration=0;
let inventoryPageController:AbortController|undefined;
let inventoryPageGeneration=0;
let reportPageController:AbortController|undefined;
let reportPageGeneration=0;
let gitActionGeneration=0;

function aborted(error:unknown):boolean {
  return error instanceof Error&&error.name==='AbortError';
}
function updateObservationCount(){ui.observing=observations.size;}
function cancelObservation(key:string){
  const controller=observations.get(key);if(!controller)return;
  observations.delete(key);controller.abort();updateObservationCount();
}
function observe(key:string,work:(signal:AbortSignal)=>Promise<void>){
  if(ui.observationsPaused||observations.has(key))return;
  const controller=new AbortController();observations.set(key,controller);updateObservationCount();
  void work(controller.signal).catch(error=>{if(!aborted(error))notify(`Could not refresh ${key}: ${error instanceof Error?error.message:String(error)}`,true);}).finally(()=>{
    if(observations.get(key)===controller){observations.delete(key);updateObservationCount();}
  });
}
export function cancelObservations(pause=true){
  if(pause)ui.observationsPaused=true;
  for(const controller of observations.values())controller.abort();
  observations.clear();updateObservationCount();
  runPageGeneration++;runPageController?.abort();runPageController=undefined;ui.runLoading=false;
  inventoryPageGeneration++;inventoryPageController?.abort();inventoryPageController=undefined;
  reportPageGeneration++;reportPageController?.abort();reportPageController=undefined;
}
export function recheckObservations(){
  cancelObservations(false);ui.observationsPaused=false;loadProject();poll();
}
export function cancelStartup(){startupController?.abort();}
function restoreLayout(){
  try{
    const saved=localStorage.getItem('peritus:layout:v1');
    if(!saved)return;
    const value=JSON.parse(saved);
    if(typeof value.projectId==='string')ui.projectId=value.projectId;
    if(typeof value.sessionId==='string')ui.sessionId=value.sessionId;
    if(value.drafts&&typeof value.drafts==='object')ui.drafts=value.drafts;
    if(Array.isArray(value.files))ui.files=value.files.filter((f:FileTab)=>typeof f.path==='string'&&typeof f.session==='string');
    if(value.modes&&typeof value.modes==='object')ui.modes=value.modes;
    if(value.attachments&&typeof value.attachments==='object')ui.attachments=Object.fromEntries(Object.entries(value.attachments).filter(([,items])=>Array.isArray(items)).map(([id,items])=>[id,(items as Attachment[]).filter(item=>typeof item?.id==='string'&&typeof item.path==='string')]));
  }catch(error){browserStorage.error=`Browser layout could not be restored: ${error instanceof Error?error.message:String(error)}. The gateway workspace will still open.`;}
}
export async function start() {
  if(startupController)return;
  const controller=new AbortController();startupController=controller;
  ui.loading=true;ui.fatal='';ui.startupActive=true;ui.startupMessage='Connecting your workspace…';
  restoreLayout();
  try {
    await refresh({signal:controller.signal});ui.loading=false;loadProject();poll();
  }catch(error){
    if(aborted(error)){ui.startupMessage='Gateway connection check cancelled. Retained recovery controls remain available.';}
    else{ui.fatal=error instanceof Error?error.message:String(error);ui.loading=false;}
  }finally{
    if(startupController===controller){startupController=undefined;ui.startupActive=false;}
  }
}
type LayoutTask={kind:'idle'|'task';id:number};
let layoutTask:LayoutTask|undefined;
export function persist() {
  // Keep the reactive effect subscribed without stringifying retained drafts in
  // the same turn as a keystroke or selection change.
  trackLayout();
  if(layoutTask)return;
  if(typeof requestIdleCallback==='function')layoutTask={kind:'idle',id:requestIdleCallback(writeLayout)};
  else layoutTask={kind:'task',id:setTimeout(writeLayout,0) as unknown as number};
}
export function flushPersist() {
  if(!layoutTask)return;
  if(layoutTask.kind==='idle')cancelIdleCallback(layoutTask.id);else clearTimeout(layoutTask.id);
  writeLayout();
}
function writeLayout() {
  layoutTask=undefined;
  try{localStorage.setItem('peritus:layout:v1',JSON.stringify({projectId:ui.projectId,sessionId:ui.sessionId,drafts:ui.drafts,files:ui.files,modes:ui.modes,attachments:ui.attachments}));}
  catch(error){browserStorage.error=`Browser layout could not be saved: ${error instanceof Error?error.message:String(error)}. Drafts remain available while this page stays open.`;}
}
function trackLayout() {
  void ui.projectId;void ui.sessionId;
  for(const key of Object.keys(ui.drafts))void ui.drafts[key];
  for(const file of ui.files){void file.path;void file.session;void file.project;}
  for(const key of Object.keys(ui.modes))void ui.modes[key];
  for(const key of Object.keys(ui.attachments))for(const attachment of ui.attachments[key]??[]){
    void attachment.id;void attachment.session;void attachment.project;void attachment.path;void attachment.bytes;void attachment.digest;void attachment.media;
  }
}
export function loadProject() {
  const id=ui.projectId;ui.facts=null;ui.factsError='';
  cancelObservation('project:git');cancelObservation('project:facts');
  if(!id){ui.git=null;ui.gitError='';return;}
  observe('project:git',signal=>loadGit({project:id,signal}));
  observe('project:facts',signal=>loadFacts(id,{signal}));
}
async function loadFacts(id:string,options:ReadOptions={}) {
  try{const value=await api.query<Facts>('facts',{project:id},options);if(ui.projectId===id&&!options.signal?.aborted){ui.facts=value;ui.factsError='';}}
  catch(error){if(aborted(error))throw error;if(ui.projectId===id){ui.facts=null;ui.factsError=error instanceof Error?error.message:String(error);}}
}
let gitLoadGeneration=0;
export async function loadGit(options:ReadOptions&{project?:string}={}) {
  const generation=++gitLoadGeneration;
  inventoryPageGeneration++;inventoryPageController?.abort();inventoryPageController=undefined;
  const id=options.project??ui.projectId;if(!id)return;
  try{const value=await api.query<GitStatus>('git',{project:id},options);if(ui.projectId===id&&generation===gitLoadGeneration&&!options.signal?.aborted){const remoteDetails=mergeGitRemotes([],value.remoteDetails);ui.git={...value,remotes:gitRemoteSummary(remoteDetails),remoteDetails};ui.gitError='';}}
  catch(error){if(aborted(error))throw error;if(ui.projectId===id&&generation===gitLoadGeneration){ui.git=null;ui.gitError=error instanceof Error?error.message:String(error);}}
}
function mergeGitRemotes(current:GitStatus['remoteDetails'],incoming:GitStatus['remoteDetails']):GitStatus['remoteDetails'] {
  const remotes=new Map(current.map(remote=>[remote.name,{...remote,fetch:[...remote.fetch],push:[...remote.push]}]));
  for(const fragment of incoming){const remote=remotes.get(fragment.name)??{name:fragment.name,fetch:[],push:[]};
    for(const value of fragment.fetch)if(!remote.fetch.includes(value))remote.fetch.push(value);
    for(const value of fragment.push)if(!remote.push.includes(value))remote.push.push(value);
    remotes.set(fragment.name,remote);
  }
  return [...remotes.values()].sort((left,right)=>left.name.localeCompare(right.name));
}
function gitRemoteSummary(remotes:GitStatus['remoteDetails']):string {
  return remotes.flatMap(remote=>[
    ...remote.fetch.map(value=>`${remote.name}\t${value} (fetch)`),
    ...(remote.push.length?remote.push:remote.fetch).map(value=>`${remote.name}\t${value} (push)`),
  ]).join('\n');
}
export async function loadMoreGitInventory():Promise<void> {
  const current=ui.git,id=ui.projectId,cursor=current?.inventoryCursor,snapshot=current?.inventorySnapshot;
  if(!current||!cursor||!snapshot||!id)return;
  const workspace=browserStorage.workspace,loadGeneration=gitLoadGeneration,generation=++inventoryPageGeneration;
  inventoryPageController?.abort();const controller=new AbortController();inventoryPageController=controller;
  try{
    const page=await api.query<GitInventoryPage>('git-inventory',{project:id,cursor,snapshot},{signal:controller.signal});
    if(controller.signal.aborted||generation!==inventoryPageGeneration||loadGeneration!==gitLoadGeneration||workspace!==browserStorage.workspace||ui.projectId!==id||ui.git?.inventorySnapshot!==snapshot||ui.git?.inventoryCursor!==cursor)return;
    if(page.snapshot!==snapshot)throw new Error('Git inventory materialization changed; refresh it again.');
    const known=new Set(ui.git.branches.map(branch=>branch.ref));
    const remoteDetails=mergeGitRemotes(ui.git.remoteDetails,page.remoteDetails);
    ui.git={...ui.git,branches:[...ui.git.branches,...page.branches.filter(branch=>!known.has(branch.ref))],remotes:gitRemoteSummary(remoteDetails),remoteDetails,inventoryCursor:page.cursor};
  }catch(error){if(controller.signal.aborted||generation!==inventoryPageGeneration)return;throw error;}
  finally{if(generation===inventoryPageGeneration&&inventoryPageController===controller)inventoryPageController=undefined;}
}
export async function selectProject(id:string) {
  ui.projectId=id;ui.sessionId=ui.workspace.sessions.find(s=>s.project===id&&!s.closed)?.id??'';
  ui.activeFile='';ui.panel='conversation';ui.git=null;loadProject();
}
export function selectSession(id:string) {
  const target=ui.workspace.sessions.find(s=>s.id===id);if(!target)return;
  if(target.project!==ui.projectId){ui.projectId=target.project;loadProject();}
  ui.sessionId=id;ui.activeFile='';ui.panel='conversation';
}
export async function openProject(root:string) {
  const value=await api.action<Project>('open-project',{root});await refresh();ui.overlay='';await selectProject(value.id);
  notify(`Project connected: ${value.name}`);
}
export async function closeProject(id=ui.projectId) {
  const selected=ui.projectId;
  await api.action('close-project',{project:id});await refresh();
  if(selected!==ui.projectId){ui.activeFile='';loadProject();}
  notify('Project tab closed. Its sessions are retained in Session library.');
}
export async function newSession(nested=false) {
  const value=await api.action<Session>('new-session',{project:ui.projectId,parent:nested?ui.sessionId:null});
  await refresh();selectSession(value.id);notify(nested?'Nested session opened in the same project.':'New conversation ready.');
}
export async function editSession(id:string,patch:Record<string,unknown>) {
  const selected=ui.sessionId;
  await api.action('session',{session:id,...patch});await refresh();
  if(ui.sessionId!==selected)ui.activeFile='';
}
export function openFile(path:string) {
  if(!ui.files.some(f=>f.path===path&&f.session===ui.sessionId))ui.files.push({path,session:ui.sessionId,project:ui.projectId});
  ui.activeFile=path;ui.panel='conversation';
}
export function closeFile(path:string) {
  if(!forgetFile(ui.projectId,path))return;
  ui.files=ui.files.filter(f=>!(f.path===path&&f.session===ui.sessionId));
  if(ui.activeFile===path)ui.activeFile='';
}
interface ConversationOwner {epoch:number;readable:number}
const conversationOwners=new Map<string,ConversationOwner>();
function conversationOwner(id:string):ConversationOwner {
  let owner=conversationOwners.get(id);
  if(!owner){owner={epoch:0,readable:0};conversationOwners.set(id,owner);}
  return owner;
}
export function beginConversationObservation(id:string):number {
  return conversationOwner(id).readable;
}
export function beginConversationEffect(id:string):number {
  const owner=conversationOwner(id);owner.epoch++;
  cancelObservation(`conversation:${id}`);
  return owner.epoch;
}
export function endConversationEffect(id:string,epoch:number){
  const owner=conversationOwner(id);
  if(owner.epoch===epoch)owner.readable=epoch;
}
export function observeConversation(id:string,value:Conversation,epoch:number) {
  const owner=conversationOwner(id);
  ui.conversations[id]=latestConversation(ui.conversations[id],value,epoch===owner.epoch);
  const mode=ui.conversations[id]?.mode;
  if(!ui.modes[id]&&mode)ui.modes[id]=mode;
}
function observeRecoveredConversation(id:string,value:Conversation,epoch:number|undefined){
  const current=ui.conversations[id],owner=conversationOwner(id);
  ui.conversations[id]=latestConversation(current,value,current===undefined&&epoch!==undefined&&epoch===owner.epoch);
  const mode=ui.conversations[id]?.mode;
  if(!ui.modes[id]&&mode)ui.modes[id]=mode;
}
function refreshConversation(id:string,fresh=false){
  const key=`conversation:${id}`;
  if(fresh)cancelObservation(key);
  observe(key,async signal=>{
    const epoch=beginConversationObservation(id);
    try{observeConversation(id,await api.query<Conversation>('conversation',{session:id},{signal}),epoch);}
    catch(error){if(aborted(error))throw error;if(ui.conversations[id]?.run)notify(`Could not refresh this run: ${error instanceof Error?error.message:String(error)}`,true);}
  });
}
export function poll() {
  if(ui.observationsPaused)return;
  observe('daemon',async signal=>{
    try{
      const status=await api.query<{ready:boolean;readiness:string;diagnostic?:string}>('daemon',{}, {signal});
      ui.connected=true;ui.ready=status.ready;ui.readiness=status.readiness;ui.connectionMessage=status.diagnostic||status.readiness;
    }catch(error){
      if(aborted(error))throw error;
      ui.connected=false;ui.ready=false;ui.readiness='Offline';ui.connectionMessage=error instanceof Error?error.message:String(error);
    }
  });
  observe('consoles',async signal=>{
    try{ui.consoles=await api.query<ConsoleSession[]>('consoles',{}, {signal});}
    catch(error){if(aborted(error))throw error;}
  });
  const projectId=ui.projectId;
  if(projectId)observe('project:facts',signal=>loadFacts(projectId,{signal}));
  for(const item of recovery.pending){
    observe(`operation:${item.operation}`,async signal=>{await reconcileOperations(item.operation,{signal});});
  }
  const selected=ui.sessionId;
  const ids=new Set([selected,...Object.entries(ui.conversations).filter(([,conversation])=>runActive(conversation.run)).map(([id])=>id)]);
  for(const id of [...ids].filter(Boolean))refreshConversation(id);
}
export async function send() {
  const id=ui.sessionId,original=ui.drafts[id]??'';const text=original;if(!text.trim()||ui.pending[id]||ui.attaching[id])return;
  const parsed=parseSlash(text.trim());
  if(parsed.kind==='error'){notify(parsed.message,true);return;}
  if(parsed.kind==='command'){
    await dispatch(parsed.name,[...parsed.args]);
    if(ui.drafts[id]===original)ui.drafts[id]='';return;
  }
  if(!ui.ready||!ui.facts?.ready)throw new Error(ui.facts?.reason||ui.factsError||ui.connectionMessage);
  const attachments=[...(ui.attachments[id]??[])];
  const epoch=beginConversationEffect(id);
  ui.pending[id]=true;
  try{
    const value=await api.action<Conversation>('send',{session:id,text,attachments:attachments.map(file=>file.id),mode:ui.modes[id]??'chat',models:ui.conversations[id]?.models??session()?.settings?.models??{},providers:ui.conversations[id]?.run?.providers??session()?.settings?.providers??{}});
    observeConversation(id,value,epoch);if(ui.drafts[id]===original)ui.drafts[id]='';
    ui.attachments[id]=(ui.attachments[id]??[]).filter(file=>!attachments.some(sent=>sent.id===file.id));
    if(ui.workspace.sessions.find(s=>s.id===id)?.title==='New conversation')await editSession(id,{title:text.slice(0,60)});
    notify(value.workbench?.observation??'Message received. Incorporation is shown when observed by the daemon.',!!value.workbench);
  }finally{endConversationEffect(id,epoch);ui.pending[id]=false;}
}
export async function attachFile(path:string,projectId=ui.projectId){
  const id=ui.sessionId;if(!id)throw new Error('Open a conversation before attaching a file.');
  if(projectId!==ui.projectId)throw new Error('Attach a file from this session’s project.');
  if((ui.attachments[id]??[]).some(file=>file.path===path)){notify('This file is already attached. Remove it first to take a fresh snapshot.');return;}
  ui.attaching[id]=(ui.attaching[id]??0)+1;
  try{const file=await api.action<Attachment>('attach-file',{session:id,project:projectId,path});ui.attachments[id]=[...(ui.attachments[id]??[]),file];if(ui.sessionId===id){ui.activeFile='';ui.panel='conversation';}notify(`Attached snapshot of ${path} to the next message.`);}
  finally{ui.attaching[id]=Math.max(0,(ui.attaching[id]??1)-1);}
}
export async function reconcileOperations(operation?:string,options:ReadOptions={}){
  const workspace=browserStorage.workspace;
  const items=[...recovery.pending].filter(item=>!operation||item.operation===operation);
  const outcomes=await Promise.all(items.map(async item=>{
    const receiptEpoch=item.session===undefined?undefined:beginConversationObservation(item.session);
    let record:{input:Record<string,unknown>;result:Conversation&{error?:string;conversation?:Conversation|null;session?:Session}|null;payload?:Record<string,unknown>;payloadError?:string}|null;
    try{record=await api.query('operation',{operation:item.operation},options);}
    catch(error){if(aborted(error))throw error;return {recovered:0,unresolved:0,failed:1};}
    if(workspace!==browserStorage.workspace)return {recovered:0,unresolved:0,failed:0};
    if(!record)return {recovered:0,unresolved:1,failed:0};
    if(item.command==='file-save'){
      if(record.payload)item.payload={...item.payload,...record.payload};
      if(record.payloadError)browserStorage.error=`Could not restore the exact pending file save: ${record.payloadError}`;
    }
    if(!record.result){
      if(item.command==='file-save')void restoreUnresolvedSave(item).catch(error=>{
        browserStorage.error=`Could not restore the exact pending file save: ${error instanceof Error?error.message:String(error)}`;
      });
      return {recovered:0,unresolved:1,failed:0};
    }
    // Reserve this receipt's UI application without deleting its recovery record. A failed
    // draft restore or receipt application leaves the exact outcome available for another try.
    const settlement=beginSettlement(item.operation,workspace);
    if(!settlement)return {recovered:0,unresolved:0,failed:0};
    const retained=settlement.value;
    try{
    if(['config','preferences'].includes(retained.command)){
      if(ui.settings.activeOperation===retained.operation)ui.settings.activeOperation='';
      if(record.input.command===retained.command&&!record.result.error)reconcileConfiguration(retained.command,record.input,record.result as unknown as Record<string,unknown>,retained.settings);
    }else if(retained.command==='file-save'&&!record.result.error){
      await settleFileSave(retained,record.result as unknown as Record<string,unknown>);
    }else if(['send','control','session-settings'].includes(retained.command)&&retained.session&&!record.result.error){
      const conversation=retained.command==='session-settings'?record.result.conversation:record.result;
      if(conversation)observeRecoveredConversation(retained.session,conversation,receiptEpoch);
      if(retained.command==='session-settings')observe('workspace',signal=>refresh({signal}));
      else if(retained.command==='send'){
        if(ui.drafts[retained.session]===record.input.text)ui.drafts[retained.session]='';
        const sent=Array.isArray(record.input.attachments)?record.input.attachments:[];
        ui.attachments[retained.session]=(ui.attachments[retained.session]??[]).filter(file=>!sent.includes(file.id));
      }
      refreshConversation(retained.session,true);
    }
    if(!endSettlement(settlement,true))return {recovered:0,unresolved:0,failed:0};
    notify(record.result.error||`Recovered the original ${retained.command} outcome. Nothing was repeated.`,!!record.result.error);
    return {recovered:1,unresolved:0,failed:0};
    }catch(error){
      if(workspace===browserStorage.workspace)notify(`Could not apply the retained ${retained.command} outcome: ${error instanceof Error?error.message:String(error)}`,true);
      return {recovered:0,unresolved:1,failed:1};
    }finally{endSettlement(settlement);}
  }));
  return outcomes.reduce((total,value)=>({recovered:total.recovered+value.recovered,unresolved:total.unresolved+value.unresolved,failed:total.failed+value.failed}),{recovered:0,unresolved:0,failed:0});
}
export async function gitAction(kind:string,paths:string[]=[],message='',options:Record<string,unknown>={}) {
  ui.gitBusy=true;const id=ui.projectId,workspace=browserStorage.workspace,generation=++gitActionGeneration;
  try{
    const value=await api.action<GitEffectResult>('git',{...options,project:id,action:kind,paths,message});
    if(generation!==gitActionGeneration||workspace!==browserStorage.workspace||ui.projectId!==id)return;
    const page=BigInt(value.stdout.bytes)>0n?value.stdout:value.stderr;
    const text=decodeGitOutput(page);
    const paged=page.next!==null||(BigInt(value.stdout.bytes)>0n&&BigInt(value.stderr.bytes)>0n);
    if(kind.startsWith('diff')||paged){
      reportPageGeneration++;reportPageController?.abort();reportPageController=undefined;
      ui.reportTitle=kind==='diff'?'Working tree diff':kind==='diff-staged'?'Staged diff':`Git ${kind} output`;ui.reportText=text||(kind.startsWith('diff')?'No changes in this comparison.':'No output on this stream.');ui.reportOutput=page;ui.reportOperation=value.operation;ui.overlay='report';
    }else notify(text.trim()||`Git ${kind} completed.`);
    ui.gitRevision++;observe('project:git',signal=>loadGit({project:id,signal}));
  }catch(error){if(generation!==gitActionGeneration||workspace!==browserStorage.workspace||ui.projectId!==id)return;throw error;}
  finally{if(generation===gitActionGeneration)ui.gitBusy=false;}
}
function decodeGitOutput(page:GitOutputPage):string {
  if(page.error)return `[${page.stream} retention error: ${page.error}]`;
  const binary=atob(page.data);const bytes=new Uint8Array(binary.length);
  for(let index=0;index<binary.length;index++)bytes[index]=binary.charCodeAt(index);
  return new TextDecoder().decode(bytes);
}
export async function loadGitOutput(offset:string,stream=ui.reportOutput?.stream):Promise<void> {
  const operation=ui.reportOperation,workspace=browserStorage.workspace;if(!operation||!stream)return;
  const generation=++reportPageGeneration;reportPageController?.abort();const controller=new AbortController();reportPageController=controller;
  try{
    const page=await api.query<GitOutputPage>('git-output',{operation,stream,offset},{signal:controller.signal});
    if(controller.signal.aborted||generation!==reportPageGeneration||workspace!==browserStorage.workspace||operation!==ui.reportOperation||ui.overlay!=='report')return;
    if(page.stream!==stream||page.offset!==offset)throw new Error('Git output paging returned a different retained page.');
    ui.reportOutput=page;ui.reportText=decodeGitOutput(page)||`No ${stream} output on this page.`;
  }catch(error){if(controller.signal.aborted||generation!==reportPageGeneration)return;throw error;}
  finally{if(generation===reportPageGeneration&&reportPageController===controller)reportPageController=undefined;}
}
export function clearGitOutput():void {
  reportPageGeneration++;reportPageController?.abort();reportPageController=undefined;
  ui.reportOutput=null;ui.reportOperation='';
}
export async function openConsole(args:string[]=[],title='Harness console',daemon=false) {
  const workspace=browserStorage.workspace;
  const value=await api.action<ConsoleSession>('console',{project:ui.projectId,session:daemon?ui.sessionId:null,args,daemon,title});
  ui.consoles.push({...value,workspace});ui.consoleId=value.id;ui.overlay='console';
}
async function runPage(cursor:string|null,replace:boolean) {
  const owner=browserStorage.workspace,generation=++runPageGeneration;
  runPageController?.abort();
  const controller=new AbortController();runPageController=controller;ui.runLoading=true;
  const expectedStore=replace?'':ui.runStore;
  try{
    const page=await api.query<RunPage>('runs',cursor?{cursor}:{},{signal:controller.signal});
    if(controller.signal.aborted||generation!==runPageGeneration||owner!==browserStorage.workspace)return;
    if(!replace&&page.store!==expectedStore)throw new Error('Run history changed durable stores. Refresh it before loading another page.');
    const seen=new Set(ui.runs.map(run=>run.id));
    if(!replace&&page.runs.some(run=>seen.has(run.id)))throw new Error('The daemon returned an overlapping run page. Refresh run history.');
    ui.runs=replace?page.runs:[...ui.runs,...page.runs];
    ui.runStore=page.store;ui.runCursor=page.cursor;
  }catch(error){
    if(controller.signal.aborted||generation!==runPageGeneration||owner!==browserStorage.workspace)return;
    throw error;
  }finally{
    if(generation===runPageGeneration&&runPageController===controller){runPageController=undefined;ui.runLoading=false;}
  }
}
export async function refreshRuns(){
  ui.runs=[];ui.runCursor=null;ui.runStore='';
  await runPage(null,true);
}
export async function loadMoreRuns(){
  const cursor=ui.runCursor;if(!cursor||ui.runLoading)return;
  await runPage(cursor,false);
}
export async function openRun(id:string) {
  const value=await api.action<Session>('open-run',{run:id});
  await refresh();await selectProject(value.project);selectSession(value.id);poll();ui.overlay='';
}
export async function openEvaluation(evaluation:ImprovementEvaluation) {
  const value=await api.action<Session>('open-workbench',{conversation:evaluation.conversation,run:evaluation.run,target:evaluation.target});
  await refresh();await selectProject(value.project);selectSession(value.id);poll();ui.overlay='';
}
export async function openWorkbench(suggestion='') {
  const workspace=browserStorage.workspace;
  const value=await api.action<ConsoleSession>('workbench',{session:ui.sessionId,suggestion});
  ui.consoles.push({...value,workspace});ui.consoleId=value.id;ui.overlay='console';
}
export async function dispatch(id:string,args:string[]=[]):Promise<void> {
  ({id,args}=resolveAlias(id,args,ui.preferences.aliases));
  const command=commands.find(c=>c.id===id);if(!command)throw new Error(`Unknown command /${id}. Open the command directory with /help.`);
  ui.palette='';
  if(['chat','plan','review','build'].includes(id)){
    ui.modes[ui.sessionId]=id as Mode;ui.activeFile='';
    if(args.length){ui.drafts[ui.sessionId]=args.join(' ');await send();}return;
  }
  switch(id){
    case 'new':return newSession();case 'nest':return newSession(true);
    case 'open':if(args.length)return openProject(args.join(' '));ui.overlay='open';return;
    case 'close-project':return closeProject();
    case 'files':
      if(!ui.preferences.explorer_visible){ui.preferences.explorer_visible=true;editSettingsPreferences();}
      ui.drawer='files';ui.panel='files';return;
    case 'git':
      if(!ui.preferences.explorer_visible){ui.preferences.explorer_visible=true;editSettingsPreferences();}
      ui.drawer='git';ui.panel='files';
      if(args.length){const command=gitCommand(args);if(command.kind==='status'){await loadGit();return;}if(!command.confirmation||confirm(command.confirmation))await gitAction(command.kind,command.paths,command.message,command.options);return;}
      await loadGit();return;
    case 'settings':case 'sessions':case 'model':case 'effort':ui.overlay=id==='effort'?'model':id;return;
    case 'help':ui.palette=' ';return;
    case 'details':ui.details=!ui.details;return;
    case 'status':case 'diff':{
      const run=ui.conversations[ui.sessionId]?.run;ui.reportTitle=id==='diff'?'Candidate diff':'Run status';
      clearGitOutput();ui.reportText=run?(id==='diff'?run.diff:runInspection(run)):'No run has been observed in this session yet.';ui.overlay='report';return;
    }
    case 'improvements':ui.overlay='improvements';return;
    case 'runs':ui.overlay='runs';await refreshRuns();return;
    case 'stop':case 'retry':case 'export':case 'acknowledge':{
      const destination=ui.sessionId,epoch=beginConversationEffect(destination);
      try{observeConversation(destination,await api.action<Conversation>('control',{session:destination,action:id}),epoch);notify(`Requested ${id} for this run.`);}
      finally{endConversationEffect(destination,epoch);}
      return;
    }
    case 'discard':ui.overlay='discard';return;
    case 'accept':case 'commit':{
      const selected=session();if(!selected)throw new Error('Open the exact run before using this control.');
      return openConsole(['runs',id,'--run',selected.run],`${command.label} · ${selected.title}`,true);
    }
    case 'providers':case 'workspaces':case 'update':return openConsole([id],command.label);
    case 'consoles':
      ui.consoles=await api.query<ConsoleSession[]>('consoles');
      ui.consoleId=ui.consoles.find(c=>c.project===ui.projectId&&c.session===ui.sessionId)?.id??ui.consoles.find(c=>c.project===ui.projectId)?.id??'';ui.overlay='console';return;
    case 'terminal':
      {const retained=ui.consoles.find(c=>c.project===ui.projectId&&c.session===ui.sessionId&&!c.ended);
      if(retained){ui.consoleId=retained.id;ui.overlay='console';return;}
      if(!ui.ready)return openConsole(['open',project()?.root??'.'],'Project setup');
      return openWorkbench();}
    case 'cli':if(args.length)return openConsole(args,'CLI command',!['update','providers','workspaces','open','help','completions','--help','--version'].includes(args[0]!));ui.overlay='cli';return;
    case 'reconnect':await refresh();recheckObservations();notify('Gateway workspace refreshed. Observation lanes are checking independently.');return;
    default:
      if(command.group==='Workbench console'){
        const suggestion=`${command.slash}${args.length?' '+args.join(' '):''}`;
        return openWorkbench(suggestion);
      }
  }
}
