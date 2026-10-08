<script lang="ts">
  import DOMPurify from 'dompurify';
  import { renderMarkdown } from '../markdown/render';
  import { safeTextOffset } from '../files/text';
  let { text }: {text:string} = $props();
  const DISPLAY_UNITS=32_000;
  let start=$state(0),history=$state<number[]>([]),source='';
  let end=$derived(safeTextOffset(text,Math.min(text.length,start+DISPLAY_UNITS)));
  let paged=$derived(text.length>DISPLAY_UNITS);
  let html=$state(''),rendering=$state(true),error=$state('');
  $effect.pre(()=>{
    const current=text;if(current===source)return;
    const previousEnd=safeTextOffset(source,Math.min(source.length,start+DISPLAY_UNITS));
    if(current.length<previousEnd||safeTextOffset(current,start)!==start
      ||current.slice(start,previousEnd)!==source.slice(start,previousEnd)){start=0;history=[];}
    source=current;
  });
  $effect(()=>{
    const current=text;
    const controller=new AbortController();rendering=true;error='';
    // Large documents remain exact text pages: formatting an isolated slice would
    // misrepresent constructs whose closing delimiter belongs to another page.
    if(current.length>DISPLAY_UNITS){rendering=false;return ()=>controller.abort();}
    void renderMarkdown(current,controller.signal).then(value=>{
      if(controller.signal.aborted)return;
      html=DOMPurify.sanitize(value, {
        FORBID_TAGS:['style','iframe','form','input','button','img','video','audio','svg'],
        FORBID_ATTR:['style','id'], ALLOW_DATA_ATTR:false,
      });
      rendering=false;
    }).catch(value=>{
      if(controller.signal.aborted)return;
      error=value instanceof Error?value.message:String(value);rendering=false;
    });
    return()=>controller.abort();
  });
  function previous(){if(history.length)start=safeTextOffset(text,history.pop()!);}
  function next(){if(end<text.length){history.push(start);start=end;}}
</script>
<div class="markdown" aria-busy={rendering}>
  {#if paged}<pre class="text-page">{text.slice(start,end)}</pre>
    <div class="text-navigation"><button class="flat" disabled={!history.length} onclick={previous}>Previous text</button><span>Text view</span><button class="flat" disabled={end===text.length} onclick={next}>Next text</button></div>
  {:else if rendering}<span class="markdown-pending" role="status">Rendering content…</span>
  {:else if error}<p role="status">Formatting unavailable: {error}</p><pre class="text-page">{text}</pre>
  {:else}{@html html}{/if}
</div>
<style>
  .text-page{white-space:pre-wrap;overflow-wrap:anywhere;max-width:100%;margin:0}
  .text-navigation{display:flex;flex-wrap:wrap;align-items:center;gap:10px;margin-top:10px;font-size:12px}
</style>
