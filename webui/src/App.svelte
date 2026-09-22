<script lang="ts">
  import { onMount,tick } from 'svelte';
  import { ui,start,persist,project,session,attempt,dispatch,selectProject,selectSession,newSession,editSession,openFile,closeFile,send,poll,notify,openProjects,closeProject,attachFile } from './lib/workspace.svelte';
  import { commands,matchesShortcut,formatShortcut } from './lib/commands/catalog';
  import type { Session,Mode } from './lib/types';
  import Icon from './lib/components/Icon.svelte';
  import Nixie from './lib/components/Nixie.svelte';
  import Explorer from './lib/components/Explorer.svelte';
  import GitPanel from './lib/components/GitPanel.svelte';
  import FileViewer from './lib/components/FileViewer.svelte';
  import Markdown from './lib/components/Markdown.svelte';
  import Palette from './lib/components/Palette.svelte';
  import Overlays from './lib/components/Overlays.svelte';
  import {protectFileDrafts} from './lib/files/drafts.svelte';
  import {recovery} from './lib/operations.svelte';
  import Attachments from './lib/components/Attachments.svelte';
  import Recovery from './lib/components/Recovery.svelte';
  import './lib/components/workflow.css';
  let fileDrag=$state(false);
  let admissionReady=$derived(ui.ready&&!!ui.facts?.ready);
  let recoveryHold=$derived(recovery.pending.some(item=>item.session===ui.sessionId));
  let messageReady=$derived(admissionReady&&!recoveryHold);
  function fileOver(event:DragEvent){if(event.dataTransfer?.types.some(type=>type==='application/x-peritus-file'||type==='Files')){event.preventDefault();fileDrag=true;if(event.dataTransfer)event.dataTransfer.dropEffect='copy';}}
  function fileDrop(event:DragEvent){
    fileDrag=false;const data=event.dataTransfer?.getData('application/x-peritus-file');
    if(data){event.preventDefault();void attempt(async()=>{const item=JSON.parse(data);if(typeof item.project!=='string'||typeof item.path!=='string')throw new Error('Invalid file selection');await attachFile(item.path,item.project);});}
    else if(event.dataTransfer?.files.length){event.preventDefault();notify('Choose a project file in the explorer to attach it. External file uploads are not enabled.',true);}
  }
  let composer=$state<HTMLTextAreaElement>(null!),transcript=$state<HTMLDivElement>(null!),following=$state(true),dragged=$state('');
  let activeProject=$derived(project()),activeSession=$derived(session());
  let apple=$state(false);
  let commandShortcut=$derived(formatShortcut(ui.preferences.shortcuts.commands,apple));
  let current=$derived(ui.conversations[ui.sessionId]);
  let mode=$derived(ui.modes[ui.sessionId]??'chat');
  let draft=$derived(ui.drafts[ui.sessionId]??'');
  let projects=$derived(openProjects());
  let opened=$derived(ui.workspace.sessions.filter(s=>!s.closed&&projects.some(p=>p.id===s.project)));
  let running=$derived(Object.values(ui.conversations).filter(c=>c.run?.busy).length);
  let sessionFiles=$derived(ui.files.filter(f=>f.session===ui.sessionId));
  let activities=$derived((current?.activities??[]).filter(a=>ui.details||['user','assistant','error'].includes(a.kind)));
  let suggestions=$derived(draft.startsWith('/')&&!draft.includes(' ')?commands.filter(c=>c.slash.startsWith(draft)).slice(0,7):[]);
  let lineage=$derived.by(()=>{
    const result:Session[]=[];let cursor=activeSession;const seen=new Set<string>();
    while(cursor&&!seen.has(cursor.id)){seen.add(cursor.id);result.unshift(cursor);cursor=ui.workspace.sessions.find(s=>s.id===cursor?.parent);}return result;
  });
  let rows=$derived.by(()=>{
    const group=(parent:string|null)=>opened.filter(s=>s.project===ui.projectId&&s.parent===parent);
    const groups=[group(null)];for(const ancestor of lineage){const children=group(ancestor.id);if(children.length)groups.push(children);}return groups;
  });
  onMount(()=>{
    apple=/Mac|iPhone|iPad|iPod/.test(navigator.platform);
    void start();let disposed=false,timer:ReturnType<typeof setTimeout>;
    async function update(){if(!document.hidden&&!ui.loading)await poll();if(!disposed)timer=setTimeout(()=>void update(),3500);}
    timer=setTimeout(()=>void update(),3500);return()=>{disposed=true;clearTimeout(timer);};
  });
  $effect(()=>{if(!ui.loading)persist();});
  $effect(()=>{
    const p=ui.preferences;const root=document.documentElement;root.dataset.theme=p.theme;root.dataset.density=p.density;root.dataset.motion=p.motion?'full':'reduced';
    root.style.setProperty('--base-font',`${p.font_size}px`);root.style.setProperty('--font-body',p.font_family);root.style.setProperty('--font-mono',p.mono_family);root.style.setProperty('--explorer-width',`${p.explorer_width}px`);
    for(const role of ['background','panel','display','text','muted','accent','line']){if(p.tokens[role])root.style.setProperty(`--${role}`,p.tokens[role]!);else root.style.removeProperty(`--${role}`);}
  });
  $effect(()=>{const count=activities.length;if(following&&count)void tick().then(()=>transcript?.scrollTo({top:transcript.scrollHeight}));});
  function keyboard(event:KeyboardEvent){
    if(event.defaultPrevented)return;
    for(const [command,binding]of Object.entries(ui.preferences.shortcuts)){if(matchesShortcut(event,binding)){event.preventDefault();if(command==='commands')ui.palette=' ';else void attempt(()=>dispatch(command));return;}}
    if(event.key==='Escape'&&!ui.overlay&&!ui.palette){ui.notice='';return;}
    if(event.altKey&&/^[1-9]$/.test(event.key)){event.preventDefault();const target=projects[Number(event.key)-1];if(target)void selectProject(target.id);}
  }
  function tabKeys(event:KeyboardEvent,items:Session[],index:number){
    let next=index;if(event.key==='ArrowRight')next=(index+1)%items.length;else if(event.key==='ArrowLeft')next=(index+items.length-1)%items.length;else if(event.key==='Home')next=0;else if(event.key==='End')next=items.length-1;else return;
    event.preventDefault();selectSession(items[next]!.id);void tick().then(()=>document.getElementById(`session-${items[next]!.id}`)?.focus());
  }
  async function closeTab(id:string){
    await editSession(id,{closed:true});await tick();
    const next=document.getElementById(`session-${ui.sessionId}`)??document.querySelector<HTMLButtonElement>('.session-rails button[aria-label="Add session"]');
    next?.focus();
  }
  async function closeProjectTab(id:string){
    await closeProject(id);await tick();
    const next=document.getElementById(`project-${ui.projectId}`)??document.querySelector<HTMLButtonElement>('.add-project');
    next?.focus();
  }
  let audioContext:AudioContext|undefined;
  function physical(event:MouseEvent){
    if(!ui.preferences.sound||!(event.target instanceof Element)||!event.target.closest('button'))return;
    audioContext??=new AudioContext();const oscillator=audioContext.createOscillator(),gain=audioContext.createGain();oscillator.type='triangle';oscillator.frequency.setValueAtTime(220,audioContext.currentTime);oscillator.frequency.exponentialRampToValueAtTime(60,audioContext.currentTime+0.045);gain.gain.setValueAtTime(.035,audioContext.currentTime);gain.gain.exponentialRampToValueAtTime(.001,audioContext.currentTime+.05);oscillator.connect(gain);gain.connect(audioContext.destination);oscillator.start();oscillator.stop(audioContext.currentTime+.05);
  }
