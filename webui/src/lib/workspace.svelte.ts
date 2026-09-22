import * as api from './api';
import {latestConversation} from './conversations';
import { commands } from './commands/catalog';
import { parseSlash } from './commands/slash';
import { gitCommand } from './commands/git';
import {forgetFile} from './files/drafts.svelte';
import {recovery,restoreOperations,forgetOperation} from './operations.svelte';
import type { Attachment,Bootstrap, ConsoleSession, Conversation, Facts, FileTab, GitStatus, Mode, Preferences, Project, Run, Session, Workspace } from './types';

export const defaults: Preferences = { theme:'nixie',density:'comfortable',motion:true,sound:false,font_size:14,font_family:'Barlow, sans-serif',mono_family:'ui-monospace, monospace',explorer_width:248,controls_visible:true,explorer_visible:true,word_wrap:true,markdown_preview:true,shortcuts:{commands:'Mod+k',files:'Mod+Shift+e',git:'Mod+Shift+g',new:'Mod+Alt+n',settings:'Mod+,'},tokens:{},aliases:{} };
export const ui = $state({
  workspace: {projects:[],sessions:[]} as Workspace, preferences:{...defaults}, loading:true, fatal:'',
  projectId:'',sessionId:'',files:[] as FileTab[],activeFile:'',drafts:{} as Record<string,string>,modes:{} as Record<string,Mode>,
  conversations:{} as Record<string,Conversation>,facts:null as Facts|null, factsError:'',
  connected:false,ready:false,readiness:'Checking',connectionMessage:'Checking daemon connection',
  attachments:{} as Record<string,Attachment[]>,attaching:{} as Record<string,number>,
  git:null as GitStatus|null,gitError:'',gitBusy:false,gitRevision:0,
  panel:'conversation', drawer:'files',overlay:'',palette:'', notice:'',noticeError:false,
  pending:{} as Record<string,boolean>,details:false,config:'',configPath:'',configError:'',
  consoles:[] as ConsoleSession[],consoleId:'',
  reportTitle:'',reportText:'',runs:[] as Run[],
});
export function openProjects(): Project[] { return ui.workspace.projects.filter(p=>!p.closed); }
export function project(): Project|undefined { return openProjects().find(p=>p.id===ui.projectId); }
export function session(): Session|undefined { return ui.workspace.sessions.find(s=>s.id===ui.sessionId); }
export function notify(text:string,error=false) { ui.notice=text;ui.noticeError=error; }
export async function attempt(work:()=>Promise<unknown>) {
  try { await work(); } catch(error) { notify(error instanceof Error?error.message:String(error),true); }
}
export async function refresh() {
  const value:Bootstrap=await api.bootstrap(); ui.workspace=value.workspace;ui.consoles=value.consoles??[];ui.config=value.config;ui.configPath=value.configPath;
  restoreOperations(value.pendingOperations??[]);
  ui.preferences=value.preferences ?? {...defaults}; ui.configError=value.configError ?? '';
  if(!project()) ui.projectId=openProjects()[0]?.id ?? '';
  if(!session() || session()?.closed || session()?.project!==ui.projectId) ui.sessionId=ui.workspace.sessions.find(s=>s.project===ui.projectId&&!s.closed)?.id ?? '';
}
export async function start() {
  try {
    const saved=localStorage.getItem('peritus:layout:v1');
    if(saved) {
      const value=JSON.parse(saved);
      if(typeof value.projectId==='string')ui.projectId=value.projectId;
      if(typeof value.sessionId==='string')ui.sessionId=value.sessionId;
      if(value.drafts&&typeof value.drafts==='object')ui.drafts=value.drafts;
      if(Array.isArray(value.files))ui.files=value.files.filter((f:FileTab)=>typeof f.path==='string'&&typeof f.session==='string');
      if(value.modes&&typeof value.modes==='object')ui.modes=value.modes;
      if(value.attachments&&typeof value.attachments==='object')ui.attachments=Object.fromEntries(Object.entries(value.attachments).filter(([,items])=>Array.isArray(items)).map(([id,items])=>[id,(items as Attachment[]).filter(item=>typeof item?.id==='string'&&typeof item.path==='string')]));
    }
    await refresh(); await loadProject(); await poll();
    await reconcileOperations();
  }catch(error){ui.fatal=error instanceof Error?error.message:String(error);}
  finally{ui.loading=false;}
}
export function persist() {
  try{localStorage.setItem('peritus:layout:v1',JSON.stringify({projectId:ui.projectId,sessionId:ui.sessionId,drafts:ui.drafts,files:ui.files,modes:ui.modes,attachments:ui.attachments}));}
  catch{notify('Browser storage is full. Drafts remain available until this page closes.',true);}
}
export async function loadProject() {
  const id=ui.projectId;ui.facts=null;ui.factsError='';
  if(!id){ui.git=null;ui.gitError='';return;}
  await Promise.all([
    loadGit(),
    api.query<Facts>('facts',{project:id}).then(value=>{if(ui.projectId===id)ui.facts=value;}).catch(error=>{if(ui.projectId===id)ui.factsError=String(error.message);}),
  ]);
}
export async function loadGit() {
  const id=ui.projectId; if(!id)return;
  try{const value=await api.query<GitStatus>('git',{project:id});if(ui.projectId===id){ui.git=value;ui.gitError='';}}
  catch(error){if(ui.projectId===id){ui.git=null;ui.gitError=error instanceof Error?error.message:String(error);}}
}
export async function selectProject(id:string) {
  ui.projectId=id;ui.sessionId=ui.workspace.sessions.find(s=>s.project===id&&!s.closed)?.id??'';
  ui.activeFile='';ui.panel='conversation';ui.git=null;await loadProject();
}
export function selectSession(id:string) {
  const target=ui.workspace.sessions.find(s=>s.id===id);if(!target)return;
  if(target.project!==ui.projectId){ui.projectId=target.project;void loadProject();}
  ui.sessionId=id;ui.activeFile='';ui.panel='conversation';
}
export async function openProject(root:string) {
  const value=await api.action<Project>('open-project',{root});await refresh();ui.overlay='';await selectProject(value.id);
  notify(`Project connected: ${value.name}`);
}
export async function closeProject(id=ui.projectId) {
  const selected=ui.projectId;
  await api.action('close-project',{project:id});await refresh();
  if(selected!==ui.projectId){ui.activeFile='';await loadProject();}
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
export function observeConversation(id:string,value:Conversation) {
  ui.conversations[id]=latestConversation(ui.conversations[id],value);
  if(!ui.modes[id]&&value.mode)ui.modes[id]=value.mode;
}
export async function poll() {
  try{const status=await api.query<{ready:boolean;readiness:string;diagnostic?:string}>('daemon');ui.connected=true;ui.ready=status.ready;ui.readiness=status.readiness;ui.connectionMessage=status.diagnostic||status.readiness;}
  catch(error){ui.connected=false;ui.ready=false;ui.readiness='Offline';ui.connectionMessage=error instanceof Error?error.message:String(error);}
  ui.consoles=await api.query<ConsoleSession[]>('consoles').catch(()=>ui.consoles);
  if(!ui.connected)return;
  const projectId=ui.projectId;
  if(projectId)try{const facts=await api.query<Facts>('facts',{project:projectId});if(ui.projectId===projectId){ui.facts=facts;ui.factsError='';}}catch(error){if(ui.projectId===projectId){ui.facts=null;ui.factsError=error instanceof Error?error.message:String(error);}}
  await reconcileOperations();
  const selected=ui.sessionId;
  const ids=new Set([selected,...Object.entries(ui.conversations).filter(([,c])=>c.run?.busy).map(([id])=>id)]);
  await Promise.all([...ids].filter(Boolean).map(async id=>{
    try{observeConversation(id,await api.query<Conversation>('conversation',{session:id}));}
    catch(error){if(ui.conversations[id]?.run)notify(`Could not refresh this run: ${error instanceof Error?error.message:error}`,true);}
  }));
}
export async function send() {
  const id=ui.sessionId,original=ui.drafts[id]??'';const text=original.trim();if(!text||ui.pending[id]||ui.attaching[id])return;
  const parsed=parseSlash(text);
  if(parsed.kind==='error'){notify(parsed.message,true);return;}
  if(parsed.kind==='command'){
    await dispatch(parsed.name,[...parsed.args]);
    if(ui.drafts[id]===original)ui.drafts[id]='';return;
  }
  if(recovery.pending.some(item=>item.session===id))throw new Error('Resolve the original operation below before sending another message.');
  if(!ui.ready||!ui.facts?.ready)throw new Error(ui.facts?.reason||ui.factsError||ui.connectionMessage);
  const attachments=[...(ui.attachments[id]??[])];
  ui.pending[id]=true;
  try{
    const value=await api.action<Conversation>('send',{session:id,text,attachments:attachments.map(file=>file.id),mode:ui.modes[id]??'chat',models:ui.conversations[id]?.models??session()?.settings?.models??{},providers:ui.conversations[id]?.run?.providers??session()?.settings?.providers??{}});
    observeConversation(id,value);if(ui.drafts[id]===original)ui.drafts[id]='';
    ui.attachments[id]=(ui.attachments[id]??[]).filter(file=>!attachments.some(sent=>sent.id===file.id));
    if(ui.workspace.sessions.find(s=>s.id===id)?.title==='New conversation')await editSession(id,{title:text.slice(0,60)});
    notify('Message received. Incorporation is shown when observed by the daemon.');
  }finally{ui.pending[id]=false;}
}
export async function attachFile(path:string,projectId=ui.projectId){
  const id=ui.sessionId;if(!id)throw new Error('Open a conversation before attaching a file.');
  if(projectId!==ui.projectId)throw new Error('Attach a file from this session’s project.');
  if((ui.attachments[id]??[]).some(file=>file.path===path)){notify('This file is already attached. Remove it first to take a fresh snapshot.');return;}
  if((ui.attachments[id]?.length??0)+(ui.attaching[id]??0)>=16)throw new Error('Attach at most 16 files to one message.');
  ui.attaching[id]=(ui.attaching[id]??0)+1;
  try{const file=await api.action<Attachment>('attach-file',{session:id,project:projectId,path});ui.attachments[id]=[...(ui.attachments[id]??[]),file];if(ui.sessionId===id){ui.activeFile='';ui.panel='conversation';}notify(`Attached snapshot of ${path} to the next message.`);}
  finally{ui.attaching[id]=Math.max(0,(ui.attaching[id]??1)-1);}
}
export async function reconcileOperations(operation?:string){
  const outcome={recovered:0,unresolved:0,failed:0};
  for(const item of [...recovery.pending]){
    if(operation&&item.operation!==operation)continue;
    if(item.session&&ui.pending[item.session]||item.command==='attach-file'&&item.session&&ui.attaching[item.session])continue;
    let record;
    try{record=await api.query<{input:Record<string,unknown>;result:Conversation&{error?:string}|null}|null>('operation',{operation:item.operation});}
    catch{outcome.failed++;continue;}
    if(!record?.result){outcome.unresolved++;continue;}
    outcome.recovered++;
    forgetOperation(item.operation);
    if(item.command==='send'&&item.session&&!record.result.error){
      observeConversation(item.session,record.result);
      if(ui.drafts[item.session]?.trim()===record.input.text)ui.drafts[item.session]='';
      const sent=Array.isArray(record.input.attachments)?record.input.attachments:[];
      ui.attachments[item.session]=(ui.attachments[item.session]??[]).filter(file=>!sent.includes(file.id));
    }
    notify(record.result.error||`Recovered the original ${item.command} outcome. Nothing was repeated.`,!!record.result.error);
  }
  return outcome;
}
export async function gitAction(kind:string,paths:string[]=[],message='',options:Record<string,unknown>={}) {
  ui.gitBusy=true;const id=ui.projectId;
  try{
    const value=await api.action<{output:string}>('git',{...options,project:id,action:kind,paths,message});
    if(kind.startsWith('diff')){ui.reportTitle=kind==='diff'?'Working tree diff':'Staged diff';ui.reportText=value.output||'No changes in this comparison.';ui.overlay='report';}
    else notify(value.output.trim()||`Git ${kind} completed.`);
    await loadGit();ui.gitRevision++;
  }finally{ui.gitBusy=false;}
}
export async function openConsole(args:string[]=[],title='Harness console',daemon=false) {
  const value=await api.action<ConsoleSession>('console',{project:ui.projectId,args,daemon,title});
  ui.consoles.push(value);ui.consoleId=value.id;ui.overlay='console';
}
export async function openRun(id:string) {
  const value=await api.action<Session>('open-run',{run:id});
  await refresh();await selectProject(value.project);selectSession(value.id);await poll();ui.overlay='';
}
export async function openWorkbench(suggestion='') {
  const value=await api.action<ConsoleSession>('workbench',{session:ui.sessionId,suggestion});
  ui.consoles.push(value);ui.consoleId=value.id;ui.overlay='console';
}
export async function dispatch(id:string,args:string[]=[],depth=0):Promise<void> {
  if(depth>8)throw new Error('Command alias cycle. Check [aliases] in your dotfile.');
  const alias=ui.preferences.aliases[id];
  if(alias){const parsed=parseSlash(alias);if(parsed.kind==='command')return dispatch(parsed.name,[...parsed.args,...args],depth+1);}
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
    case 'files':ui.preferences.explorer_visible=true;ui.drawer='files';ui.panel='files';return;
    case 'git':
      ui.preferences.explorer_visible=true;ui.drawer='git';ui.panel='files';
      if(args.length){const command=gitCommand(args);if(command.kind==='status'){await loadGit();return;}if(!command.confirmation||confirm(command.confirmation))await gitAction(command.kind,command.paths,command.message,command.options);return;}
      await loadGit();return;
    case 'settings':case 'sessions':case 'model':case 'effort':ui.overlay=id==='effort'?'model':id;return;
    case 'help':ui.palette=' ';return;
    case 'details':ui.details=!ui.details;return;
    case 'status':case 'diff':{
      const run=ui.conversations[ui.sessionId]?.run;ui.reportTitle=id==='diff'?'Candidate diff':'Run status';
      ui.reportText=run?(id==='diff'?run.diff:run.status):'No run has been observed in this session yet.';ui.overlay='report';return;
    }
    case 'improvements':ui.overlay='improvements';return;
    case 'runs':ui.runs=await api.query<Run[]>('runs');ui.overlay='runs';return;
    case 'stop':case 'retry':case 'export':
      observeConversation(ui.sessionId,await api.action<Conversation>('control',{session:ui.sessionId,action:id}));notify(`Requested ${id} for this run.`);return;
    case 'discard':ui.overlay='discard';return;
    case 'accept':case 'commit':return openConsole(['runs',id,'--run',ui.sessionId],`${command.label} · ${session()?.title}`,true);
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
    case 'reconnect':await refresh();await loadProject();await poll();notify(ui.connected?'Reconnected. Original operation records retained.':ui.connectionMessage,!ui.connected);return;
    default:
      if(command.group==='Workbench console'){
        const suggestion=`${command.slash}${args.length?' '+args.join(' '):''}`;
        return openWorkbench(suggestion);
      }
  }
}
