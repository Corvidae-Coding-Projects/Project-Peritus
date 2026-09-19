export interface PendingOperation {operation:string;command:string;session?:string;project?:string;path?:string;at?:number}
export const recovery=$state({pending:[] as PendingOperation[]});
export function retainOperation(value:PendingOperation){
  if(!recovery.pending.some(item=>item.operation===value.operation))recovery.pending.push(value);
  try{sessionStorage.setItem(`peritus:operation:${value.operation}`,JSON.stringify(value));}catch{/* The gateway also durably records admitted operations. */}
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
