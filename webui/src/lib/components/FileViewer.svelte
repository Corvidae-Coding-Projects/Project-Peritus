<script lang="ts">
  import hljs from 'highlight.js/lib/core';
  import python from 'highlight.js/lib/languages/python';
  import rust from 'highlight.js/lib/languages/rust';
  import javascript from 'highlight.js/lib/languages/javascript';
  import typescript from 'highlight.js/lib/languages/typescript';
  import json from 'highlight.js/lib/languages/json';
  import {query,rawUrl,pdfUrl,saveFile} from '../api';
  import {ui,attempt,notify,loadGit} from '../workspace.svelte';
  import {fileDrafts,fileKey,retainFile,fileBytes,type FileDraft} from '../files/drafts.svelte';
  import {textFormat,pageStarts,type TextSnapshot} from '../files/text';
  import Markdown from './Markdown.svelte';
  import TextEditor from './TextEditor.svelte';
  import PdfViewer from './PdfViewer.svelte';
  import Icon from './Icon.svelte';
  import './editor.css';
  hljs.registerLanguage('py',python);hljs.registerLanguage('rs',rust);hljs.registerLanguage('js',javascript);hljs.registerLanguage('ts',typescript);hljs.registerLanguage('json',json);
  let {path,project}:{path:string;project:string}=$props();
  let snapshot=$state<TextSnapshot>(),draft=$state<FileDraft>(),error=$state(''),loading=$state(false),preview=$state(true),zoom=$state(100),page=$state(1);
  let ext=$derived(path.split('.').pop()?.toLowerCase()??'');
  let kind=$derived(ext==='pdf'?'pdf':['png','jpg','jpeg','webp','svg','gif','avif','bmp','ico','apng'].includes(ext)?'image':['mp3','wav','ogg','opus','flac','m4a','aac','weba'].includes(ext)?'audio':'text');
  let dirty=$derived(!!draft&&draft.text!==draft.original);
  let text=$derived(dirty&&draft?fileBytes(draft):snapshot?.text??'');
  let format=$derived(textFormat(text)),pages=$derived(pageStarts(text)),large=$derived(pages.length>1);
  let source=$derived(text.slice(pages[Math.min(page,pages.length)-1]??0,pages[page]??text.length));
  let firstLine=$derived.by(()=>{let count=1;for(let i=0;i<(pages[page-1]??0);i++)if(text[i]==='\n')count++;return count;});
  let highlighted=$derived(!large&&hljs.getLanguage(ext)?hljs.highlight(source,{language:ext}).value:'');
  $effect(()=>{
    const current=path,id=project;preview=ui.preferences.markdown_preview;error='';loading=false;snapshot=undefined;draft=undefined;zoom=100;page=1;
    let cancelled=false;
    if(kind==='text'){
      loading=true;query<TextSnapshot>('text',{project:id,path:current}).then(value=>{if(!cancelled){snapshot=value;draft=retainFile(id,current,value);}})
        .catch(e=>{if(!cancelled)error=e.message;}).finally(()=>{if(!cancelled)loading=false;});
    }
    return()=>{cancelled=true;};
  });
  function edit(){
    if(!draft)return;
    if(draft.newline==='mixed'){notify('Mixed line endings are view-only here to avoid rewriting unrelated lines. Use your external editor.',true);return;}
    if((snapshot?.bytes??0)>2*1024*1024&&!confirm('Editing a large file can slow your browser. This editor is intended for small textual changes. Continue?'))return;
    draft.editing=true;
  }
  async function save(){
    if(!draft||!dirty||draft.saving)return;
    const current=draft,id=project,name=path,content=fileBytes(current);current.saving=true;current.error='';ui.notice='';
    try{
      const result=await saveFile(id,name,current.revision,content);
      current.revision=result.revision;current.original=current.text;
      if(project===id&&path===name)snapshot={...result,text:content};
      ui.gitRevision++;void loadGit();notify(`Saved ${name}`);
    }catch(e){current.error=e instanceof Error?e.message:String(e);}
    finally{current.saving=false;}
  }
  async function reload(){
    if(draft?.saving||dirty&&!confirm(`Discard your unsaved changes and reload ${path} from disk?`))return;
    const id=project,name=path;
    const value=await query<TextSnapshot>('text',{project:id,path:name});
    delete fileDrafts[fileKey(id,name)];const fresh=retainFile(id,name,value);
    if(project===id&&path===name){snapshot=value;draft=fresh;page=1;}
  }
