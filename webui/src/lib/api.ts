import type { Bootstrap } from './types';
import {retainOperation,forgetOperation,recovery} from './operations.svelte';
import type { SettingsDraftSnapshot } from './operations.svelte';
import {browserStorage,selectWorkspace} from './storage.svelte';
import {decodeTextSource,type TextReadProgress} from './files/read';
import type {TextSnapshot} from './files/text';

export class ApiError extends Error {constructor(message:string,public uncertain=false,public definitive=true){super(message);}}
export interface ActionOptions {operation?:string;settings?:SettingsDraftSnapshot;workspace?:string}

let token = '';
let bootstrapGeneration=0;
export interface ObservationOptions {signal?:AbortSignal}
export async function request<T>(path: string, body?: unknown, observation:ObservationOptions={}): Promise<T> {
  const options=()=>({ ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }),...(observation.signal?{signal:observation.signal}:{}),headers:{'Content-Type':'application/json','x-peritus-token':token} });
  let response=await fetch(path,options());
  observation.signal?.throwIfAborted();
  if(response.status===403&&path!=='/api/bootstrap'){
    await bootstrap(observation);response=await fetch(path,options()); // Rejection by the auth boundary cannot have admitted an effect.
  }
  const value=await response.json();
  observation.signal?.throwIfAborted();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Request failed (${response.status})`,!!value?.uncertain);
  return value as T;
}
export async function bootstrap(observation:ObservationOptions={}): Promise<Bootstrap> {
  const generation=++bootstrapGeneration;
  const value = await request<Bootstrap>('/api/bootstrap',undefined,observation);
  if(generation===bootstrapGeneration){token=value.token;selectWorkspace(value.identity??'');}
  return value;
}
export async function query<T>(kind: string, args: Record<string, string | number> = {}, observation:ObservationOptions={}): Promise<T> {
  const workspace=browserStorage.workspace;
  const params = new URLSearchParams({ kind, ...Object.fromEntries(Object.entries(args).map(([key, value]) => [key, String(value)])),workspace });
  const value=await request<T>(`/api/query?${params}`,undefined,observation);
  if(workspace!==browserStorage.workspace)throw new DOMException('The observation belongs to the previous gateway workspace.','AbortError');
  return value;
}
export async function readText(project:string,path:string,publish:(value:TextReadProgress)=>void,observation:ObservationOptions={}):Promise<TextSnapshot> {
  const workspace=browserStorage.workspace;
  const url='/api/text?'+new URLSearchParams({workspace,project,path});
  const fetchSource=()=>fetch(url,{...(observation.signal?{signal:observation.signal}:{}),headers:{'x-peritus-token':token}});
  let response=await fetchSource();
  if(response.status===403){await bootstrap(observation);response=await fetchSource();}
  if(!response.ok){const value=await response.json();throw new ApiError(value?.error??`File observation failed (${response.status})`);}
  if(workspace!==browserStorage.workspace){await response.body?.cancel();throw new ApiError('The file observation belongs to the previous gateway workspace.');}
  const result=await decodeTextSource(response,value=>{
    observation.signal?.throwIfAborted();
    if(workspace!==browserStorage.workspace)throw new ApiError('The file observation belongs to the previous gateway workspace.');
    publish(value);
  },observation.signal);
  observation.signal?.throwIfAborted();
  if(workspace!==browserStorage.workspace)throw new DOMException('The file observation belongs to the previous gateway workspace.','AbortError');
  return result;
}
export async function action<T = Record<string, unknown>>(command: string, args: Record<string, unknown> = {}, options:ActionOptions={}): Promise<T> {
  const operation = options.operation??crypto.randomUUID();
  const workspace=options.workspace??browserStorage.workspace;
  const payload = JSON.parse(JSON.stringify({ ...args, command, operation, workspace })) as Record<string,unknown>;
  void retainOperation({command,operation,payload,at:Date.now(),...(typeof args.session==='string'?{session:args.session}:{}),...(typeof args.project==='string'?{project:args.project}:{}),...(options.settings?{settings:options.settings}:{})});
  try {
    const result = await request<T>('/api/action', payload);
    forgetOperation(operation,workspace);
    if(workspace!==browserStorage.workspace)throw new ApiError('The original action completed in its owning workspace. Reopen that workspace to view its outcome.');
    return result;
  } catch (error) {
    if(error instanceof ApiError&&!error.uncertain){forgetOperation(operation,workspace);throw error;}
    if(workspace!==browserStorage.workspace)throw new ApiError('The original action belongs to the previous gateway workspace. Its retained identity is available when that workspace is reopened.',true,false);
    // Never resend a mutation after a broken connection. Query its original record.
    const receipt=await query<{result:T&{error?:string}|null}|null>('operation',{operation}).catch(()=>null);
    if(workspace!==browserStorage.workspace)throw new ApiError('Receipt inspection changed gateway workspaces. The original action is retained in its owning workspace.',true,false);
    if(receipt?.result){forgetOperation(operation,workspace);if(receipt.result.error)throw new ApiError(receipt.result.error);return receipt.result;}
    throw new ApiError(`${error instanceof Error?error.message:error} Outcome uncertain; inspect the original operation before sending again. [${operation}]`,true,false);
  }
}
export function rawUrl(project: string, path: string, download = false): string {
  return `/api/raw?${new URLSearchParams({project, path, kind: download ? 'download' : 'view'})}`;
}
export function pdfUrl(project:string,path:string):string {
  return `/api/raw?${new URLSearchParams({project,path,kind:'pdf'})}`;
}

export interface TerminalInputAttempt {workspace:string;input:string;body:string;digest?:string;form:boolean;state?:'pending'|'unknown'}
export interface RecoverableTerminalInput {input:string;body:string;digest:string;state:'pending'|'unknown'}

async function terminalFetch(workspace:string,path:string,options:()=>RequestInit,observation:ObservationOptions={}):Promise<Response>{
  let response:Response;
  const observedOptions=()=>({...options(),...(observation.signal?{signal:observation.signal}:{})});
  try{response=await fetch(path,observedOptions());}
  catch(error){throw new ApiError(error instanceof Error?error.message:String(error),true,false);}
  observation.signal?.throwIfAborted();
  if(response.status===403){
    await bootstrap(observation);
    if(workspace!==browserStorage.workspace)throw new ApiError('The console belongs to the previous gateway workspace. Reopen that workspace before retrying.',true,false);
    response=await fetch(path,observedOptions());
  }
  observation.signal?.throwIfAborted();
  if(workspace!==browserStorage.workspace){await response.body?.cancel();throw new ApiError('The console response belongs to the previous gateway workspace.',true,false);}
  return response;
}

async function sha256(value:string):Promise<string>{
  const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(value));
  return Array.from(new Uint8Array(bytes),byte=>byte.toString(16).padStart(2,'0')).join('');
}

export function terminalInputAttempt(workspace:string,body:string,form:boolean):TerminalInputAttempt {
  return {workspace,input:crypto.randomUUID().replaceAll('-',''),body,form};
}

export async function terminalInput(id:string,attempt:TerminalInputAttempt):Promise<void> {
  const workspace=attempt.workspace;
  attempt.digest??=await sha256(attempt.body);
  const params=new URLSearchParams({workspace,input:attempt.input,digest:attempt.digest,form:String(attempt.form)});
  const options=()=>({method:'POST',headers:{'Content-Type':'application/octet-stream','x-peritus-token':token},body:attempt.body});
  const response=await terminalFetch(workspace,`/api/terminal/${encodeURIComponent(id)}/input?${params}`,options);
  const value=await response.json();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Terminal input failed (${response.status})`,!!value?.uncertain,!value?.uncertain);
}

