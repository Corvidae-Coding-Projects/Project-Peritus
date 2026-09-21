<script lang="ts">
  import { tick } from 'svelte';
  import { ui,project,session,attempt,openProject,editSession,selectSession,dispatch,notify,refresh,openRun } from '../workspace.svelte';
  import { action } from '../api';
  import { parseSlash } from '../commands/slash';
  import Icon from './Icon.svelte';
  import Settings from './Settings.svelte';
  import Models from './Models.svelte';
  import Console from './Console.svelte';
  let dialog:HTMLDialogElement,root=$state(''),sessionFilter=$state(''),cli=$state('status'),sessionTitle=$state(''),parent=$state('');
  const titles:Record<string,string>={open:'Open a project',settings:'Console configuration',model:'Models & reasoning',sessions:'Session library',report:'Inspection',runs:'Run history',console:'Harness console',cli:'Scriptable CLI',discard:'Discard candidate changes',session:'Organize session'};
  let title=$derived(ui.overlay==='report'?ui.reportTitle:titles[ui.overlay]??'Control');
  $effect(()=>{if(ui.overlay){dialog.showModal();if(ui.overlay==='session'){sessionTitle=session()?.title??'';parent=session()?.parent??'';}void tick().then(()=>dialog.querySelector<HTMLInputElement>('input:not([type=checkbox])')?.focus());}else dialog?.close();});
  async function runCli(){const parsed=parseSlash('/cli '+cli);if(parsed.kind!=='command')throw new Error('Check your command’s quoted arguments.');await dispatch('cli',[...parsed.args]);}
  const cliPresets=[['Status','status'],['Get artifact','artifact get --artifact ID --output PATH'],['Upload artifact','artifact put --artifact ID --input PATH --media-type text/plain'],['Cancel artifact transfer','artifact cancel --transfer ID --artifact ID'],['Watch events','events watch --topic TOPIC --snapshot-acceptable'],['Answer prompt','prompt answer --binding PATH --text "Your answer"'],['Cancel prompt','prompt cancel --binding PATH'],['Attach terminal','terminal attach --process ID'],['Terminal input','terminal input --attachment ID --process ID --originating-request ID --input PATH'],['Resize terminal','terminal resize --attachment ID --process ID --originating-request ID --columns 100 --rows 30'],['Detach terminal','terminal detach --attachment ID --process ID --originating-request ID'],['Cancel terminal','terminal cancel --attachment ID --process ID --originating-request ID'],['Submit command','command submit --actor ID --envelope PATH --payload PATH --idempotency-key KEY'],['Shutdown daemon','shutdown --wait'],['Shell completions','completions bash'],['Update checks','update --enable-checks']];
