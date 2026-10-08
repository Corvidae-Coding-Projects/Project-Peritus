<script lang="ts">
  import {onDestroy,untrack} from 'svelte';
  import hljs from 'highlight.js/lib/core';
  import python from 'highlight.js/lib/languages/python';
  import rust from 'highlight.js/lib/languages/rust';
  import javascript from 'highlight.js/lib/languages/javascript';
  import typescript from 'highlight.js/lib/languages/typescript';
  import json from 'highlight.js/lib/languages/json';
  import {readText,rawUrl,pdfUrl,saveFile} from '../api';
  import {ui,attempt,notify,loadGit} from '../workspace.svelte';
  import {browserStorage} from '../storage.svelte';
  import {fileBytes,restoreFileDraft,retainFileIncrementally,persistFileDraft,flushFileDraft,touchFileDraft,discardFileDraft,fileDrafts,fileKey,observeFileDraftChanges,type FileDraft} from '../files/drafts.svelte';
  import {emptyTextIndex,indexText,prefixTextIndex,TEXT_PAGE_UNITS,type TextIndex,type TextSnapshot} from '../files/text';
  import Markdown from './Markdown.svelte';
  import TextEditor from './TextEditor.svelte';
  import PdfViewer from './PdfViewer.svelte';
  import Icon from './Icon.svelte';
  import './editor.css';
  hljs.registerLanguage('py',python);hljs.registerLanguage('rs',rust);hljs.registerLanguage('js',javascript);hljs.registerLanguage('ts',typescript);hljs.registerLanguage('json',json);
  let {path,project}:{path:string;project:string}=$props();
  let snapshot=$state<TextSnapshot>(),snapshotAuthoritative=$state(false),draft=$state<FileDraft>(),error=$state(''),loading=$state(false),preparing=$state(false),preview=$state(true),zoom=$state(100),page=$state(1);
  let textIndex=$state<TextIndex>(emptyTextIndex()),reloadController:AbortController|undefined,preparationController:AbortController|undefined;
  let reading=$state(false),readBytes=$state(0),verifying=$state(false),retryRead=$state(0);
  let ext=$derived(path.split('.').pop()?.toLowerCase()??'');
  let kind=$derived(ext==='pdf'?'pdf':['png','jpg','jpeg','webp','svg','gif','avif','bmp','ico','apng'].includes(ext)?'image':['mp3','wav','ogg','opus','flac','m4a','aac','weba'].includes(ext)?'audio':'text');
  let dirty=$derived(!!draft&&(draft.baselineKnown===false||draft.text!==draft.original));
  let text=$derived(draft&&(dirty||draft.editing)?draft.text:snapshot?.text??'');
  let pages=$derived(textIndex.pages),large=$derived(text.length>TEXT_PAGE_UNITS);
  let availablePages=$derived(textIndex.complete?pages.length:Math.max(1,pages.length-1));
  let pageReady=$derived(!large||textIndex.complete||page<pages.length);
  let source=$derived(pageReady?text.slice(pages.at(page-1)?.start??0,pages.at(page)?.start??text.length):'');
  let firstLine=$derived(pages.at(page-1)?.line??1);
  let highlighted=$derived(textIndex.complete&&!large&&hljs.getLanguage(ext)?hljs.highlight(source,{language:ext}).value:'');
  $effect(()=>{
    void retryRead;
    const current=path,id=project,owner=browserStorage.workspace;preview=ui.preferences.markdown_preview;error='';loading=false;reading=false;readBytes=0;verifying=false;preparing=false;preparationController=undefined;snapshot=undefined;snapshotAuthoritative=false;draft=undefined;zoom=100;page=1;
    let cancelled=false;const controller=new AbortController();
    if(kind==='text'){
      loading=true;reading=true;restoreFileDraft(id,current).then(()=>{
        if(cancelled||owner!==browserStorage.workspace)return;
        const retained=fileDrafts[fileKey(id,current)];
        if(retained){draft=retained;loading=false;snapshot={text:retained.text,revision:retained.revision,bytes:retained.bytes??retained.text.length};}
        return readText(id,current,progress=>{
          if(cancelled||owner!==browserStorage.workspace)return;
          readBytes=progress.bytes;verifying=progress.verifying;
          if(!retained){snapshot={text:progress.text,revision:'',bytes:progress.bytes};loading=false;}
        },{signal:controller.signal});
      }).then(value=>{if(!cancelled&&owner===browserStorage.workspace&&value){snapshot=value;snapshotAuthoritative=true;}})
        .catch(e=>{if(!cancelled&&!aborted(e))error=e instanceof Error?e.message:String(e);}).finally(()=>{if(!cancelled){loading=false;reading=false;}});
    }
    return()=>{cancelled=true;controller.abort();reloadController?.abort();preparationController?.abort();flushFileDraft(id,current);};
  });
  $effect(()=>{
    const current=draft;if(!current)return;
    return observeFileDraftChanges(current,(before,change)=>{
      textIndex=prefixTextIndex(textIndex,before,current.text,change);
    });
  });
  $effect(()=>{
    const indexing=kind==='text',value=indexing?text:'',controller=new AbortController();
    const retained=untrack(()=>textIndex),prefix=retained.source===value?retained:emptyTextIndex(value);
    textIndex=prefix;page=1;
    const timer=setTimeout(()=>{
      if(indexing)void indexText(value,next=>{if(!controller.signal.aborted&&next.source===text)textIndex=next;},controller.signal,TEXT_PAGE_UNITS,prefix)
        .catch(e=>{if(!aborted(e))error=e instanceof Error?e.message:String(e);});
    },0);
    return()=>{clearTimeout(timer);controller.abort();};
  });
  $effect(()=>{const current=draft;if(current){void current.version;persistFileDraft(project,path,current);}});
  $effect(()=>{if(page>availablePages)page=availablePages;});
  async function edit(){
    if((draft?.newline??textIndex.format.newline)==='mixed'){notify('Mixed line endings are view-only here to avoid rewriting unrelated lines. Use your external editor.',true);return;}
    if(!draft){
      const value=snapshot,id=project,name=path;
      if(!value||!snapshotAuthoritative||!textIndex.complete||textIndex.source!==value.text)throw new Error('Wait for the file to finish loading and checking before editing it.');
      const controller=new AbortController();preparationController?.abort();preparationController=controller;preparing=true;
      try{
        const retained=await retainFileIncrementally(id,name,value,textIndex.format,controller.signal);
        if(controller.signal.aborted||project!==id||path!==name)return;draft=retained;
      }catch(e){if(!aborted(e))throw e;else return;}
      finally{if(preparationController===controller){preparationController=undefined;preparing=false;}}
    }
    const current=draft;if(!current)return;current.editing=true;touchFileDraft(current);
  }
  async function save(){
    if(!draft||!dirty||draft.saving)return;
    const current=draft,id=project,name=path,content=fileBytes(current);current.saving=true;current.error='';touchFileDraft(current);ui.notice='';
    try{
      const result=await saveFile(id,name,current.revision,content);
      current.revision=result.revision;current.original=current.text;current.baselineKnown=true;current.bytes=result.bytes;
      if(project===id&&path===name){snapshot={...result,text:content};snapshotAuthoritative=true;}
      ui.gitRevision++;void loadGit();notify(`Saved ${name}`);
    }catch(e){current.error=e instanceof Error?e.message:String(e);}
    finally{current.saving=false;touchFileDraft(current);}
  }
  async function reload(){
    if(draft?.saving||dirty&&!confirm(`Discard your unsaved changes and reload ${path} from disk?`))return;
    const id=project,name=path,owner=browserStorage.workspace,controller=new AbortController();reloadController?.abort();reloadController=controller;error='';reading=true;
    try{
      const value=await readText(id,name,progress=>{
        if(controller.signal.aborted||owner!==browserStorage.workspace||project!==id||path!==name)return;
        readBytes=progress.bytes;verifying=progress.verifying;
      },{signal:controller.signal});
      if(controller.signal.aborted||owner!==browserStorage.workspace||project!==id||path!==name)return;
      discardFileDraft(id,name);snapshot=value;snapshotAuthoritative=true;draft=undefined;page=1;
    }catch(e){if(!aborted(e))throw e;}
    finally{if(reloadController===controller){reloadController=undefined;reading=false;}}
  }
  async function copyContents(){await navigator.clipboard.writeText(draft&&(dirty||!snapshotAuthoritative)?fileBytes(draft):snapshot?.text??'');notify('File contents copied.');}
  function aborted(error:unknown){return error instanceof Error&&error.name==='AbortError';}
  onDestroy(()=>{reloadController?.abort();preparationController?.abort();flushFileDraft(project,path);});