</script>
<section class="file-viewer" aria-label={`File viewer: ${path}`}>
  <div class="viewer-toolbar"><div class="breadcrumb"><Icon name={kind==='text'?'code':kind}/><span>{path}{#if dirty}<span class="file-unsaved" aria-label="Unsaved changes"> · unsaved</span>{/if}</span></div><div class="button-cluster">
    {#if kind==='text'&&draft}
      <div class="editor-mode" role="group" aria-label="File mode"><button class="key small" aria-pressed={!draft.editing} onclick={()=>{if(draft)draft.editing=false;}}>View</button><button class="key small" aria-pressed={draft.editing} onclick={edit}>Edit</button></div>
      {#if draft.editing}<button class="key small primary" disabled={!dirty||draft.saving} onclick={()=>void save()}>{draft.saving?'Saving…':'Save'}</button><button class="key small" disabled={draft.saving} onclick={()=>void attempt(reload)}>Revert / reload</button>
      {:else if ext==='md'&&!large}<button class="key small" aria-pressed={preview} onclick={()=>preview=!preview}>{preview?'View source':'Read Markdown'}</button>{/if}
      <button class="flat icon-button" aria-label="Copy file contents" onclick={()=>void attempt(async()=>{await navigator.clipboard.writeText(text);notify('File contents copied.');})}><Icon name="file" size={16}/></button>
    {/if}
    {#if kind==='pdf'}<a class="key small" href={pdfUrl(project,path)} target="_blank" rel="noopener noreferrer">Open PDF in new tab<Icon name="arrow" size={16}/></a>{/if}
    <a class="key small" href={rawUrl(project,path,true)} download><Icon name="download" size={16}/>Download</a></div></div>
  {#if draft?.error}<div class="editor-notice error" role="alert"><span>{draft.error}</span><button class="key small" onclick={()=>void attempt(reload)}>Reload disk version</button></div>
  {:else if dirty&&!draft?.editing}<div class="editor-notice">Previewing your unsaved draft. Return to Edit to save it.</div>{/if}
  {#if large&&!draft?.editing}<div class="preview-pages"><span>Large file · plain-text pages</span><button class="key small" disabled={page<=1} onclick={()=>page--}>Previous page</button><label>Page <input type="number" aria-label="Preview page" min="1" max={pages.length} value={page} onchange={event=>page=Math.max(1,Math.min(pages.length,Number(event.currentTarget.value)||1))}/> of {pages.length}</label><button class="key small" disabled={page>=pages.length} onclick={()=>page++}>Next page</button></div>{/if}
  <div class="viewer-content" class:editing={draft?.editing} class:code-view={kind==='text'&&!(ext==='md'&&preview&&!large)}>
    {#if error}<div class="viewer-empty"><Icon name="file" size={36}/><h2>Preview unavailable</h2><p>{error}</p><a class="key" href={rawUrl(project,path,true)} download>Download this file</a></div>
    {:else if loading}<div class="skeleton-lines" aria-label="Loading file"><i></i><i></i><i></i></div>
    {:else if kind==='pdf'}{#key `${project}:${path}`}<PdfViewer {project} {path}/>{/key}
    {:else if kind==='image'}<div class="image-stage"><img src={rawUrl(project,path)} alt={`Project image: ${path}`} style={`width:${zoom}%;max-width:none`} onerror={()=>error='Your browser could not decode this image. Download it to open in another viewer.'}/></div>
    {:else if kind==='audio'}<div class="audio-stage"><Icon name="audio" size={54}/><h2>{path.split('/').pop()}</h2><audio controls preload="metadata" src={rawUrl(project,path)} onerror={()=>error='Your browser does not support this audio encoding. Download it to use another player.'}></audio><p>Local project audio · native playback controls</p></div>
    {:else if draft?.editing}{#key draft}<TextEditor {draft} {path} onsave={save}/>{/key}
    {:else if ext==='md'&&preview&&!large}<Markdown {text}/>
    {:else}<div class="source-lines"><div class="line-numbers" aria-hidden="true">{Array.from({length:source.split('\n').length},(_,i)=>i+firstLine).join('\n')}</div>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (The overflowing read-only source region must be keyboard-scrollable.) -->
      <pre class:wrap={ui.preferences.word_wrap} role="region" tabindex="0" aria-label="File source">{#if highlighted}<code>{@html highlighted}</code>{:else}<code>{source}</code>{/if}</pre>
    </div>{/if}
  </div>
  <div class="viewer-foot"><span>{dirty?'Unsaved changes':draft?.editing?'Local file editing':'View only'} · not attached to chat</span>{#if kind==='image'}<label>Zoom <input aria-label="Image zoom" type="range" min="25" max="200" step="25" bind:value={zoom}/>{zoom}%</label>{:else if kind==='text'}<span>{format.lines.toLocaleString()} lines · UTF-8 · {((snapshot?.bytes??0)/1024).toLocaleString(undefined,{maximumFractionDigits:1})} KiB</span>{/if}</div>
</section>
