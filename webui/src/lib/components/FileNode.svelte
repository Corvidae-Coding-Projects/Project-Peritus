<script lang="ts">
  import { query } from '../api';
  import { openFile, notify, ui } from '../workspace.svelte';
  import type { Directory, Entry } from '../types';
  import Icon from './Icon.svelte';
  import FileNode from './FileNode.svelte';
  let { entry, project, depth=0, onmenu }: {entry:Entry;project:string;depth?:number;onmenu:(entry:Entry,event:MouseEvent|KeyboardEvent)=>void}=$props();
  let expanded=$state(false),loading=$state(false),children=$state<Entry[]>([]),next=$state<number|null>(null);
  async function load(offset=0){loading=true;try{const page=await query<Directory>('files',{project,path:entry.path,offset});children=offset?[...children,...page.entries]:page.entries;next=page.next;}catch(error){notify(String(error),true);}finally{loading=false;}}
  async function activate(){if(entry.directory){expanded=!expanded;if(expanded&&!children.length)await load();}else openFile(entry.path);}
  let icon=$derived(entry.directory?'folder':/\.(png|jpe?g|webp|svg|gif|avif)$/i.test(entry.name)?'image':/\.(mp3|ogg|wav|flac|opus)$/i.test(entry.name)?'audio':/\.(rs|py|js|ts|tsx|json|toml)$/i.test(entry.name)?'code':'file');
</script>
<li>
  <div class="file-row" class:selected={ui.activeFile===entry.path} style={`--indent:${depth}`}>
    <button class="file-label" title={entry.directory?entry.path:`${entry.path} · Open to view; drag into chat to attach`} aria-expanded={entry.directory?expanded:undefined} onclick={()=>void activate()}
      draggable={!entry.directory} ondragstart={event=>{if(!entry.directory&&event.dataTransfer){event.dataTransfer.effectAllowed='copy';event.dataTransfer.setData('application/x-peritus-file',JSON.stringify({project,path:entry.path}));}}}
      oncontextmenu={(event)=>{event.preventDefault();onmenu(entry,event);}}
      onkeydown={(event)=>{if(event.key==='F10'&&event.shiftKey){event.preventDefault();onmenu(entry,event);}if(entry.directory&&event.key==='ArrowRight'&&!expanded){event.preventDefault();void activate();}if(entry.directory&&event.key==='ArrowLeft'&&expanded){event.preventDefault();expanded=false;}}}>
      <span class:expanded class="file-caret">{#if entry.directory}<Icon name="chevron" size={12}/>{/if}</span>
      <Icon name={icon} size={16}/><span class="file-name">{entry.name}</span>{#if entry.symlink}<Icon name="link" size={12}/>{/if}
    </button>
    <button class="file-more flat icon-button" aria-label={`Actions for ${entry.name}`} onclick={(event)=>onmenu(entry,event)}><Icon name="more" size={16}/></button>
  </div>
  {#if expanded}
    {#if loading&&!children.length}<p class="tree-loading">Loading directory…</p>{/if}
    <ul class="file-tree">{#each children as child (child.path)}<FileNode entry={child} {project} depth={depth+1} {onmenu}/>{/each}</ul>
    {#if next!==null}<button class="load-more flat" onclick={()=>void load(next!)}>Load more files</button>{/if}
    {#if !loading&&!children.length}<p class="tree-loading">Empty directory</p>{/if}
  {/if}
</li>
