<script lang="ts">
  import { onDestroy } from 'svelte';
  import { query, rawUrl, action } from '../api';
  import { DIRECTORY_PAGE_SIZE, reconcileDirectoryPage, reconcileDirectoryProgress, waitForDirectoryPoll } from '../files/directory';
  import { ui, project, openFile, attachFile, attempt, notify, loadGit, gitAction } from '../workspace.svelte';
  import type { Directory, Entry } from '../types';
  import FileNode from './FileNode.svelte';
  import Icon from './Icon.svelte';

  interface RootRequest {
    generation: number;
    project: string;
    directory: string | null;
    filter: string;
    offset: number;
    cursor: string;
    snapshot: string;
  }

  let entries=$state<Entry[]>([]),filter=$state(''),next=$state<string|null>(null),loading=$state(false),error=$state(''),indexed=$state(0);
  let directoryIdentity=$state<string|null>(null),snapshot=$state(''),activeRequest=$state('');
  let treeGeneration=$state(0),refreshGeneration=$state(0),generationSequence=0,disposed=false;
  let entryIdentities=new Set<string>(),controller:AbortController|null=null;
  let selected=$state<Entry|null>(null),menu:HTMLDialogElement,ignorePreview=$state('');
  let projectId=$derived(ui.projectId);
  let rootAncestors=$derived(directoryIdentity===null?[]:[directoryIdentity]);
  let previousProject='',previousFilter='';

  function requestKey(request:RootRequest):string {
    return JSON.stringify([request.generation,request.project,request.directory,request.filter,request.offset,request.cursor,request.snapshot]);
  }
  function isCurrent(request:RootRequest,key:string):boolean {
    return !disposed&&request.generation===treeGeneration&&request.project===projectId&&request.filter===filter&&activeRequest===key;
  }
  async function loadPage(request:RootRequest,key:string,owner:AbortController){
    try{
      let cursor=request.cursor,observedDirectory=request.directory;
      for(;;){
        const page=await query<Directory>('files',{project:request.project,path:'',directory:observedDirectory??'',filter:request.filter,offset:request.offset,cursor,snapshot:request.snapshot},{signal:owner.signal});
        if(!isCurrent(request,key))return;
        indexed=page.indexed;
        if(!page.ready){
          cursor=reconcileDirectoryProgress({...request,directory:observedDirectory},page,cursor);observedDirectory=page.directory;directoryIdentity=page.directory;
          await waitForDirectoryPoll(owner.signal);
          if(!isCurrent(request,key))return;
          continue;
        }
        const reconciled=reconcileDirectoryPage(entries,entryIdentities,{...request,directory:observedDirectory,cursor},page);
        if(request.offset===0)entryIdentities.clear();
        for(const identity of reconciled.addedIdentities)entryIdentities.add(identity);
        directoryIdentity=reconciled.directory;entries=reconciled.entries;next=reconciled.next;snapshot=reconciled.snapshot;
        return;
      }
    }catch(value){if(!owner.signal.aborted&&isCurrent(request,key))error=String(value);}
    finally{if(isCurrent(request,key)){activeRequest='';loading=false;if(controller===owner)controller=null;}}
  }
  function loadMore(){
    const id=projectId;
    if(!id||loading||next===null||directoryIdentity===null||!snapshot)return;
    const request:RootRequest={generation:treeGeneration,project:id,directory:directoryIdentity,filter,offset:entries.length,cursor:next,snapshot};
    const key=requestKey(request);controller?.abort();const owner=new AbortController();controller=owner;activeRequest=key;loading=true;error='';void loadPage(request,key,owner);
  }
  function refresh(){refreshGeneration+=1;}
  $effect(()=>{
    const id=projectId,revision=ui.gitRevision,requestedFilter=filter,refreshRequest=refreshGeneration;
    void revision;void refreshRequest;
    controller?.abort();controller=null;
    const preserve=id===previousProject&&requestedFilter===previousFilter;
    previousProject=id;previousFilter=requestedFilter;
    const generation=++generationSequence;treeGeneration=generation;next=null;snapshot='';error='';indexed=0;
    if(!preserve){entries=[];entryIdentities.clear();directoryIdentity=null;selected=null;}
    if(!id){activeRequest='';loading=false;return;}
    const request:RootRequest={generation,project:id,directory:null,filter:requestedFilter,offset:0,cursor:'',snapshot:''};
    const key=requestKey(request),owner=new AbortController();controller=owner;activeRequest=key;loading=true;
    const timer=setTimeout(()=>void loadPage(request,key,owner),requestedFilter?150:0);
    return ()=>{clearTimeout(timer);owner.abort();};
  });
  onDestroy(()=>{disposed=true;controller?.abort();});
  function context(entry:Entry){selected=entry;ignorePreview='';menu.showModal();}
  async function stage(){if(!selected||!ui.git)return;const full=`${project()?.root}/${selected.path}`;const prefix=ui.git.root+'/';if(!full.startsWith(prefix))throw new Error('This file is outside the selected Git repository.');await gitAction('add',[full.slice(prefix.length)]);menu.close();}
  async function ignore(){if(!selected)return;if(!ignorePreview){const value=await query<{pattern:string}>('ignore',{project:projectId,path:selected.path});ignorePreview=value.pattern;return;}await action('ignore',{project:projectId,path:selected.path});menu.close();notify('Exact entry added to .gitignore.');await loadGit();ui.gitRevision++;}
</script>
<section class="explorer" aria-label="Project file explorer">
  <div class="drawer-heading"><h2>Project files</h2><button class="flat icon-button" aria-label="Refresh project files" onclick={refresh}><Icon name="refresh" size={16}/></button></div>
  <div class="filter-field"><Icon name="search" size={15}/><input aria-label="Filter root directory" placeholder="Filter root directory…" bind:value={filter}/><kbd>/</kbd></div>
  <div class="root-label"><Icon name="folder" size={15}/><span title={project()?.root}>{project()?.name}</span><span class="root-marker">ROOT</span></div>
  <div class="tree-scroll">
    {#if error}<div class="inline-error"><p>{error}</p><button onclick={refresh}>Try again</button></div>{/if}
    {#if loading&&!entries.length}<div class="skeleton-lines" aria-label="Loading project files"><i></i><i></i><i></i><i></i></div>{#if indexed}<p class="small-empty">Indexing {indexed.toLocaleString()} entries…</p>{/if}{/if}
    <ul class="file-tree">{#each entries as entry(entry.path)}<FileNode {entry} project={projectId} generation={treeGeneration} ancestors={rootAncestors} onmenu={context}/>{/each}</ul>
    {#if !loading&&!entries.length&&!error}<p class="small-empty">{projectId?(filter?'No entries match this filter.':'This directory is empty. Files appear here as you create them.'):'Open a project to browse files.'}</p>{/if}
    {#if next!==null}<button class="load-more" disabled={loading} onclick={loadMore}>Load next {DIRECTORY_PAGE_SIZE} entries</button>{/if}
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
