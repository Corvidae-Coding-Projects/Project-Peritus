<script lang="ts">
  import { query, rawUrl, action } from '../api';
  import { ui, project, openFile, attachFile, attempt, notify, loadGit, gitAction } from '../workspace.svelte';
  import type { Directory, Entry } from '../types';
  import FileNode from './FileNode.svelte';
  import Icon from './Icon.svelte';
  let entries=$state<Entry[]>([]),filter=$state(''),next=$state<number|null>(null),loading=$state(false),error=$state('');
  let selected=$state<Entry|null>(null),menu:HTMLDialogElement,ignorePreview=$state('');
  let projectId=$derived(ui.projectId);
  async function load(offset=0){const id=projectId;if(!id)return;loading=true;error='';try{const page=await query<Directory>('files',{project:id,path:'',offset});if(id===projectId){entries=offset?[...entries,...page.entries]:page.entries;next=page.next;}}catch(e){if(id===projectId)error=String(e);}finally{if(id===projectId)loading=false;}}
  $effect(()=>{const id=projectId;const revision=ui.gitRevision;entries=[];next=null;error='';loading=false;if(id){void revision;void load();}});
  function context(entry:Entry){selected=entry;ignorePreview='';menu.showModal();}
  async function stage(){if(!selected||!ui.git)return;const full=`${project()?.root}/${selected.path}`;const prefix=ui.git.root+'/';if(!full.startsWith(prefix))throw new Error('This file is outside the selected Git repository.');await gitAction('add',[full.slice(prefix.length)]);menu.close();}
  async function ignore(){if(!selected)return;if(!ignorePreview){const value=await query<{pattern:string}>('ignore',{project:projectId,path:selected.path});ignorePreview=value.pattern;return;}await action('ignore',{project:projectId,path:selected.path});menu.close();notify('Exact entry added to .gitignore.');await loadGit();ui.gitRevision++;}
</script>
<section class="explorer" aria-label="Project file explorer">
  <div class="drawer-heading"><h2>Project files</h2><button class="flat icon-button" aria-label="Refresh project files" onclick={()=>void load()}><Icon name="refresh" size={16}/></button></div>
  <div class="filter-field"><Icon name="search" size={15}/><input aria-label="Filter root entries" placeholder="Filter root entries…" bind:value={filter}/><kbd>/</kbd></div>
  <div class="root-label"><Icon name="folder" size={15}/><span title={project()?.root}>{project()?.name}</span><span class="root-marker">ROOT</span></div>
  <div class="tree-scroll">
    {#if error}<div class="inline-error"><p>{error}</p><button onclick={()=>void load()}>Try again</button></div>{/if}
    {#if loading&&!entries.length}<div class="skeleton-lines" aria-label="Loading project files"><i></i><i></i><i></i><i></i></div>{/if}
    <ul class="file-tree">{#each entries.filter(e=>e.name.toLowerCase().includes(filter.toLowerCase())) as entry(entry.path)}<FileNode {entry} project={projectId} onmenu={context}/>{/each}</ul>
    {#if !loading&&!entries.length&&!error}<p class="small-empty">{projectId?'This directory is empty. Files appear here as you create them.':'Open a project to browse files.'}</p>{/if}
    {#if next!==null}<button class="load-more" onclick={()=>void load(next!)}>Load next 250 entries</button>{/if}
  </div>
  <div class="drawer-foot"><span class="status-dot neutral"></span>Hidden & ignored files included</div>
</section>
<dialog bind:this={menu} class="context-dialog" aria-label="File actions" onclick={(event)=>{if(event.target===menu)menu.close();}}>
  {#if selected}
    <div class="dialog-heading"><strong>{selected.name}</strong><button class="flat icon-button" aria-label="Close file actions" onclick={()=>menu.close()}><Icon name="close"/></button></div>
    <div class="menu-actions">
      {#if !selected.directory}<button onclick={()=>{openFile(selected!.path);menu.close();}}><Icon name="eye"/>Open in file tab</button><button disabled={!ui.sessionId} onclick={()=>{const path=selected!.path;menu.close();void attempt(()=>attachFile(path));}}><Icon name="plus"/>Attach to next message</button><a href={rawUrl(projectId,selected.path,true)} download><Icon name="download"/>Download file</a>{/if}
      <button onclick={()=>void attempt(async()=>{await navigator.clipboard.writeText(selected!.path);menu.close();notify('Relative file path copied.');})}><Icon name="file"/>Copy relative path</button>
      <button disabled={!ui.git||ui.gitBusy} onclick={()=>void attempt(stage)}><Icon name="plus"/>Stage in Git</button>
      <button disabled={!ui.git} onclick={()=>void attempt(ignore)}><Icon name="git"/>{ignorePreview?'Add this exact rule':'Add to .gitignore…'}</button>
    </div>
    {#if ignorePreview}<div class="ignore-preview"><code>{ignorePreview}</code><p>Added to the selected repository’s .gitignore. Already tracked files remain tracked.</p></div>{/if}
  {/if}
</dialog>