</script>
<section class="file-viewer" aria-label={`File viewer: ${path}`}>
  <div class="viewer-toolbar"><div class="breadcrumb"><Icon name={kind==='text'?'code':kind}/><span>{path}{#if dirty}<span class="file-unsaved" aria-label="Unsaved changes"> · unsaved</span>{/if}</span></div><div class="button-cluster">
    {#if kind==='text'}
      <div class="editor-mode" role="group" aria-label="File mode"><button class="key small" aria-pressed={!draft?.editing} onclick={()=>{if(draft){draft.editing=false;touchFileDraft(draft);}}}>View</button><button class="key small" aria-pressed={!!draft?.editing} disabled={preparing||!snapshot||(!draft&&(!textIndex.complete||!snapshotAuthoritative))} onclick={()=>void attempt(edit)}>{preparing?'Preparing…':'Edit'}</button></div>
      {#if draft?.editing}<button class="key small primary" disabled={!dirty||draft.saving} onclick={()=>void save()}>{draft.saving?'Saving…':'Save'}</button><button class="key small" disabled={draft.saving} onclick={()=>void attempt(reload)}>Revert / reload</button>
      {:else if ext==='md'&&!large}<button class="key small" aria-pressed={preview} onclick={()=>preview=!preview}>{preview?'View source':'Read Markdown'}</button>{/if}
      <button class="flat icon-button" aria-label="Copy file contents" disabled={!draft&&!snapshotAuthoritative} onclick={()=>void attempt(copyContents)}><Icon name="file" size={16}/></button>
    {/if}
    {#if kind==='pdf'}<a class="key small" href={pdfUrl(project,path)} target="_blank" rel="noopener noreferrer">Open PDF in new tab<Icon name="arrow" size={16}/></a>{/if}
    <a class="key small" href={rawUrl(project,path,true)} download><Icon name="download" size={16}/>Download</a></div></div>
  {#if draft?.error||error&&draft}<div class="editor-notice error" role="alert"><span>{draft?.error||error}</span><button class="key small" onclick={()=>void attempt(reload)}>Reload disk version</button></div>
  {:else if dirty&&!draft?.editing}<div class="editor-notice">Previewing your unsaved draft. Return to Edit to save it.</div>{/if}
  {#if reading}<div class="editor-notice" role="status">{verifying?'Checking the file for changes':'Reading file'} · {(readBytes/1024).toLocaleString(undefined,{maximumFractionDigits:1})} KiB received. Preview the first page while loading continues.</div>{/if}
  {#if large&&!draft?.editing}<div class="preview-pages"><span>Large file · plain-text pages{textIndex.complete?'':' · indexing'}</span><button class="key small" disabled={page<=1} onclick={()=>page--}>Previous page</button><label>Page <input type="number" aria-label="Preview page" min="1" max={availablePages} value={page} onchange={event=>page=Math.max(1,Math.min(availablePages,Number(event.currentTarget.value)||1))}/> of {textIndex.complete?pages.length:`${availablePages}+`}</label><button class="key small" disabled={page>=availablePages} onclick={()=>page++}>Next page</button></div>{/if}
  <div class="viewer-content" class:editing={draft?.editing} class:code-view={kind==='text'&&!(ext==='md'&&preview&&!large)}>
    {#if error&&!draft}<div class="viewer-empty"><Icon name="file" size={36}/><h2>Preview unavailable</h2><p>{error}</p>{#if kind==='text'}<button class="key" onclick={()=>retryRead++}>Retry preview</button>{/if}<a class="key" href={rawUrl(project,path,true)} download>Download this file</a></div>
    {:else if loading}<div class="skeleton-lines" aria-label="Loading file"><i></i><i></i><i></i></div>
    {:else if kind==='pdf'}{#key `${project}:${path}`}<PdfViewer {project} {path}/>{/key}
    {:else if kind==='image'}<div class="image-stage"><img src={rawUrl(project,path)} alt={`Project image: ${path}`} style={`width:${zoom}%;max-width:none`} onerror={()=>error='Your browser could not decode this image. Download it to open in another viewer.'}/></div>
    {:else if kind==='audio'}<div class="audio-stage"><Icon name="audio" size={54}/><h2>{path.split('/').pop()}</h2><audio controls preload="metadata" src={rawUrl(project,path)} onerror={()=>error='Your browser does not support this audio encoding. Download it to use another player.'}></audio><p>Local project audio · native playback controls</p></div>
    {:else if draft?.editing}{#key draft}<TextEditor {draft} {path} index={textIndex} onsave={save}/>{/key}
    {:else if !pageReady}<div class="skeleton-lines" role="status" aria-label="Indexing file page"><i></i><i></i><i></i></div>
    {:else if ext==='md'&&preview&&!large&&textIndex.complete}<Markdown {text}/>
    {:else}<div class="source-lines"><div class="line-numbers" aria-hidden="true">{Array.from({length:source.split('\n').length},(_,i)=>i+firstLine).join('\n')}</div>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (The overflowing read-only source region must be keyboard-scrollable.) -->
      <pre class:wrap={ui.preferences.word_wrap} role="region" tabindex="0" aria-label="File source">{#if highlighted}<code>{@html highlighted}</code>{:else}<code>{source}</code>{/if}</pre>
    </div>{/if}
  </div>
  <div class="viewer-foot"><span>{dirty?'Unsaved changes':draft?.editing?'Local file editing':'View only'} · not attached to chat</span>{#if kind==='image'}<label>Zoom <input aria-label="Image zoom" type="range" min="25" max="200" step="25" bind:value={zoom}/>{zoom}%</label>{:else if kind==='text'}<span>{textIndex.format.lines.toLocaleString()}{textIndex.complete?'':'+'} lines · UTF-8 · {(draft?.newline??textIndex.format.newline).toUpperCase()} · {((snapshot?.bytes??draft?.bytes??0)/1024).toLocaleString(undefined,{maximumFractionDigits:1})} KiB</span>{/if}</div>
</section>