export async function reviewTerminalInput(id:string,attempt:TerminalInputAttempt):Promise<void> {
  if(attempt.state!=='unknown'||!attempt.digest)throw new ApiError('Inspect the exact retained unknown input before acknowledging it.');
  const params=new URLSearchParams({workspace:attempt.workspace,input:attempt.input,digest:attempt.digest});
  const response=await terminalFetch(attempt.workspace,`/api/terminal/${encodeURIComponent(id)}/input-review?${params}`,()=>({method:'POST',headers:{'x-peritus-token':token}}));
  const value=await response.json();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Input review failed (${response.status})`,!!value?.uncertain);
}

export async function terminalRead<T>(workspace:string,id:string,after:string,recoverInput=false,observation:ObservationOptions={}):Promise<T>{
  const params=new URLSearchParams({workspace,after,recover_input:String(recoverInput)});
  const response=await terminalFetch(workspace,`/api/terminal/${encodeURIComponent(id)}?${params}`,()=>({headers:{'x-peritus-token':token}}),observation);
  const value=await response.json();
  observation.signal?.throwIfAborted();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Terminal read failed (${response.status})`,!!value?.uncertain);
  return value as T;
}

export async function terminalResize(workspace:string,id:string,cols:number,rows:number,pixelWidth:number,pixelHeight:number):Promise<{ok:boolean}> {
  const body=JSON.stringify({workspace,cols,rows,pixelWidth,pixelHeight});
  const response=await terminalFetch(workspace,`/api/terminal/${encodeURIComponent(id)}/resize`,()=>({method:'POST',headers:{'Content-Type':'application/json','x-peritus-token':token},body}));
  const value=await response.json();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Terminal resize failed (${response.status})`,!!value?.uncertain);
  return value;
}

export async function terminalInterrupt(workspace:string,id:string):Promise<void> {
  const body=JSON.stringify({workspace});
  const response=await terminalFetch(workspace,`/api/terminal/${encodeURIComponent(id)}/interrupt`,()=>({method:'POST',headers:{'Content-Type':'application/json','x-peritus-token':token},body}));
  const value=await response.json();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Terminal interrupt failed (${response.status})`,!!value?.uncertain);
}

