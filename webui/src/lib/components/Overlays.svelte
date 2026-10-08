<script lang="ts">
  import { tick } from 'svelte';
  import { ui,project,session,attempt,openProject,editSession,selectSession,dispatch,notify,refresh,openRun,runInspection,loadMoreRuns,loadGitOutput,clearGitOutput } from '../workspace.svelte';
  import { action } from '../api';
  import { parseSlash } from '../commands/slash';
  import Icon from './Icon.svelte';
  import Settings from './Settings.svelte';
  import Models from './Models.svelte';
  import Improvements from './Improvements.svelte';
  import Console from './Console.svelte';
  import TextBody from './TextBody.svelte';
  let dialog:HTMLDialogElement,root=$state(''),sessionFilter=$state(''),cli=$state('status'),sessionTitle=$state(''),parent=$state('');
  const COLLECTION_RENDER_BATCH=40;
  let sessionStart=$state(0),runStart=$state(0),sessionAnchor=$state(''),runAnchor=$state('');
  let parentFilter=$state(''),parentStart=$state(0);
  let consoleStart=$state(0),consoleFocus='';
  const titles:Record<string,string>={improvements:'Harness improvement inbox',open:'Open a project',settings:'Console configuration',model:'Models & reasoning',sessions:'Session library',report:'Inspection',runs:'Run history',console:'Harness console',cli:'Scriptable CLI',discard:'Discard candidate changes',session:'Organize session'};
  let title=$derived(ui.overlay==='report'?ui.reportTitle:titles[ui.overlay]??'Control');
  let sessionNeedle=$derived(sessionFilter.toLowerCase()),parentNeedle=$derived(parentFilter.toLowerCase());
  let matchingSessions=$derived(ui.workspace.sessions.filter(s=>s.title.toLowerCase().includes(sessionNeedle)));
  let visibleSessions=$derived(matchingSessions.slice(sessionStart,sessionStart+COLLECTION_RENDER_BATCH)),visibleRuns=$derived(ui.runs.slice(runStart,runStart+COLLECTION_RENDER_BATCH));
  let matchingParents=$derived(ui.workspace.sessions.filter(s=>s.project===ui.projectId&&s.id!==ui.sessionId&&!s.closed&&s.title.toLowerCase().includes(parentNeedle)));
  let visibleParents=$derived(matchingParents.slice(parentStart,parentStart+COLLECTION_RENDER_BATCH));
  let retainedParent=$derived(parent&&!visibleParents.some(item=>item.id===parent)?ui.workspace.sessions.find(item=>item.id===parent):undefined);
  let projectConsoles=$derived(ui.consoles.filter(item=>item.project===ui.projectId));
  let visibleConsoles=$derived(projectConsoles.slice(consoleStart,consoleStart+COLLECTION_RENDER_BATCH));
  $effect(()=>{const overlay=ui.overlay;if(overlay==='sessions'){sessionStart=0;sessionAnchor='';}if(overlay==='runs'){runStart=0;runAnchor='';}if(overlay==='session'){parentStart=0;parentFilter='';}});
  $effect(()=>{const items=matchingSessions,anchor=sessionAnchor;if(anchor){const at=items.findIndex(item=>item.id===anchor);if(at>=0)sessionStart=at;}if(sessionStart>=items.length)sessionStart=Math.max(0,items.length-COLLECTION_RENDER_BATCH);});
  $effect(()=>{const items=ui.runs,anchor=runAnchor;if(anchor){const at=items.findIndex(item=>item.id===anchor);if(at>=0)runStart=at;}if(runStart>=items.length)runStart=Math.max(0,items.length-COLLECTION_RENDER_BATCH);});
  $effect(()=>{if(parentStart>=matchingParents.length)parentStart=Math.max(0,matchingParents.length-COLLECTION_RENDER_BATCH);});
  $effect(()=>{
    const key=JSON.stringify([ui.projectId,ui.consoleId]),items=projectConsoles;
    if(key!==consoleFocus){consoleFocus=key;const at=items.findIndex(item=>item.id===ui.consoleId);consoleStart=Math.max(0,Math.floor(at/COLLECTION_RENDER_BATCH)*COLLECTION_RENDER_BATCH);}
    if(consoleStart>=items.length)consoleStart=Math.max(0,items.length-COLLECTION_RENDER_BATCH);
  });
  $effect(()=>{
    const overlay=ui.overlay;
    if(overlay){dialog.showModal();if(overlay==='session'){sessionTitle=session()?.title??'';parent=session()?.parent??'';}void tick().then(()=>dialog.querySelector<HTMLInputElement>('input:not([type=checkbox])')?.focus());}else dialog?.close();
  });
  async function runCli(){const parsed=parseSlash('/cli '+cli);if(parsed.kind!=='command')throw new Error('Check your command’s quoted arguments.');await dispatch('cli',[...parsed.args]);}
  function sessionPage(direction:number){sessionStart=Math.max(0,Math.min(Math.max(0,matchingSessions.length-1),sessionStart+direction*COLLECTION_RENDER_BATCH));sessionAnchor=matchingSessions[sessionStart]?.id??'';}
  function runPage(direction:number){runStart=Math.max(0,Math.min(Math.max(0,ui.runs.length-1),runStart+direction*COLLECTION_RENDER_BATCH));runAnchor=ui.runs[runStart]?.id??'';}
  async function moreRuns(){const start=ui.runs.length;await loadMoreRuns();if(ui.runs.length>start){runStart=start;runAnchor=ui.runs[start]?.id??'';}}
  const decimal=(value:string)=>BigInt(value).toLocaleString();
  const outputEnd=()=>ui.reportOutput?(BigInt(ui.reportOutput.offset)+BigInt(ui.reportOutput.pageBytes)).toLocaleString():'0';
  const cliPresets=[['Status','status'],['Get artifact','artifact get --artifact ID --output PATH'],['Upload artifact','artifact put --artifact ID --input PATH --media-type text/plain'],['Cancel artifact transfer','artifact cancel --transfer ID --artifact ID'],['Watch events','events watch --topic TOPIC --snapshot-acceptable'],['Answer prompt','prompt answer --binding PATH --text "Your answer"'],['Cancel prompt','prompt cancel --binding PATH'],['Attach terminal','terminal attach --process ID'],['Terminal input','terminal input --attachment ID --process ID --originating-request ID --input PATH'],['Resize terminal','terminal resize --attachment ID --process ID --originating-request ID --columns 100 --rows 30'],['Detach terminal','terminal detach --attachment ID --process ID --originating-request ID'],['Cancel terminal','terminal cancel --attachment ID --process ID --originating-request ID'],['Submit command','command submit --actor ID --envelope PATH --payload PATH --idempotency-key KEY'],['Shutdown daemon','shutdown --wait'],['Shell completions','completions bash'],['Update checks','update --enable-checks']];