</script>
<dialog bind:this={dialog} class="control-dialog" class:wide={['settings','console','report'].includes(ui.overlay)} aria-label={title} onclose={()=>ui.overlay=''} onclick={(event)=>{if(event.target===dialog)ui.overlay='';}}>
  <div class="dialog-heading"><h2>{title}</h2><button class="key icon-button" aria-label="Close panel" onclick={()=>ui.overlay=''}><Icon name="close"/></button></div>
  <div class="dialog-body">
    {#if ui.overlay==='open'}
      <form class="open-project-form" onsubmit={(event)=>{event.preventDefault();void attempt(()=>openProject(root));}}><p>Keep multiple projects running in one browser tab. Each project retains its own sessions, files, and repository settings.</p><label for="project-root">Project root directory</label><input id="project-root" bind:value={root} placeholder="/home/you/projects/my-project" required/><p class="setting-note">Use a directory on the machine running Peritus. Nested sessions must share this exact canonical root.</p><div class="dialog-actions"><button class="key primary">Open project<Icon name="arrow"/></button></div></form>
    {:else if ui.overlay==='settings'}<Settings/>
    {:else if ui.overlay==='model'}{#key ui.sessionId}<Models/>{/key}
    {:else if ui.overlay==='report'}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to scroll long inspection output.) -->
      <pre class="report-text" role="region" aria-label="Inspection output" tabindex="0">{ui.reportText||'No detail is available yet.'}</pre>
    {:else if ui.overlay==='session'}
      <form class="session-form" onsubmit={(event)=>{event.preventDefault();void attempt(async()=>{await editSession(ui.sessionId,{title:sessionTitle,parent:parent||null});ui.overlay='';notify('Session organization saved.');});}}><label>Session title<input bind:value={sessionTitle} required maxlength="256"/></label><label>Nest beneath<select bind:value={parent}><option value="">Top-level session</option>{#each ui.workspace.sessions.filter(s=>s.project===ui.projectId&&s.id!==ui.sessionId&&!s.closed) as item}<option value={item.id}>{item.title}</option>{/each}</select></label><p class="setting-note">Only this project’s sessions are listed. Cycles and cross-project nesting are rejected by the server.</p><div class="dialog-actions"><button class="key primary">Save session</button></div></form>
    {:else if ui.overlay==='sessions'}
      <p class="dialog-description">Closing a tab keeps its conversation and running work. Reopen a session here.</p><input class="library-filter" aria-label="Find a session" bind:value={sessionFilter} placeholder="Find a session…"/>
      <div class="session-library">{#each ui.workspace.sessions.filter(s=>s.title.toLowerCase().includes(sessionFilter.toLowerCase())) as item}<div class="library-row"><Icon name={item.parent?'branch':'chat'}/><span><strong>{item.title}</strong><small>{ui.workspace.projects.find(p=>p.id===item.project)?.name} · {item.closed?'Tab closed':'Open'}{item.parent?' · Nested':''}</small></span><button class="key small" onclick={()=>void attempt(async()=>{await editSession(item.id,{closed:false});selectSession(item.id);ui.overlay='';})}>Open</button></div>{/each}</div>
    {:else if ui.overlay==='runs'}
      <p class="dialog-description">Actual observations from the daemon, including runs started by other clients.</p>
      {#each ui.runs as run}<div class="library-row"><Icon name="layers"/><span><strong>{run.task}</strong><small>{run.phase} · {run.id.slice(0,8)}</small></span><button class="key small" onclick={()=>void attempt(()=>openRun(run.id))}>Open conversation</button><button class="key small" onclick={()=>{ui.reportTitle=run.task;ui.reportText=[run.status,run.summary,run.gates,run.review].filter(Boolean).join('\n\n');ui.overlay='report';}}>Inspect</button></div>{/each}
      {#if !ui.runs.length}<div class="small-empty">No daemon-owned runs have been observed yet. Start a conversation to begin.</div>{/if}
    {:else if ui.overlay==='console'}
      <div class="console-tabs">{#each ui.consoles.filter(c=>c.project===ui.projectId) as console}<button class:active={console.id===ui.consoleId} onclick={()=>ui.consoleId=console.id}>{console.title}{console.ended?' · exited':''}</button>{/each}</div>
      {#each ui.consoles.filter(c=>c.id===ui.consoleId) as console(console.id)}<Console id={console.id} suggestion={console.suggestion}/>{/each}
      <div class="console-footer-actions"><button class="flat" onclick={()=>void attempt(()=>dispatch('terminal'))}>New session console</button><button class="flat" onclick={()=>void attempt(()=>dispatch('cli'))}>Scriptable command…</button><button class="flat" onclick={()=>void attempt(async()=>{await refresh();notify('Provider and workspace configuration reloaded.');})}>Reload configuration after setup</button></div>
    {:else if ui.overlay==='cli'}
      <p class="dialog-description">Run any normal Peritus CLI command in a retained console. Arguments are passed directly, without shell expansion. The selected project is the working directory.</p>
      <form class="cli-form" onsubmit={(event)=>{event.preventDefault();void attempt(runCli);}}><label>Command template<select onchange={(event)=>cli=event.currentTarget.value}><option value="status">Choose a command…</option>{#each cliPresets as preset}<option value={preset[1]}>{preset[0]}</option>{/each}</select></label><label>Arguments after <code>peritus</code><textarea rows="4" bind:value={cli} spellcheck="false"></textarea></label><p class="setting-note">Replace ID, PATH, and TOPIC placeholders with exact values. Daemon commands receive the configured endpoint automatically.</p><div class="dialog-actions"><button class="key primary">Run in console<Icon name="terminal"/></button></div></form>
    {:else if ui.overlay==='discard'}
      <p>This removes the current run’s candidate changes. Export a patch first if you need to keep them.</p><code>{ui.sessionId}</code><div class="dialog-actions"><button class="key" onclick={()=>ui.overlay=''}>Keep changes</button><button class="key danger" onclick={()=>void attempt(async()=>{await action('control',{session:ui.sessionId,action:'discard'});ui.overlay='';notify('Candidate discard requested.');})}>Discard candidate</button></div>
    {/if}
  </div>
</dialog>