export async function retryOperation(operation:string):Promise<void> {
  const retained=recovery.pending.find(value=>value.operation===operation);
  await request('/api/operation-retry',{operation,workspace:retained?.workspace??browserStorage.workspace,confirmed:true,payload:retained?.payload});
  // Reconciliation owns the eventual claim and application of this operation's exact result.
}

export async function cancelOperation(operation:string):Promise<void> {
  const retained=recovery.pending.find(value=>value.operation===operation);
  await request('/api/operation-cancel',{operation,workspace:retained?.workspace??browserStorage.workspace});
}

export async function saveFile(project:string,path:string,revision:string,text:string):Promise<{revision:string;bytes:number}> {
  const operation=crypto.randomUUID();
  const workspace=browserStorage.workspace;
  void retainOperation({operation,command:'file-save',project,path,payload:{project,path,revision,text},at:Date.now()});
  const params=new URLSearchParams({project,path,revision,operation,workspace});
  try{
    let response=await fetch(`/api/file?${params}`,{method:'PUT',headers:{'Content-Type':'text/plain;charset=UTF-8','x-peritus-token':token},body:text});
    if(response.status===403){await bootstrap();response=await fetch(`/api/file?${params}`,{method:'PUT',headers:{'Content-Type':'text/plain;charset=UTF-8','x-peritus-token':token},body:text});}
    const result=await response.json();
    if(!response.ok||result.error)throw new ApiError(result.error??`Save failed (${response.status})`,!!result.uncertain);
    forgetOperation(operation,workspace);
    if(workspace!==browserStorage.workspace)throw new ApiError('The original save completed in its owning workspace. Reopen that workspace to view the saved file.');
    return result;
  }catch(error){
    if(error instanceof ApiError&&!error.uncertain){forgetOperation(operation,workspace);throw error;}
    if(workspace!==browserStorage.workspace)throw new ApiError('The original save belongs to the previous gateway workspace. Its exact body remains retained there.',true,false);
    const receipt=await query<{result:{revision:string;bytes:number;error?:string}|null}|null>('operation',{operation}).catch(()=>null);
    if(workspace!==browserStorage.workspace)throw new ApiError('Receipt inspection changed gateway workspaces. The original save body is retained in its owning workspace.',true,false);
    if(receipt?.result){forgetOperation(operation,workspace);if(receipt.result.error)throw new ApiError(receipt.result.error);return receipt.result;}
    throw new ApiError('Save outcome uncertain. Your draft is retained. Reconnect and inspect the disk version before saving again.',true,false);
  }
}
