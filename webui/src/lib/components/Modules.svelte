<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { query, action } from '../api';
  import { ui, dispatch, attempt } from '../workspace.svelte';
  import Markdown from './Markdown.svelte';
  type Build={module_id:string;phase:string;detail:string;source_digest:string;identity?:{package_digest:string}};
  type Entry={id:string;name:string;version:string;description:string;digest?:string;source_digest?:string;source?:boolean;enabled?:boolean;bundled?:boolean;error?:string;tags?:string[];source_build?:Build};
  type MenuAction={label:string;arguments:string;submit:boolean};
  type PublishedArtifact={name:string;sha256:string;bytes:number;export_path:string};
  type ModuleView={actions:MenuAction[];summary?:string;summary_bytes?:number;summary_sha256?:string;summary_truncated?:boolean;progress?:{state?:string;summary?:string;role?:string;operation?:string;target?:unknown};artifact?:PublishedArtifact|null};
  type Market={session?:{module:string;digest:string;view:ModuleView};repository:string;commit:string|null;source_commit?:string|null;notice?:string;installed:Entry[];modules:Entry[];sources?:Entry[];source_builds?:Build[]};
  let market=$state<Market|null>(null),busy=$state(false),error=$state(''),filter=$state('');
  let loadedRun=$state<string|null>(null);
  const rows=$derived.by(()=>{
    const all=new Map<string,{entry:Entry;installed:Entry|undefined}>();
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
  onMount(()=>{void (async()=>{try{await load();if(!market?.commit)await change('refresh');}catch(e){error=String(e);}})();const timer=setInterval(()=>void load().catch(e=>error=String(e)),1200);return()=>clearInterval(timer);});
  async function use(entry:Entry){
    ui.overlay='';ui.activeFile='';ui.modes[ui.sessionId]='chat';
    ui.drafts[ui.sessionId]='/module '+entry.id+' ';
    await tick();document.getElementById('conversation-input')?.focus();
  }
  async function invoke(entry:Entry,item:MenuAction){if(item.submit){ui.overlay='';await dispatch('module',[entry.id,item.arguments]);}else{await use(entry);ui.drafts[ui.sessionId]='/module '+entry.id+' '+item.arguments;}}
  async function copyExport(artifact:PublishedArtifact){await navigator.clipboard.writeText(artifact.export_path);ui.notice=`Copied ${artifact.name} export path.`;}
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
        {#if entry.source}<button class="key" disabled={busy||!!entry.source_build&&!['installed','failed','cancelled','interrupted'].includes(entry.source_build.phase)} onclick={()=>void change('build-install',{...entry,digest:entry.source_digest??''})}>{installed?'Rebuild and install':'Build and install'}</button>
          {#if entry.source_build&&['queued','preparing_source','preparing_dependencies','compiling','packaging'].includes(entry.source_build.phase)}<button class="key" disabled={busy} onclick={()=>void change('cancel-build',entry)}>Cancel build</button>{/if}
        {:else if !installed||installed.digest!==entry.digest||installed.error}<button class="key" disabled={busy} onclick={()=>void change('install',entry)}>{installed?.error?'Repair install':installed?'Install update':'Install'} (disabled)</button>{/if}
        {#if installed}<button class="key" disabled={busy} onclick={()=>void change(installed.enabled?'disable':'enable',installed)}>{installed.enabled?'Disable':'Enable'}</button>
          {#if installed.enabled&&!installed.error}<button class="key primary" title={`Use installed version ${installed.version}`} onclick={()=>void use(installed)}>Use module</button>{/if}
          {#if !installed.bundled}<button class="key" disabled={busy} onclick={()=>void change('remove',installed)}>Remove</button>{/if}
        {/if}
      </div>
      {#if installed?.enabled&&!installed.error&&market?.session?.module===installed.id&&market.session.digest===installed.digest}
        <section class="module-run" aria-label={`${installed.name} current run`}>
          <h4>Current campaign</h4>
          <p class="module-progress" role="status"><strong>{market.session.view.progress?.state?.replaceAll('_',' ')??'waiting for first publication'}</strong>{#if market.session.view.progress?.summary}<br/>{market.session.view.progress.summary}{/if}</p>
          {#if market.session.view.summary}<details open><summary>Latest durable module summary{market.session.view.summary_truncated?' (bounded preview)':''}</summary><Markdown text={market.session.view.summary}/>{#if market.session.view.summary_sha256}<p class="setting-note">{market.session.view.summary_bytes?.toLocaleString()} bytes · SHA-256 {market.session.view.summary_sha256}</p>{/if}</details>{/if}
          {#if market.session.view.artifact}<div class="module-export"><strong>Latest export</strong><span>{market.session.view.artifact.name} · {market.session.view.artifact.bytes.toLocaleString()} bytes</span><code>{market.session.view.artifact.sha256}</code><code>{market.session.view.artifact.export_path}</code><button class="key" onclick={()=>void attempt(()=>copyExport(market!.session!.view.artifact!))}>Copy export path</button></div>{/if}
          <div class="market-actions" aria-label="Module actions">{#each market.session.view.actions as item}<button class="key" disabled={busy} onclick={()=>void attempt(()=>invoke(installed,item))}>{item.label}</button>{/each}</div>
        </section>
      {/if}
    </article>
  {/each}
  {#if !rows.length}<p>No modules match this search.</p>{/if}
  {#if market}<p class="setting-note">Source: {market.repository}<br/>Catalog commit: {market.commit??'Not downloaded'}{#if market.source_commit}<br/>Source catalog commit: {market.source_commit}{/if}. Cached discovery is inert and works offline. Build and install is explicit; every installation remains disabled until enabled.</p>{/if}
</div>
<style>
  .market-toolbar,.market-actions{display:flex;gap:.6rem;flex-wrap:wrap;align-items:center}.market-toolbar input{flex:1;min-width:12rem}.market-module{padding:1rem 0;border-bottom:1px solid var(--line,#555)}h3{margin:.3rem 0}h4{margin:.2rem 0 .7rem}small{font-weight:normal;font-size:.8em}.market-actions{margin:.7rem 0}.market-error{color:#ff9c9c}code{overflow-wrap:anywhere}.module-run{border:1px solid var(--line);background:var(--display);padding:1rem;margin:.9rem 0}.module-progress{border-left:3px solid var(--accent);padding:.35rem .7rem}.module-run details{margin:.8rem 0}.module-run details>summary{cursor:pointer;color:var(--accent)}.module-export{display:grid;gap:.35rem;border-top:1px solid var(--line);padding-top:.8rem}.module-export .key{justify-self:start}
</style>
