<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { query } from '../api';
  import { reconcileDirectoryPage, reconcileDirectoryProgress, waitForDirectoryPoll } from '../files/directory';
  import { openFile, notify, ui } from '../workspace.svelte';
  import type { Directory, Entry } from '../types';
  import Icon from './Icon.svelte';
  import FileNode from './FileNode.svelte';

  interface NodeRequest {
    nodeGeneration:number;
    treeGeneration:number;
    project:string;
    path:string;
    directory:string|null;
    filter:string;
    offset:number;
    cursor:string;
    snapshot:string;
  }

  let { entry, project, generation, ancestors, depth=0, onmenu }: {entry:Entry;project:string;generation:number;ancestors:string[];depth?:number;onmenu:(entry:Entry,event:MouseEvent|KeyboardEvent)=>void}=$props();
  let expanded=$state(false),loading=$state(false),children=$state<Entry[]>([]),next=$state<string|null>(null),error=$state(''),indexed=$state(0);
  let directoryIdentity=$state<string|null>(entry.directoryIdentity),snapshot=$state(''),activeRequest=$state('');
  let nodeGeneration=$state(0),generationSequence=0,disposed=false;
  let childIdentities=new Set<string>(),controller:AbortController|null=null;
  let cycle=$derived(entry.directory&&entry.directoryIdentity!==null&&ancestors.includes(entry.directoryIdentity));
  let childAncestors=$derived(directoryIdentity===null?ancestors:[...ancestors,directoryIdentity]);

  function requestKey(request:NodeRequest):string {
    return JSON.stringify([request.nodeGeneration,request.treeGeneration,request.project,request.path,request.directory,request.offset,request.cursor,request.snapshot]);
  }
  function isCurrent(request:NodeRequest,key:string):boolean {
    return !disposed&&request.nodeGeneration===nodeGeneration&&request.treeGeneration===generation&&request.project===project&&request.path===entry.path&&activeRequest===key;
  }
  async function loadPage(request:NodeRequest,key:string,owner:AbortController){
    try{
      let cursor=request.cursor,observedDirectory=request.directory;
      for(;;){
        const page=await query<Directory>('files',{project:request.project,path:request.path,directory:observedDirectory??'',filter:'',offset:request.offset,cursor,snapshot:request.snapshot},{signal:owner.signal});
        if(!isCurrent(request,key))return;
        indexed=page.indexed;
        if(!page.ready){
          cursor=reconcileDirectoryProgress({...request,directory:observedDirectory},page,cursor);observedDirectory=page.directory;directoryIdentity=page.directory;
          await waitForDirectoryPoll(owner.signal);
          if(!isCurrent(request,key))return;
          continue;
        }
        const reconciled=reconcileDirectoryPage(children,childIdentities,{...request,directory:observedDirectory,cursor},page);
        if(request.offset===0)childIdentities.clear();
        for(const identity of reconciled.addedIdentities)childIdentities.add(identity);
        directoryIdentity=reconciled.directory;children=reconciled.entries;next=reconciled.next;snapshot=reconciled.snapshot;
        return;
      }
    }catch(value){if(!owner.signal.aborted&&isCurrent(request,key)){error=String(value);notify(error,true);}}
    finally{if(isCurrent(request,key)){activeRequest='';loading=false;if(controller===owner)controller=null;}}
  }
  function beginLoad(offset=0,cursor=''){
    if(loading||cycle)return;
    const request:NodeRequest={nodeGeneration,treeGeneration:generation,project,path:entry.path,directory:directoryIdentity,filter:'',offset,cursor,snapshot:offset?snapshot:''};
    const key=requestKey(request);controller?.abort();const owner=new AbortController();controller=owner;activeRequest=key;loading=true;error='';void loadPage(request,key,owner);
  }
  async function activate(){
    if(!entry.directory){openFile(entry.path);return;}
    if(cycle){notify('This directory resolves to an ancestor that is already open.',true);return;}
    expanded=!expanded;if(expanded&&!children.length&&!loading)beginLoad();
  }
  $effect(()=>{
    const expectedProject=project,expectedTreeGeneration=generation,expectedPath=entry.path,expectedDirectory=entry.directoryIdentity,isCycle=cycle;
    void expectedProject;void expectedTreeGeneration;void expectedPath;
    controller?.abort();controller=null;
    const reopen=untrack(()=>expanded),current=++generationSequence;nodeGeneration=current;
    children=[];childIdentities.clear();next=null;snapshot='';directoryIdentity=expectedDirectory;activeRequest='';error='';loading=false;indexed=0;
    if(isCycle){expanded=false;return;}
    if(reopen){
      loading=true;
      const request:NodeRequest={nodeGeneration:current,treeGeneration:expectedTreeGeneration,project:expectedProject,path:expectedPath,directory:expectedDirectory,filter:'',offset:0,cursor:'',snapshot:''};
      const key=requestKey(request),owner=new AbortController();controller=owner;activeRequest=key;
      const timer=setTimeout(()=>void loadPage(request,key,owner),0);
      return ()=>{clearTimeout(timer);owner.abort();};
    }
  });
  onDestroy(()=>{disposed=true;controller?.abort();});
  let icon=$derived(entry.directory?'folder':/\.(png|jpe?g|webp|svg|gif|avif)$/i.test(entry.name)?'image':/\.(mp3|ogg|wav|flac|opus)$/i.test(entry.name)?'audio':/\.(rs|py|js|ts|tsx|json|toml)$/i.test(entry.name)?'code':'file');