</script>

<svelte:window onkeydown={keyboard} onclick={physical} onbeforeunload={protectFileDrafts}/>
<a class="skip-link" href="#conversation-input">Skip to message composer</a>
<div class="console-shell">
  <header class="top-plate">
    <a class="brand" href="/" aria-label="Peritus control console"><span class="brand-mark"><svg viewBox="0 0 36 40" aria-hidden="true"><path d="M7 34V6h15a9 9 0 0 1 0 18H7m10-18v28"/></svg></span><span><strong>PERITUS</strong><small>HARNESS CONTROL CONSOLE</small></span></a>
    <div class="header-instruments"><Nixie value={projects.length} label="Projects"/><Nixie value={opened.length} label="Sessions"/><Nixie value={running} label="Running"/></div>
    <div class="header-actions"><button class="command-key key" aria-label="Command directory" onclick={()=>ui.palette=' '}><Icon name="terminal"/><span>Command</span>{#if commandShortcut}<kbd>{commandShortcut}</kbd>{/if}</button><button class="key icon-button" aria-label="Console settings" onclick={()=>void dispatch('settings')}><Icon name="settings"/></button></div>
  </header>

  <nav class="project-rail" aria-label="Open projects"><span class="rail-label">PROJECTS</span><div class="project-tabs">{#each projects as item,index(item.id)}<div class="project-tab-group"><button id={`project-${item.id}`} class="project-tab" class:active={item.id===ui.projectId} aria-current={item.id===ui.projectId?'page':undefined} title={`${item.root} · Alt+${index+1}`} onclick={()=>void attempt(()=>selectProject(item.id))}><span class="channel-number">{String(index+1).padStart(2,'0')}</span><span>{item.name}</span><span class="project-indicator"></span></button><button class="flat icon-button project-close" aria-label={`Close project: ${item.name}`} title="Close project tab; keep sessions and work" onclick={()=>void attempt(()=>closeProjectTab(item.id))}><Icon name="close" size={14}/></button></div>{/each}</div><button class="key small add-project" aria-label="Open project" onclick={()=>ui.overlay='open'}><Icon name="plus" size={16}/><span>Open project</span></button></nav>

  {#if ui.fatal}<main class="startup-error"><Icon name="bolt" size={40}/><h1>The console could not connect</h1><p>{ui.fatal}</p><button class="key primary" onclick={()=>{ui.fatal='';void start();}}>Reconnect to gateway</button></main>
  {:else if ui.loading}<main class="startup-loading"><div class="skeleton-lines" aria-label="Loading workspace"><i></i><i></i><i></i></div><p>Connecting your workspace…</p></main>
  {:else}
    <nav class="mobile-panel-nav" aria-label="Workspace panels">{#each [['files','Files','folder'],['conversation','Session','chat'],['controls','Controls','settings']] as item}<button class:active={ui.panel===item[0]} onclick={()=>ui.panel=item[0]!}><Icon name={item[2]!} size={16}/>{item[1]}</button>{/each}</nav>
    <main class="workspace" class:no-explorer={!ui.preferences.explorer_visible} class:no-controls={!ui.preferences.controls_visible} data-panel={ui.panel}>
      <aside class="file-drawer" aria-label="Project drawer">
        <div class="drawer-switch"><button class:active={ui.drawer==='files'} onclick={()=>ui.drawer='files'}><Icon name="folder" size={16}/>Files</button><button class:active={ui.drawer==='git'} onclick={()=>{ui.drawer='git';void dispatch('git');}}><Icon name="git" size={16}/>Git<span class="count-chip">{ui.git?.changes.length??'—'}</span></button></div>
        {#if ui.drawer==='git'}<GitPanel/>{:else}<Explorer/>{/if}
      </aside>

      <section class="session-chassis" aria-label="Active session" class:file-drop-ready={fileDrag} ondragover={fileOver} ondragleave={event=>{if(!event.currentTarget.contains(event.relatedTarget as Node|null))fileDrag=false;}} ondrop={fileDrop}>
        {#if fileDrag}<div class="file-drop-label">Drop a text file to attach its saved snapshot to the next message</div>{/if}
        <div class="session-rails">
          {#each rows as row,depth}
            <div class="session-level" style={`--level:${depth}`}><span class="level-connector">{#if depth===0}<Icon name="layers" size={15}/>{:else}<Icon name="branch" size={15}/>{/if}</span><div class="session-tabs" style:grid-template-columns={row.length?`repeat(${row.length},max-content)`:'none'}>
              {#if row.length}<div class="session-tab-list" role="tablist" aria-label={depth?`Nested sessions level ${depth}`:'Sessions'}>
              {#each row as item,index(item.id)}<div class="session-tab" style:grid-column={index+1} class:active={lineage.some(a=>a.id===item.id)} class:current={item.id===ui.sessionId} class:drop-ready={dragged&&dragged!==item.id}>
                <button id={`session-${item.id}`} role="tab" aria-selected={lineage.some(a=>a.id===item.id)} tabindex={lineage.some(a=>a.id===item.id)?0:-1} title={`${item.title} · Drag onto another session to nest`} draggable="true" ondragstart={(event)=>{dragged=item.id;event.dataTransfer?.setData('text/peritus-session',item.id);}} ondragend={()=>dragged=''} ondragover={(event)=>event.preventDefault()} ondrop={(event)=>{event.preventDefault();const source=event.dataTransfer?.getData('text/peritus-session');if(source)void attempt(()=>editSession(source,{parent:item.id}));dragged='';}} onclick={()=>selectSession(item.id)} onkeydown={(event)=>tabKeys(event,row,index)}><span class="status-dot" class:busy={ui.conversations[item.id]?.run?.busy}></span><span class="session-tab-title">{item.title}</span></button>
               </div>{/each}
              </div>
              {#each row as item,index(item.id)}<button class="flat icon-button session-tab-close" style:grid-column={index+1} aria-label={`Close session: ${item.title}`} title="Close tab; keep work running" onclick={()=>void attempt(()=>closeTab(item.id))}><Icon name="close" size={14}/></button>{/each}{/if}
            </div><button class="flat icon-button" aria-label={depth?'Add nested session':'Add session'} onclick={()=>void attempt(()=>newSession(depth>0))}><Icon name="plus" size={16}/></button></div>
          {/each}
        </div>
        {#if activeSession}
          <div class="session-heading"><div><h1>{activeSession.title}</h1><span class="session-target" title={activeProject?.root}><Icon name="folder" size={13}/>{activeProject?.root}</span></div><div class="button-cluster"><button class="key small nest-button" aria-label="Nest session" onclick={()=>void attempt(()=>newSession(true))}><Icon name="branch" size={15}/>Nest session</button><button class="key icon-button" aria-label="Organize active session" onclick={()=>ui.overlay='session'}><Icon name="more"/></button></div></div>
          <div class="content-tabs" aria-label="Session content"><button class:active={!ui.activeFile} onclick={()=>ui.activeFile=''}><Icon name="chat" size={15}/>Conversation</button>{#each sessionFiles as file(file.path)}<div class="content-file-tab" class:active={ui.activeFile===file.path}><button title={file.path} onclick={()=>ui.activeFile=file.path}><Icon name="file" size={14}/>{file.path.split('/').pop()}</button><button aria-label={`Close ${file.path}`} onclick={()=>closeFile(file.path)}><Icon name="close" size={12}/></button></div>{/each}</div>
          {#if ui.activeFile}<FileViewer path={ui.activeFile} project={ui.projectId}/>
          {:else}
            {#if !admissionReady}<div class="connection-banner"><span><Icon name="link" size={15}/>{!ui.ready?ui.connectionMessage:ui.facts?.reason||ui.factsError||'Connect this project to start a conversation.'}</span><button class="key small" onclick={()=>void attempt(()=>dispatch('reconnect'))}>Recheck</button><button class="key small" onclick={()=>void attempt(()=>dispatch('terminal'))}>Set up project<Icon name="arrow" size={14}/></button></div>{/if}
            <Recovery/>
            <div class="transcript" bind:this={transcript} onscroll={()=>following=transcript.scrollHeight-transcript.scrollTop-transcript.clientHeight<80}>
              {#if !activities.length}
                <div class="ready-state" class:compact={!!draft||!!ui.attachments[ui.sessionId]?.length}>
                  <div class="ready-instrument"><Nixie value={Math.max(1,opened.findIndex(s=>s.id===ui.sessionId)+1)} label="Session channel" digits={3} large/><div class="instrument-legend"><span class="status-dot" class:offline={!messageReady}></span>{recoveryHold?'RECOVERY HOLD':!ui.connected?'AWAITING CONNECTION':messageReady?'CHANNEL READY':'NOT READY'}</div></div>
                  <h2>A clear channel.<br/>A new possibility.</h2><p>Bring a question, a stubborn bug, or your next big idea.<br class="desktop-break"/> Peritus takes it from here, with you at the controls.</p>
                  <div class="starter-commands"><button onclick={()=>{ui.modes[ui.sessionId]='plan';ui.drafts[ui.sessionId]='Help me understand this project and plan the next steps.';composer.focus();}}><Icon name="layers"/><span>Explore this project<small>Understand before changing</small></span><Icon name="arrow" size={16}/></button><button onclick={()=>{ui.modes[ui.sessionId]='review';ui.drafts[ui.sessionId]='Review this project for correctness and maintainability. Do not change files.';composer.focus();}}><Icon name="eye"/><span>Get a second opinion<small>Independent, read-only review</small></span><Icon name="arrow" size={16}/></button></div>
                  {#if !ui.connected&&ui.facts?.workspace}<div class="setup-prompt"><span>Start the harness to connect this channel.</span><button class="flat" onclick={()=>void attempt(()=>dispatch('terminal'))}>Open setup console<Icon name="arrow" size={14}/></button></div>{/if}
                </div>
              {:else}
                {#each activities as activity(activity.id)}<article class="message" class:user={activity.kind==='user'} class:system-message={!['user','assistant'].includes(activity.kind)}><div class="message-meta"><span class="message-avatar">{#if activity.kind==='assistant'}<Icon name="bolt" size={15}/>{:else if activity.kind==='user'}Y{:else}<Icon name="terminal" size={14}/>{/if}</span><strong>{activity.kind==='assistant'?'Peritus':activity.kind==='user'?'You':activity.kind}</strong><span class="message-sequence">{activity.id.padStart(3,'0')}</span></div><div class="message-content"><Markdown text={activity.text}/>{#if activity.detail}<details><summary>Details</summary><pre>{activity.detail}</pre></details>{/if}</div></article>{/each}
              {/if}
              {#if current?.run?.busy}<div class="working-observation"><span class="status-dot busy"></span>Peritus is working<span>{current.run.phase}</span></div>{/if}
            </div>
            <form class="composer" onsubmit={(event)=>{event.preventDefault();void attempt(send);}}>
              <Attachments/>
              {#if suggestions.length}<div class="slash-suggestions" aria-label="Slash command suggestions">{#each suggestions as command}<button type="button" onclick={()=>{ui.drafts[ui.sessionId]=command.slash;composer.focus();}}><code>{command.slash}</code><span>{command.label}</span></button>{/each}</div>{/if}
              <div class="composer-controls"><div class="mode-switch" aria-label="Conversation mode">{#each ['chat','plan','review','build'] as item}<button type="button" class:active={mode===item} aria-pressed={mode===item} onclick={()=>ui.modes[ui.sessionId]=item as Mode}>{item}</button>{/each}</div><button type="button" class="model-button flat" onclick={()=>ui.overlay='model'}><span class="status-dot neutral"></span>{current?.models?.writer?.id||activeSession?.settings?.models?.writer?.id||ui.facts?.providers[0]?.model||'Configure model'}<Icon name="down" size={13}/></button></div>
              <div class="composer-well"><textarea id="conversation-input" bind:this={composer} aria-label="Message Peritus or enter a slash command" value={draft} oninput={(event)=>ui.drafts[ui.sessionId]=event.currentTarget.value} placeholder={mode==='plan'?'What would you like to plan?':mode==='review'?'What would you like reviewed?':'What are we working on?'} rows="3" onkeydown={(event)=>{if(event.key==='Enter'&&!event.shiftKey&&!event.isComposing){event.preventDefault();void attempt(send);}if(event.key==='Tab'&&suggestions[0]){event.preventDefault();ui.drafts[ui.sessionId]=suggestions[0].slash+' ';}}}></textarea><button class="send-key key primary" type="submit" disabled={!draft.trim()||!!ui.pending[ui.sessionId]||!!ui.attaching[ui.sessionId]||(!draft.startsWith('/')&&!messageReady)} aria-label="Send message"><Icon name="arrow" size={24}/><span>{ui.pending[ui.sessionId]?'Sending':'Send'}</span></button></div>
              <div class="composer-foot"><button class="flat" type="button" onclick={()=>ui.palette=' '}><span class="slash-symbol">/</span>Commands</button><span>{mode==='plan'||mode==='review'?'Read-only tools':mode==='build'?'Checked delivery pipeline':'Conversation & directed work'}</span><span class="enter-hint"><kbd>Enter</kbd> send · <kbd>Shift Enter</kbd> new line</span></div>
            </form>
          {/if}
        {:else}<div class="viewer-empty"><h2>{activeProject?'Your work is still here.':'Open a project to get started.'}</h2><p>Open a saved session or start another conversation.</p>{#if activeProject}<button class="key primary" onclick={()=>void attempt(()=>newSession())}>New conversation</button>{:else}<button class="key primary" onclick={()=>ui.overlay='open'}>Open project</button>{/if}<button class="flat" onclick={()=>ui.overlay='sessions'}>Browse session library</button></div>{/if}
      </section>

      <aside class="control-bank" aria-label="Harness controls"><div class="control-heading"><h2>Control bank</h2><span class="engraved-symbol">P / 01</span></div>
        <div class="connection-module"><span class="status-lamp" class:online={ui.ready}></span><div><strong>{!ui.connected?'Daemon offline':ui.ready?'Daemon ready':'Daemon connected · not ready'}</strong><small>{ui.readiness}</small></div><button class="flat icon-button" aria-label="Reconnect daemon" onclick={()=>void attempt(()=>dispatch('reconnect'))}><Icon name="refresh" size={15}/></button></div>
        <div class="target-readout"><h3>Execution target</h3><span class="target-type"><Icon name="shield" size={14}/>{ui.facts?.workspace?.trust==='trusted'?'Trusted workspace':'Setup required'}</span><code>{ui.facts?.workspace?.execution||activeProject?.root}</code><button class="flat" onclick={()=>void attempt(()=>dispatch('workspaces'))}>Configure workspace<Icon name="arrow" size={13}/></button></div>
        <div class="run-controls"><div class="control-section-title"><h3>Session controls</h3><Icon name="bolt" size={14}/></div><button class="key stop-key" disabled={!current?.run?.busy} onclick={()=>void attempt(()=>dispatch('stop'))}><span class="stop-cap"><Icon name="stop" size={15}/></span>Stop work<kbd>/stop</kbd></button><div class="utility-keys">{#each [['details','Activity','layers'],['diff','Changes','git'],['runs','Runs','clock'],['terminal','Console','terminal']] as item}<button class="key" class:pressed={item[0]==='details'&&ui.details} onclick={()=>void attempt(()=>dispatch(item[0]!))}><Icon name={item[2]!}/>{item[1]}</button>{/each}</div></div>
        <div class="signal-readout"><div><span>State</span><strong>{current?.run?.phase||'Idle'}</strong></div><div><span>Input received</span><strong>{current?.received??'—'}</strong></div><div><span>Incorporated</span><strong>{current?.incorporated??'—'}</strong></div><p>Observed by the daemon. A received message may still be waiting for a model request.</p></div>
        <div class="advanced-controls"><h3>More control, when you need it.</h3><button class="flat" onclick={()=>void attempt(()=>dispatch('providers'))}><Icon name="settings" size={15}/>Providers & accounts<Icon name="chevron" size={13}/></button><button class="flat" onclick={()=>void attempt(()=>dispatch('sessions'))}><Icon name="layers" size={15}/>Session library<Icon name="chevron" size={13}/></button><button class="flat" onclick={()=>ui.palette=' '}><Icon name="terminal" size={15}/>All harness commands<Icon name="chevron" size={13}/></button></div>
        <div class="bank-nameplate"><span>PERITUS</span><small>LOCAL-FIRST / HUMAN-DIRECTED</small><i></i><i></i></div>
      </aside>
    </main>
  {/if}

  {#if ui.notice}<div class="notice" class:error={ui.noticeError} role={ui.noticeError?'alert':'status'}><Icon name={ui.noticeError?'help':'check'} size={17}/><span>{ui.notice}</span><button class="flat icon-button" aria-label="Dismiss notification" onclick={()=>ui.notice=''}><Icon name="close" size={15}/></button></div>{/if}
  <footer class="status-bar"><span><i class="status-dot" class:offline={!ui.connected}></i>{ui.connected?'Connected':'Offline'}<span class="status-separator">/</span><span>{activeProject?.name||'No project'}</span></span><button class="flat footer-git" onclick={()=>void attempt(()=>dispatch('git'))}><Icon name="git" size={13}/>{ui.git?.branch||'Repository not configured'}{#if ui.git?.changes.length}<span>{ui.git.changes.length} changes</span>{/if}</button><button class="flat shortcut-help" onclick={()=>ui.palette=' '}><Icon name="help" size={13}/>Keyboard & commands</button><span class="footer-version">PERITUS · 0.0.3</span></footer>
</div>
<Palette/><Overlays/>
