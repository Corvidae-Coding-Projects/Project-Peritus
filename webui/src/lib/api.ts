import type { Bootstrap } from './types';
import {retainOperation,forgetOperation} from './operations.svelte';

export class ApiError extends Error {constructor(message:string,public uncertain=false,public definitive=true){super(message);}}

let token = '';
export async function request<T>(path: string, body?: unknown): Promise<T> {
  const options=()=>({ ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }),headers:{'Content-Type':'application/json','x-peritus-token':token} });
  let response=await fetch(path,options());
  if(response.status===403&&path!=='/api/bootstrap'){
    await bootstrap();response=await fetch(path,options()); // Rejection by the auth boundary cannot have admitted an effect.
  }
  const value=await response.json();
  if(!response.ok||value?.error)throw new ApiError(value?.error||`Request failed (${response.status})`,!!value?.uncertain);
  return value as T;
}
export async function bootstrap(): Promise<Bootstrap> {
  const value = await request<Bootstrap>('/api/bootstrap'); token = value.token; return value;
}
export function query<T>(kind: string, args: Record<string, string | number> = {}): Promise<T> {
  const params = new URLSearchParams({ kind, ...Object.fromEntries(Object.entries(args).map(([key, value]) => [key, String(value)])) });
  return request(`/api/query?${params}`);
}
export async function action<T = Record<string, unknown>>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const operation = crypto.randomUUID();
  const payload = { command, operation, ...args };
  retainOperation({command,operation,at:Date.now(),...(typeof args.session==='string'?{session:args.session}:{}),...(typeof args.project==='string'?{project:args.project}:{})});
  try {
    const result = await request<T>('/api/action', payload);
    forgetOperation(operation); return result;
  } catch (error) {
    if(error instanceof ApiError&&!error.uncertain){forgetOperation(operation);throw error;}
    // Never resend a mutation after a broken connection. Query its original record.
    const receipt=await query<{result:T&{error?:string}|null}|null>('operation',{operation}).catch(()=>null);
    if(receipt?.result){forgetOperation(operation);if(receipt.result.error)throw new ApiError(receipt.result.error);return receipt.result;}
    throw new ApiError(`${error instanceof Error?error.message:error} Outcome uncertain; inspect the original operation before sending again. [${operation}]`,true,false);
  }
}
export function rawUrl(project: string, path: string, download = false): string {
  return `/api/raw?${new URLSearchParams({project, path, kind: download ? 'download' : 'view'})}`;
}
export function pdfUrl(project:string,path:string):string {
  return `/api/raw?${new URLSearchParams({project,path,kind:'pdf'})}`;
}

export async function saveFile(project:string,path:string,revision:string,text:string):Promise<{revision:string;bytes:number}> {
  const operation=crypto.randomUUID();
  retainOperation({operation,command:'file-save',project,path,at:Date.now()});
  const params=new URLSearchParams({project,path,revision,operation});
  try{
    let response=await fetch(`/api/file?${params}`,{method:'PUT',headers:{'Content-Type':'text/plain;charset=UTF-8','x-peritus-token':token},body:text});
    if(response.status===403){await bootstrap();response=await fetch(`/api/file?${params}`,{method:'PUT',headers:{'Content-Type':'text/plain;charset=UTF-8','x-peritus-token':token},body:text});}
    const result=await response.json();
    if(!response.ok||result.error)throw new ApiError(result.error??`Save failed (${response.status})`,!!result.uncertain);
    forgetOperation(operation);return result;
  }catch(error){
    if(error instanceof ApiError&&!error.uncertain){forgetOperation(operation);throw error;}
    const receipt=await query<{result:{revision:string;bytes:number;error?:string}|null}|null>('operation',{operation}).catch(()=>null);
    if(receipt?.result){forgetOperation(operation);if(receipt.result.error)throw new ApiError(receipt.result.error);return receipt.result;}
    throw new ApiError('Save outcome uncertain. Your draft is retained. Reconnect and inspect the disk version before saving again.',true,false);
  }
}
