<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { query, action } from '../api';
  import { ui, dispatch, attempt } from '../workspace.svelte';
  type Entry={id:string;name:string;version:string;description:string;digest:string;enabled?:boolean;bundled?:boolean;error?:string;tags?:string[]};
  type MenuAction={label:string;arguments:string;submit:boolean};
  type Market={session?:{module:string;digest:string;view:{actions:MenuAction[]}};repository:string;commit:string|null;notice?:string;installed:Entry[];modules:Entry[]};
  let market=$state<Market|null>(null),busy=$state(false),error=$state(''),filter=$state('');
  const rows=$derived.by(()=>{
    const all=new Map<string,{entry:Entry;installed?:Entry}>();
    for(const entry of market?.installed??[])all.set(entry.id,{entry,installed:entry});
    for(const entry of market?.modules??[])all.set(entry.id,{entry,installed:all.get(entry.id)?.installed});
    const q=filter.toLowerCase();return [...all.values()].filter(({entry})=>[entry.id,entry.name,entry.description,...entry.tags??[]].join(' ').toLowerCase().includes(q));
  });
  async function load(){market=await query<Market>('modules',{run:ui.conversations[ui.sessionId]?.run?.id??''});}
  async function change(operation:string,entry?:Entry){
    if(busy)return;busy=true;error='';
    try{await action('modules',{action:operation,module:entry?.id,digest:entry?.digest});await load();}
    catch(e){error=e instanceof Error?e.message:String(e);}finally{busy=false;}
  }
  onMount(()=>{void (async()=>{try{await load();if(!market?.commit)await change('refresh');}catch(e){error=String(e);}})();});
  async function use(entry:Entry){
    ui.overlay='';ui.activeFile='';ui.modes[ui.sessionId]='chat';
    ui.drafts[ui.sessionId]='/module '+entry.id+' ';
    await tick();document.getElementById('conversation-input')?.focus();
  }
  async function invoke(entry:Entry,item:MenuAction){if(item.submit){ui.overlay='';await dispatch('module',[entry.id,item.arguments]);}else{await use(entry);ui.drafts[ui.sessionId]='/module '+entry.id+' '+item.arguments;}}
</script>
<div class="module-market">
  <p>Install native Rust plugins that expand Peritus. Each plugin supplies its own code and actions. Install trusted code; native plugins run under your OS account. Agent access to host tools follows current permissions.</p>
  <div class="market-toolbar"><input aria-label="Search modules" bind:value={filter} placeholder="Search modules"/><button class="key" disabled={busy} onclick={()=>void change('refresh')}>{busy?'Working…':'Refresh market'}</button></div>
  {#if error}<p class="market-error" role="alert">{error}</p>{/if}
  {#if market?.notice}<p role="status">{market.notice}</p>{/if}
  {#each rows as {entry,installed}(entry.id)}
    <article class="market-module" aria-label={entry.name??entry.id}>
      <h3>{entry.name??entry.id} <small>{entry.version}</small></h3><p>{entry.description??installed?.error}</p>
      <code>{entry.id}</code><p class="setting-note">{installed?.bundled?'Bundled':installed?'Installed':'Available'} · {installed?.enabled?'Enabled':'Disabled'}{installed&&installed.digest!==entry.digest?' · Update available':''}</p>
      <p class="setting-note">{#if installed}Installed: {installed.version} · {installed.digest}<br/>{/if}{#if !installed?.bundled}Available: {entry.version} · {entry.digest}{/if}</p>
      {#if installed?.error}<p role="alert">{installed.error}</p>{/if}
      <div class="market-actions">
        {#if !installed||installed.digest!==entry.digest||installed.error}<button class="key" disabled={busy} onclick={()=>void change('install',entry)}>{installed?.error?'Repair install':installed?'Install update':'Install'} (disabled)</button>{/if}
        {#if installed}<button class="key" disabled={busy} onclick={()=>void change(installed.enabled?'disable':'enable',installed)}>{installed.enabled?'Disable':'Enable'}</button>
          {#if installed.enabled&&!installed.error}<button class="key primary" title={`Use installed version ${installed.version}`} onclick={()=>void use(installed)}>Use module</button>{/if}
          {#if !installed.bundled}<button class="key" disabled={busy} onclick={()=>void change('remove',installed)}>Remove</button>{/if}
        {/if}
      </div>
      {#if installed?.enabled&&!installed.error&&market?.session?.module===installed.id&&market.session.digest===installed.digest}<div class="market-actions">
        {#each market.session.view.actions as item}<button class="key" disabled={busy} onclick={()=>void attempt(()=>invoke(installed,item))}>{item.label}</button>{/each}
      </div>{/if}
    </article>
  {/each}
  {#if !rows.length}<p>No modules match this search.</p>{/if}
  {#if market}<p class="setting-note">Source: {market.repository}<br/>Catalog commit: {market.commit??'Not downloaded'}. Cached discovery works offline. Installation and updates require explicit enablement.</p>{/if}
</div>
<style>
  .market-toolbar,.market-actions{display:flex;gap:.6rem;flex-wrap:wrap;align-items:center}.market-toolbar input{flex:1;min-width:12rem}.market-module{padding:1rem 0;border-bottom:1px solid var(--line,#555)}h3{margin:.3rem 0}small{font-weight:normal;font-size:.8em}.market-actions{margin:.7rem 0}.market-error{color:#ff9c9c}code{overflow-wrap:anywhere}
</style>
