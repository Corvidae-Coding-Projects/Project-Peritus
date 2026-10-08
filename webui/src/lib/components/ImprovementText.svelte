<script lang="ts">
  import { query } from '../api';
  import { browserStorage } from '../storage.svelte';
  import type { ImprovementTextPage, ImprovementTextReference } from '../types';

  let {project,candidate,run='',reference}: {
    project:string;candidate:string;run?:string;reference:ImprovementTextReference
  } = $props();
  let offset=$state('0'), next=$state<string|null>(null);
  let history=$state<string[]>([]), text=$state('');
  let loading=$state(false), error=$state(''), retry=$state(0);
  let selected='';
  let generation=0;

  $effect(()=>{
    const owner=[browserStorage.workspace,project,candidate,run,reference.source].join('\0');
    if(owner!==selected){selected=owner;offset='0';history=[];text='';next=null;}
    void retry;
    const position=offset, current=++generation;
    const controller=new AbortController();
    loading=true;error='';
    const owns=()=>current===generation&&!controller.signal.aborted
      &&owner===[browserStorage.workspace,project,candidate,run,reference.source].join('\0');
    void query<ImprovementTextPage>('improvement-text',{
      project,candidate,run,source:reference.source,offset:position
    },{signal:controller.signal}).then(page=>{
      if(!owns())return;
      const end=BigInt(position)+BigInt(new TextEncoder().encode(page.text).byteLength);
      const size=BigInt(reference.bytes);
      if(page.offset!==position||end>size
        ||(page.next===null?end!==size:BigInt(page.next)!==end||end<=BigInt(position)))
        throw new Error('The daemon returned an inconsistent text slice.');
      text=page.text;next=page.next;
    }).catch(value=>{
      if(owns())error=value instanceof Error?value.message:String(value);
    }).finally(()=>{if(owns())loading=false;});
    return ()=>{controller.abort();};
  });

  function advance(){
    if(next===null||loading)return;
    history.push(offset);offset=next;
  }
  function previous(){
    if(!history.length||loading)return;
    offset=history.pop()!;
  }
</script>

{#if error}<p role="alert">{error}</p><button class="flat" onclick={()=>retry++}>Retry reading</button>{/if}
{#if loading}<p class="setting-note">Reading text…</p>{:else if !error}<pre>{text}</pre>{/if}
{#if history.length||next!==null}<div class="text-pages">
  <button class="flat" disabled={loading||!history.length} onclick={previous}>Previous text</button>
  <span class="setting-note">Byte {offset} of {reference.bytes}</span>
  <button class="flat" disabled={loading||next===null} onclick={advance}>Next text</button>
</div>{/if}

<style>
  pre{white-space:pre-wrap;overflow-wrap:anywhere;max-height:320px;overflow:auto;margin:12px 0}
  .text-pages{display:flex;flex-wrap:wrap;align-items:center;gap:10px}
</style>
