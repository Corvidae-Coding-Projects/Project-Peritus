//! Stable gateway-owned browser records. Native effect authority stays in the gateway.

interface RecordValue { workspace:string; kind:string; id:string; value:unknown }
export const browserStorage=$state({workspace:'',error:''});
let opening:Promise<IDBDatabase>|undefined;
const writes=new Map<string,Promise<void>>();

export function selectWorkspace(identity:string) {
  browserStorage.workspace=identity;
}
function failure(error:unknown) {
  browserStorage.error=`Browser persistence is unavailable: ${error instanceof Error?error.message:String(error)}. Keep this page open or export unsaved work until storage is available.`;
}
async function database():Promise<IDBDatabase> {
  if(opening)return opening;
  opening=new Promise<IDBDatabase>((resolve,reject)=>{
    const request=indexedDB.open('peritus-workspaces',1);
    let abandoned=false;
    request.onupgradeneeded=()=>{
      const store=request.result.createObjectStore('records',{keyPath:['workspace','kind','id']});
      store.createIndex('owner',['workspace','kind']);
    };
    request.onblocked=()=>{
      abandoned=true;const error=new Error('Another open tab is holding the browser database upgrade. Close that tab to release its owner.');
      failure(error);reject(error);
    };
    request.onerror=()=>reject(request.error??new Error('Could not open browser database'));
    request.onsuccess=()=>{
      const db=request.result;
      if(abandoned){db.close();return;}
      db.onversionchange=()=>{db.close();opening=undefined;};
      resolve(db);
    };
  });
  try{return await opening;}catch(error){opening=undefined;throw error;}
}
function completion(transaction:IDBTransaction):Promise<void> {
  return new Promise((resolve,reject)=>{
    transaction.oncomplete=()=>resolve();
    transaction.onerror=()=>reject(transaction.error??new Error('Browser database transaction failed'));
    transaction.onabort=()=>reject(transaction.error??new Error('Browser database transaction aborted'));
  });
}
function enqueue(workspace:string,kind:string,id:string,write:(store:IDBObjectStore)=>void,required=false):Promise<void> {
  if(!workspace){
    const error=new Error('The gateway workspace identity is not available yet');failure(error);
    return required?Promise.reject(error):Promise.resolve();
  }
  const key=JSON.stringify([workspace,kind,id]);
  const previous=writes.get(key)??Promise.resolve();
  const next=previous.catch(()=>{}).then(async()=>{
    try{
      const db=await database(),transaction=db.transaction('records','readwrite');
      const done=completion(transaction);
      try{write(transaction.objectStore('records'));}catch(error){transaction.abort();await done.catch(()=>{});throw error;}
      await done;
    }catch(error){failure(error);if(required)throw error;}
  });
  writes.set(key,next);
  const cleanup=()=>{if(writes.get(key)===next)writes.delete(key);};
  void next.then(cleanup,cleanup);
  return next;
}
export function storeRecord(kind:string,id:string,value:unknown,workspace=browserStorage.workspace):Promise<void> {
  return enqueue(workspace,kind,id,store=>store.put({workspace,kind,id,value} satisfies RecordValue));
}
/** A compound publication must not adopt a head after any prerequisite write failed. */
export function storeRecordChecked(kind:string,id:string,value:unknown,workspace=browserStorage.workspace):Promise<void> {
  return enqueue(workspace,kind,id,store=>store.put({workspace,kind,id,value} satisfies RecordValue),true);
}
export function reportStorageFailure(error:unknown):void {failure(error);}
export function storeRecordIfAbsent(kind:string,id:string,value:unknown,workspace=browserStorage.workspace):Promise<void>{
  return enqueue(workspace,kind,id,store=>{
    const reading=store.get([workspace,kind,id]);
    reading.onsuccess=()=>{
      if(reading.result===undefined)store.add({workspace,kind,id,value} satisfies RecordValue);
    };
  });
}
export function removeRecord(kind:string,id:string,workspace=browserStorage.workspace):Promise<void> {
  return enqueue(workspace,kind,id,store=>store.delete([workspace,kind,id]));
}
export async function readRecord<T>(kind:string,id:string,workspace=browserStorage.workspace):Promise<T|undefined> {
  try{
    const db=await database(),transaction=db.transaction('records','readonly');
    const done=completion(transaction);
    const reading=new Promise<RecordValue|undefined>((resolve,reject)=>{
      const request=transaction.objectStore('records').get([workspace,kind,id]);
      request.onsuccess=()=>resolve(request.result as RecordValue|undefined);
      request.onerror=()=>reject(request.error??new Error('Could not restore the browser record'));
    });
    const [record]=await Promise.all([reading,done]);
    return record?.value as T|undefined;
  }catch(error){failure(error);throw error;}
}

/** Restore one owned record per transaction, publishing progress before reading the next. */
export async function visitRecords<T>(
  kind:string,publish:(value:T)=>void|Promise<void>,workspace=browserStorage.workspace,
):Promise<boolean>{
  let after:string|undefined;
  try{
    for(;;){
      if(workspace!==browserStorage.workspace)return false;
      const db=await database(),transaction=db.transaction('records','readonly');
      const done=completion(transaction);
      const lower=after===undefined?[workspace,kind]:[workspace,kind,after];
      // Record IDs are strings; the array upper endpoint sorts after every string ID.
      const range=IDBKeyRange.bound(lower,[workspace,kind,[]],after!==undefined,true);
      const reading=new Promise<RecordValue|undefined>((resolve,reject)=>{
        const request=transaction.objectStore('records').openCursor(range);
        request.onsuccess=()=>resolve(request.result?.value as RecordValue|undefined);
        request.onerror=()=>reject(request.error??new Error('Could not restore the next browser record'));
      });
      const [record]=await Promise.all([reading,done]);
      if(workspace!==browserStorage.workspace)return false;
      if(!record)return true;
      if(record.workspace!==workspace||record.kind!==kind||typeof record.id!=='string'
        ||after!==undefined&&record.id<=after)
        throw new Error('The retained browser record has an inconsistent ownership key.');
      after=record.id;
      await publish(record.value as T);
    }
  }catch(error){failure(error);return false;}
}

export async function readRecords<T>(kind:string):Promise<T[]|undefined>{
  const values:T[]=[];
  return await visitRecords<T>(kind,value=>{values.push(value);})?values:undefined;
}
