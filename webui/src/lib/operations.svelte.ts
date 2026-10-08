import type { Preferences } from './types';
import {browserStorage,storeRecord,storeRecordIfAbsent,removeRecord,visitRecords} from './storage.svelte';

export interface SettingsDraftSnapshot {
  preferences:Preferences;
  source:string;
  preferencesClean:boolean;
  sourceClean:boolean;
  preferencesRevision:number;
  preferencesBaselineRevision:number;
  sourceRevision:number;
  sourceBaselineRevision:number;
}
export interface PendingOperation {operation:string;command:string;workspace?:string;session?:string;project?:string;path?:string;at?:number;settings?:SettingsDraftSnapshot;payload?:Record<string,unknown>}
export const recovery=$state({pending:[] as PendingOperation[]});
let owner='',restoration:Promise<void>|undefined;
const claimed=new Set<string>();
const settling=new Map<string,symbol>();
export interface OperationSettlement {value:PendingOperation;workspace:string;token:symbol}
function selectOwner(){
  if(owner===browserStorage.workspace)return;
  owner=browserStorage.workspace;recovery.pending=[];claimed.clear();settling.clear();restoration=undefined;
}
function mergeOperation(value:PendingOperation){
  const index=recovery.pending.findIndex(item=>item.operation===value.operation);
  const settings=recovery.pending[index]?.settings??value.settings;
  const retained=index<0?value:{...value,...recovery.pending[index],...(settings===undefined?{}:{settings})};
  if(index<0)recovery.pending.push(retained);else recovery.pending[index]=retained;
  return retained;
}
export async function retainOperation(value:PendingOperation){
  selectOwner();claimed.delete(value.operation);
  const retained=mergeOperation({...value,workspace:browserStorage.workspace});
  await storeRecord('operation',value.operation,JSON.parse(JSON.stringify(retained)));
}
export function forgetOperation(operation:string,workspace=browserStorage.workspace){
  claimOperation(operation,workspace);
}
export function beginSettlement(operation:string,workspace=browserStorage.workspace):OperationSettlement|undefined{
  if(workspace!==browserStorage.workspace)return undefined;
  selectOwner();
  if(settling.has(operation))return undefined;
  const value=recovery.pending.find(item=>item.operation===operation);
  if(!value)return undefined;
  const token=Symbol(operation);settling.set(operation,token);
  return {value,workspace,token};
}
export function endSettlement(settlement:OperationSettlement,completed=false):boolean{
  const {value,workspace,token}=settlement;
  if(workspace!==browserStorage.workspace||owner!==workspace||settling.get(value.operation)!==token)return false;
  settling.delete(value.operation);
  if(completed)return claimOperation(value.operation,workspace)!==undefined;
  return true;
}
export function claimOperation(operation:string,workspace=browserStorage.workspace):PendingOperation|undefined{
  if(workspace!==browserStorage.workspace){void removeRecord('operation',operation,workspace);return undefined;}
  selectOwner();claimed.add(operation);
  const retained=recovery.pending.find(item=>item.operation===operation);
  if(!retained)return undefined;
  recovery.pending=recovery.pending.filter(item=>item.operation!==operation);
  void removeRecord('operation',operation,workspace);
  try{sessionStorage.removeItem(`peritus:operation:${operation}`);}catch{/* Legacy metadata is not the durable record. */}
  return retained;
}
export function restoreOperations(values:PendingOperation[]=[],workspace=browserStorage.workspace){
  if(workspace!==browserStorage.workspace)return;
  selectOwner();
  const restore=(value:PendingOperation)=>{
    if((value.workspace===undefined||value.workspace===workspace)&&!claimed.has(value.operation))mergeOperation({...value,workspace});
  };
  for(const value of values)restore(value);
  const gatewayIds=new Set(values.map(value=>value.operation));
  try{
    for(const key of Object.keys(sessionStorage).filter(key=>key.startsWith('peritus:operation:'))){
      try{
        const value=JSON.parse(sessionStorage.getItem(key)!);
        // Legacy records have no workspace identity. Bind them only when this gateway owns
        // the same original operation; a different workspace cannot adopt the browser payload.
        if(typeof value?.operation==='string'&&typeof value.command==='string'&&gatewayIds.has(value.operation)){
          restore(value);void migrateLegacyOperation(value,workspace);
        }
      }catch{/* Malformed legacy metadata has no authority. */}
    }
  }catch(error){browserStorage.error=`Could not read legacy browser recovery records: ${String(error)}. Gateway records remain available.`;}
  if(restoration)return;
  restoration=visitRecords<PendingOperation>('operation',value=>{
    if(typeof value?.operation==='string'&&typeof value.command==='string')restore(value);
  },workspace).then(()=>{}).finally(()=>{if(workspace===browserStorage.workspace)restoration=undefined;});
}
async function migrateLegacyOperation(value:PendingOperation,workspace:string){
  if(workspace!==browserStorage.workspace||claimed.has(value.operation))return;
  // Existing durable payloads retain their original ownership. Legacy session metadata
  // is migrated only when the owning gateway has confirmed that operation identity.
  const retained=recovery.pending.find(item=>item.operation===value.operation);
  if(retained)await storeRecordIfAbsent('operation',value.operation,JSON.parse(JSON.stringify(retained)),workspace);
}
