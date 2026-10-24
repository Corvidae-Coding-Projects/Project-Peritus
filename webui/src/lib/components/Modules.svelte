<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { query, action } from '../api';
  import { ui, dispatch, attempt } from '../workspace.svelte';
  type Build={module_id:string;phase:string;detail:string;source_digest:string;identity?:{package_digest:string}};
  type Entry={id:string;name:string;version:string;description:string;digest?:string;source_digest?:string;source?:boolean;enabled?:boolean;bundled?:boolean;error?:string;tags?:string[];source_build?:Build};
  type MenuAction={label:string;arguments:string;submit:boolean};
  type Market={session?:{module:string;digest:string;view:{actions:MenuAction[]}};repository:string;commit:string|null;source_commit?:string|null;notice?:string;installed:Entry[];modules:Entry[];sources?:Entry[];source_builds?:Build[]};
  let market=$state<Market|null>(null),busy=$state(false),error=$state(''),filter=$state('');
  let loadedRun=$state<string|null>(null);
  const rows=$derived.by(()=>{
    const all=new Map<string,{entry:Entry;installed?:Entry}>();
    for(const entry of market?.installed??[])all.set(entry.id,{entry,installed:entry});
    for(const entry of market?.modules??[])all.set(entry.id,{entry,installed:all.get(entry.id)?.installed});
    for(const source of market?.sources??[]){const prior=all.get(source.id);all.set(source.id,{entry:{...(prior?.entry??source),...source,source:true},installed:prior?.installed});}
    for(const build of market?.source_builds??[]){const prior=all.get(build.module_id);if(prior)prior.entry.source_build=build;}
    const q=filter.toLowerCase();return [...all.values()].filter(({entry})=>[entry.id,entry.name,entry.description,...entry.tags??[]].join(' ').toLowerCase().includes(q));
  });
  async function load(){
    const run=ui.conversations[ui.sessionId]?.run?.id??'';
    loadedRun=run;
    const result=await query<Market>('modules',{run});
    if((ui.conversations[ui.sessionId]?.run?.id??'')===run)market=result;
  }
  $effect(()=>{
    const run=ui.conversations[ui.sessionId]?.run?.id??'';
    if(loadedRun!==null&&loadedRun!==run)void load().catch(e=>error=String(e));
  });
  async function change(operation:string,entry?:Entry){
    if(busy)return;busy=true;error='';
    try{await action('modules',{action:operation,module:entry?.id,digest:entry?.digest});await load();}
    catch(e){error=e instanceof Error?e.message:String(e);}finally{busy=false;}
  }
  onMount(()=>{void (async()=>{try{await load();if(!market?.commit)await change('refresh');}catch(e){error=String(e);}})();const timer=setInterval(()=>{if(market?.source_builds?.some(build=>['queued','preparing_source','preparing_dependencies','compiling','packaging'].includes(build.phase)))void load().catch(e=>error=String(e));},1000);return()=>clearInterval(timer);});
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
      <code>{entry.id}</code><p class="setting-note">{installed?.bundled?'Bundled':installed?'Installed':entry.source?'Source available':'Available'} · {installed?.enabled?'Enabled':'Disabled'}{installed&&entry.digest&&installed.digest!==entry.digest?' · Update available':''}</p>
      <p class="setting-note">{#if installed}Installed: {installed.version} · {installed.digest}<br/>{/if}{#if entry.source}Source: {entry.version} · {entry.source_digest}{:else if !installed?.bundled}Available: {entry.version} · {entry.digest}{/if}</p>
      {#if entry.source_build}<p role="status"><strong>Source build: {entry.source_build.phase.replaceAll('_',' ')}</strong><br/>{entry.source_build.detail}</p>{/if}
      {#if installed?.error}<p role="alert">{installed.error}</p>{/if}
      <div class="market-actions">
        {#if entry.source}<button class="key" disabled={busy||!!entry.source_build&&!['installed','failed','cancelled','interrupted'].includes(entry.source_build.phase)} onclick={()=>void change('build-install',{...entry,digest:entry.source_digest})}>{installed?'Rebuild and install':'Build and install'} (disabled)</button>
          {#if entry.source_build&&['queued','preparing_source','preparing_dependencies','compiling','packaging'].includes(entry.source_build.phase)}<button class="key" disabled={busy} onclick={()=>void change('cancel-build',entry)}>Cancel build</button>{/if}
        {:else if !installed||installed.digest!==entry.digest||installed.error}<button class="key" disabled={busy} onclick={()=>void change('install',entry)}>{installed?.error?'Repair install':installed?'Install update':'Install'} (disabled)</button>{/if}
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
  {#if market}<p class="setting-note">Source: {market.repository}<br/>Catalog commit: {market.commit??'Not downloaded'}{#if market.source_commit}<br/>Source catalog commit: {market.source_commit}{/if}. Cached discovery is inert and works offline. Build and install is explicit; every installation remains disabled until enabled.</p>{/if}
</div>
<style>
  .market-toolbar,.market-actions{display:flex;gap:.6rem;flex-wrap:wrap;align-items:center}.market-toolbar input{flex:1;min-width:12rem}.market-module{padding:1rem 0;border-bottom:1px solid var(--line,#555)}h3{margin:.3rem 0}small{font-weight:normal;font-size:.8em}.market-actions{margin:.7rem 0}.market-error{color:#ff9c9c}code{overflow-wrap:anywhere}
</style>