</script>
<li>
  <div class="file-row" class:selected={ui.activeFile===entry.path} style={`--indent:${depth}`}>
    <button class="file-label" title={cycle?`${entry.path} · Resolves to an open ancestor`:entry.directory?entry.path:`${entry.path} · Open to view; drag into chat to attach`} aria-expanded={entry.directory&&!cycle?expanded:undefined} aria-disabled={cycle||undefined} onclick={()=>void activate()}
      draggable={!entry.directory} ondragstart={event=>{if(!entry.directory&&event.dataTransfer){event.dataTransfer.effectAllowed='copy';event.dataTransfer.setData('application/x-peritus-file',JSON.stringify({project,path:entry.path}));}}}
      oncontextmenu={(event)=>{event.preventDefault();onmenu(entry,event);}}
      onkeydown={(event)=>{if(event.key==='F10'&&event.shiftKey){event.preventDefault();onmenu(entry,event);}if(entry.directory&&event.key==='ArrowRight'&&!expanded){event.preventDefault();void activate();}if(entry.directory&&event.key==='ArrowLeft'&&expanded){event.preventDefault();expanded=false;}}}>
      <span class:expanded class="file-caret">{#if entry.directory}<Icon name="chevron" size={12}/>{/if}</span>
      <Icon name={icon} size={16}/><span class="file-name">{entry.name}</span>{#if entry.symlink}<Icon name="link" size={12}/>{/if}
    </button>
    <button class="file-more flat icon-button" aria-label={`Actions for ${entry.name}`} onclick={(event)=>onmenu(entry,event)}><Icon name="more" size={16}/></button>
  </div>
  {#if expanded}
    {#if loading&&!children.length}<p class="tree-loading">{indexed?`Indexing ${indexed.toLocaleString()} entries…`:'Loading directory…'}</p>{/if}
    {#if error}<p class="tree-loading">{error} <button class="flat" onclick={()=>beginLoad()}>Try again</button></p>{/if}
    <ul class="file-tree">{#each children as child (child.path)}<FileNode entry={child} {project} {generation} ancestors={childAncestors} depth={depth+1} {onmenu}/>{/each}</ul>
    {#if next!==null}<button class="load-more flat" disabled={loading} onclick={()=>beginLoad(children.length,next!)}>Load more files</button>{/if}
    {#if !loading&&!children.length&&!error}<p class="tree-loading">Empty directory</p>{/if}
  {/if}
</li>
