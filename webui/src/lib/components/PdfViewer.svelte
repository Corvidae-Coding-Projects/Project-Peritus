<script lang="ts">
  import {query,pdfUrl} from '../api';
  import Icon from './Icon.svelte';
  let {project,path}:{project:string;path:string}=$props();
  let checking=$state(true),error=$state(''),retry=$state(0);
  const supported=typeof navigator!=='undefined'&&navigator.pdfViewerEnabled!==false;
  $effect(()=>{
    const id=project,name=path;void retry;
    let cancelled=false;checking=true;error='';
    query<{bytes:number}>('pdf',{project:id,path:name})
      .catch(reason=>{if(!cancelled)error=reason instanceof Error?reason.message:String(reason);})
      .finally(()=>{if(!cancelled)checking=false;});
    return()=>{cancelled=true;};
  });
</script>
<div class="pdf-preview">
  {#if checking}
    <div class="skeleton-lines" role="status" aria-label="Checking PDF"><i></i><i></i><i></i></div>
  {:else if error}
    <div class="viewer-empty" role="status"><Icon name="file" size={36}/><h2>PDF preview unavailable</h2><p>{error}</p><button class="key" onclick={()=>retry++}>Retry PDF preview</button></div>
  {:else if !supported}
    <div class="viewer-empty"><Icon name="file" size={36}/><h2>Inline PDF viewing is unavailable</h2><p>This browser has no enabled inline PDF viewer. Use Open PDF in new tab or Download above to open it in a PDF application.</p></div>
  {:else}
    <p class="pdf-hint">Browser PDF viewer · if the document stays blank, use Open PDF in new tab or Download.</p>
    <iframe title={`PDF preview: ${path}`} src={pdfUrl(project,path)}></iframe>
  {/if}
</div>
<style>
  .pdf-preview{height:100%;min-height:220px;display:flex;flex-direction:column}
  .pdf-preview iframe{display:block;flex:1;width:100%;min-height:180px;border:0;background:var(--display)}
  .pdf-preview .viewer-empty{min-height:0;flex:1}
  .pdf-hint{padding:9px 14px;margin:0;color:var(--muted);font-size:.8rem;line-height:1.5;border-bottom:1px solid var(--line)}
</style>
