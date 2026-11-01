export interface PendingOperation {operation:string;command:string;session?:string;project?:string;path?:string;at?:number;draftOriginal?:string}
export const recovery=$state({pending:[] as PendingOperation[]});
export function retainOperation(value:PendingOperation){
  const key=`peritus:operation:${value.operation}`;
  let retained=value;
  try{
    const saved=JSON.parse(sessionStorage.getItem(key)??'null');
    if(retained.draftOriginal===undefined&&typeof saved?.draftOriginal==='string')retained={...retained,draftOriginal:saved.draftOriginal};
  }catch{/* Client-only metadata is optional; retain the server operation record. */}
  if(!recovery.pending.some(item=>item.operation===retained.operation))recovery.pending.push(retained);
  try{sessionStorage.setItem(key,JSON.stringify(retained));}catch{/* The gateway also durably records admitted operations. */}
}
export function forgetOperation(operation:string){
  recovery.pending=recovery.pending.filter(item=>item.operation!==operation);
  try{sessionStorage.removeItem(`peritus:operation:${operation}`);}catch{/* Memory and gateway state remain authoritative. */}
}
export function restoreOperations(values:PendingOperation[]=[]){
  for(const value of values)retainOperation(value);
  for(const key of Object.keys(sessionStorage).filter(key=>key.startsWith('peritus:operation:'))){
    try{const value=JSON.parse(sessionStorage.getItem(key)!);if(typeof value.operation==='string'&&typeof value.command==='string')retainOperation(value);}catch{/* Ignore malformed client-only metadata. */}
  }
}
