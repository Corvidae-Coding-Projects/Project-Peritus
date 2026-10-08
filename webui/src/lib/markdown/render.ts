interface Job {
  id:string;text:string;resolve:(html:string)=>void;reject:(error:unknown)=>void;
  signal?:AbortSignal;abort:()=>void;
}
let worker:Worker|undefined,active:Job|undefined;
const queued:Job[]=[];

function finish(job:Job,error?:unknown,html?:string){
  job.signal?.removeEventListener('abort',job.abort);
  if(error!==undefined)job.reject(error);else job.resolve(html!);
}
function replaceWorker(){worker?.terminate();worker=undefined;}
function failed(error:unknown){
  const job=active;active=undefined;replaceWorker();
  if(job)finish(job,error);
  queueMicrotask(advance);
}
function advance(){
  if(active||!queued.length)return;
  active=queued.shift()!;
  try{
    if(!worker){
      const renderer=new Worker(new URL('./worker.ts',import.meta.url),{type:'module'});
      worker=renderer;
      renderer.onmessage=event=>{
        if(worker!==renderer)return;
        const job=active;if(!job)return;
        const value=event.data;
        if(value?.id!==job.id){failed(new Error('Markdown renderer returned another document.'));return;}
        active=undefined;
        if(typeof value.error==='string')finish(job,new Error(value.error));
        else if(typeof value.html==='string')finish(job,undefined,value.html);
        else finish(job,new Error('Markdown renderer returned invalid content.'));
        advance();
      };
      renderer.onerror=event=>{
        if(worker!==renderer)return;
        event.preventDefault();failed(new Error(event.message||'Markdown renderer failed.'));
      };
      renderer.onmessageerror=()=>{if(worker===renderer)failed(new Error('Markdown renderer response could not be read.'));};
    }
    worker.postMessage({id:active.id,text:active.text});
  }catch(error){failed(error);}
}

/** A cancellable observation; no renderer job owns or limits conversation execution. */
export function renderMarkdown(text:string,signal?:AbortSignal):Promise<string>{
  return new Promise((resolve,reject)=>{
    if(signal?.aborted){reject(signal.reason??new DOMException('Rendering cancelled.','AbortError'));return;}
    const job:Job={id:crypto.randomUUID(),text,resolve,reject,signal,abort:()=>{
      if(active===job){active=undefined;replaceWorker();}
      else{const index=queued.indexOf(job);if(index<0)return;queued.splice(index,1);}
      finish(job,signal?.reason??new DOMException('Rendering cancelled.','AbortError'));
      advance();
    }};
    signal?.addEventListener('abort',job.abort,{once:true});queued.push(job);advance();
  });
}
