import { marked } from 'marked';

interface RenderRequest {id:string;text:string}
const worker=globalThis as unknown as {
  onmessage:((event:MessageEvent<RenderRequest>)=>void)|null;
  postMessage:(value:unknown)=>void;
};
const renderer=new marked.Renderer();
function escape(value:string){
  return value.replace(/[&<>"']/g,unit=>({
    '&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'
  })[unit]!);
}
// Image syntax remains inert. The UI sanitizes all returned HTML before publishing it.
renderer.image=({text})=>`<span>[Image: ${escape(text)}]</span>`;
worker.onmessage=event=>{
  const {id,text}=event.data;
  try{worker.postMessage({id,html:marked.parse(text,{async:false,renderer})});}
  catch(error){worker.postMessage({id,error:error instanceof Error?error.message:String(error)});}
};