</script>
<dialog bind:this={dialog} class="control-dialog" class:wide={['settings','console','report','improvements'].includes(ui.overlay)} aria-label={title} onclose={()=>ui.overlay=''} onclick={(event)=>{if(event.target===dialog)ui.overlay='';}}>
  <div class="dialog-heading"><h2>{title}</h2><button class="key icon-button" aria-label="Close panel" onclick={()=>ui.overlay=''}><Icon name="close"/></button></div>
  <div class="dialog-body">
    {#if ui.overlay==='open'}
      <form class="open-project-form" onsubmit={(event)=>{event.preventDefault();void attempt(()=>openProject(root));}}><p>Keep multiple projects running in one browser tab. Each project retains its own sessions, files, and repository settings.</p><label for="project-root">Project root directory</label><input id="project-root" bind:value={root} placeholder="/home/you/projects/my-project" required/><p class="setting-note">Use a directory on the machine running Peritus. Nested sessions must share this exact canonical root.</p><div class="dialog-actions"><button class="key primary">Open project<Icon name="arrow"/></button></div></form>
    {:else if ui.overlay==='settings'}<Settings/>
    {:else if ui.overlay==='model'}{#key ui.sessionId}<Models/>{/key}
    {:else if ui.overlay==='report'}
      {#if ui.reportOutput}<div class="dialog-actions"><button class="key small" disabled={ui.reportOutput.previous===null} onclick={()=>void attempt(()=>loadGitOutput(ui.reportOutput!.previous!))}>Previous page</button><button class="key small" disabled={ui.reportOutput.next===null} onclick={()=>void attempt(()=>loadGitOutput(ui.reportOutput!.next!))}>Next page</button><button class="flat" onclick={()=>void attempt(()=>loadGitOutput('0',ui.reportOutput!.stream==='stdout'?'stderr':'stdout'))}>{ui.reportOutput.stream==='stdout'?'View stderr':'View stdout'}</button><small>{ui.reportOutput.stream} bytes {decimal(ui.reportOutput.offset)}–{outputEnd()} of {decimal(ui.reportOutput.bytes)} · sha256 {ui.reportOutput.digest}</small></div>{/if}
      <TextBody text={ui.reportText||'No detail is available yet.'} className="report-text" label="Inspection output"/>
    {:else if ui.overlay==='session'}
      <form class="session-form" onsubmit={(event)=>{event.preventDefault();void attempt(async()=>{await editSession(ui.sessionId,{title:sessionTitle,parent:parent||null});ui.overlay='';notify('Session organization saved.');});}}><label>Session title<input bind:value={sessionTitle} required/></label><label>Find a parent session<input value={parentFilter} oninput={event=>{parentFilter=event.currentTarget.value;parentStart=0;}}/></label><label>Nest beneath<select bind:value={parent}><option value="">Top-level session</option>{#if retainedParent}<option value={retainedParent.id}>{retainedParent.title} · selected</option>{/if}{#each visibleParents as item}<option value={item.id}>{item.title}</option>{/each}</select></label><div class="collection-more"><button type="button" class="key small" disabled={!parentStart} onclick={()=>parentStart=Math.max(0,parentStart-COLLECTION_RENDER_BATCH)}>Previous parents</button><span>{matchingParents.length?parentStart+1:0}–{Math.min(matchingParents.length,parentStart+COLLECTION_RENDER_BATCH)} of {matchingParents.length.toLocaleString()}</span><button type="button" class="key small" disabled={parentStart+COLLECTION_RENDER_BATCH>=matchingParents.length} onclick={()=>parentStart+=COLLECTION_RENDER_BATCH}>Next parents</button></div><p class="setting-note">Only this project’s sessions are listed. Cycles and cross-project nesting are rejected by the server.</p><div class="dialog-actions"><button class="key primary">Save session</button></div></form>
    {:else if ui.overlay==='sessions'}
      <p class="dialog-description">Closing a tab keeps its conversation and running work. Reopen a session here.</p><input class="library-filter" aria-label="Find a session" value={sessionFilter} oninput={event=>{sessionFilter=event.currentTarget.value;sessionStart=0;sessionAnchor='';}} placeholder="Find a session…"/>
      <div class="session-library">{#each visibleSessions as item}<div class="library-row"><Icon name={item.parent?'branch':'chat'}/><span><strong>{item.title}</strong><small>{ui.workspace.projects.find(p=>p.id===item.project)?.name} · {item.closed?'Tab closed':'Open'}{item.parent?' · Nested':''}</small></span><button class="key small" onclick={()=>void attempt(async()=>{await editSession(item.id,{closed:false});selectSession(item.id);ui.overlay='';})}>Open</button></div>{/each}</div>
      {#if matchingSessions.length>COLLECTION_RENDER_BATCH}<div class="collection-more"><button class="key small" disabled={!sessionStart} onclick={()=>sessionPage(-1)}>Previous sessions</button><span>{sessionStart+1}–{sessionStart+visibleSessions.length} of {matchingSessions.length.toLocaleString()}</span><button class="key small" disabled={sessionStart+visibleSessions.length>=matchingSessions.length} onclick={()=>sessionPage(1)}>Next sessions</button></div>{/if}
    {:else if ui.overlay==='improvements'}{#key ui.projectId}<Improvements/>{/key}
    {:else if ui.overlay==='runs'}
      <p class="dialog-description">Actual observations from the daemon, including runs started by other clients.</p>
      {#each visibleRuns as run}<div class="library-row"><Icon name="layers"/><span><strong>{run.task}</strong><small>{run.operation.state} · {run.id.slice(0,8)}</small></span><button class="key small" onclick={()=>void attempt(()=>openRun(run.id))}>Open conversation</button><button class="key small" onclick={()=>{clearGitOutput();ui.reportTitle=run.task;ui.reportText=runInspection(run);ui.overlay='report';}}>Inspect</button></div>{/each}
      {#if ui.runs.length>COLLECTION_RENDER_BATCH||ui.runCursor}<div class="collection-more"><button class="key small" disabled={!runStart} onclick={()=>runPage(-1)}>Previous runs</button><span>{ui.runs.length?runStart+1:0}–{runStart+visibleRuns.length} of {ui.runs.length.toLocaleString()} loaded</span>{#if runStart+visibleRuns.length<ui.runs.length}<button class="key small" onclick={()=>runPage(1)}>Next runs</button>{:else if ui.runCursor}<button class="key small" disabled={ui.runLoading} onclick={()=>void attempt(moreRuns)}>{ui.runLoading?'Loading runs…':'Load next runs'}</button>{/if}</div>{/if}
      {#if ui.runLoading&&!ui.runs.length}<p class="setting-note">Loading run history…</p>{:else if !ui.runs.length}<div class="small-empty">No daemon-owned runs have been observed yet. Start a conversation to begin.</div>{/if}
    {:else if ui.overlay==='console'}
      <div class="console-tabs">{#each visibleConsoles as console}<button class:active={console.id===ui.consoleId} onclick={()=>ui.consoleId=console.id}>{console.title}{console.ended?' · exited':''}</button>{/each}</div>
      {#if projectConsoles.length>COLLECTION_RENDER_BATCH}<div class="collection-more"><button class="key small" disabled={!consoleStart} onclick={()=>consoleStart=Math.max(0,consoleStart-COLLECTION_RENDER_BATCH)}>Previous consoles</button><span>{consoleStart+1}–{consoleStart+visibleConsoles.length} of {projectConsoles.length.toLocaleString()}</span><button class="key small" disabled={consoleStart+visibleConsoles.length>=projectConsoles.length} onclick={()=>consoleStart+=COLLECTION_RENDER_BATCH}>Next consoles</button></div>{/if}
      {#each ui.consoles.filter(c=>c.id===ui.consoleId) as console(`${console.workspace}:${console.id}`)}<Console id={console.id} workspace={console.workspace} suggestion={console.suggestion}/>{/each}
      <div class="console-footer-actions"><button class="flat" onclick={()=>void attempt(()=>dispatch('terminal'))}>New session console</button><button class="flat" onclick={()=>void attempt(()=>dispatch('cli'))}>Scriptable command…</button><button class="flat" onclick={()=>void attempt(async()=>{await refresh();notify('Provider and workspace configuration reloaded.');})}>Reload configuration after setup</button></div>
    {:else if ui.overlay==='cli'}
      <p class="dialog-description">Run any normal Peritus CLI command in a retained console. Arguments are passed directly, without shell expansion. The selected project is the working directory.</p>
      <form class="cli-form" onsubmit={(event)=>{event.preventDefault();void attempt(runCli);}}><label>Command template<select onchange={(event)=>cli=event.currentTarget.value}><option value="status">Choose a command…</option>{#each cliPresets as preset}<option value={preset[1]}>{preset[0]}</option>{/each}</select></label><label>Arguments after <code>peritus</code><textarea rows="4" bind:value={cli} spellcheck="false"></textarea></label><p class="setting-note">Replace ID, PATH, and TOPIC placeholders with exact values. Daemon commands receive the configured endpoint automatically.</p><div class="dialog-actions"><button class="key primary">Run in console<Icon name="terminal"/></button></div></form>
    {:else if ui.overlay==='discard'}
      <p>This removes the current run’s candidate changes. Export a patch first if you need to keep them.</p><code>{session()?.run}</code><div class="dialog-actions"><button class="key" onclick={()=>ui.overlay=''}>Keep changes</button><button class="key danger" onclick={()=>void attempt(async()=>{await action('control',{session:ui.sessionId,action:'discard'});ui.overlay='';notify('Candidate discard requested.');})}>Discard candidate</button></div>
    {/if}
  </div>
</dialog>
