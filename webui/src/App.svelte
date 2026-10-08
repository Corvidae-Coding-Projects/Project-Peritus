<script lang="ts">
  import { onMount,tick,untrack } from 'svelte';
  import { ui,start,cancelStartup,persist,flushPersist,project,session,attempt,dispatch,selectProject,selectSession,newSession,editSession,openFile,closeFile,send,poll,notify,openProjects,closeProject,attachFile,runActive,cancelObservations,recheckObservations } from './lib/workspace.svelte';
  import { commands,matchesShortcut,formatShortcut } from './lib/commands/catalog';
  import type { RunLegalControls,Session,Mode } from './lib/types';
  import Icon from './lib/components/Icon.svelte';
  import Nixie from './lib/components/Nixie.svelte';
  import Explorer from './lib/components/Explorer.svelte';
  import GitPanel from './lib/components/GitPanel.svelte';
  import FileViewer from './lib/components/FileViewer.svelte';
  import Markdown from './lib/components/Markdown.svelte';
  import ActivityDetails from './lib/components/ActivityDetails.svelte';
  import Palette from './lib/components/Palette.svelte';
  import Overlays from './lib/components/Overlays.svelte';
  import {flushFileDrafts,protectFileDrafts} from './lib/files/drafts.svelte';
  import {browserStorage} from './lib/storage.svelte';
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
  const TAB_RENDER_BATCH=16;
  let projectTabStart=$state(0),fileTabStart=$state(0),railOffsets=$state<Record<string,number>>({});
  let visibleProjects=$derived(projects.slice(projectTabStart,projectTabStart+TAB_RENDER_BATCH));
  let projectFocusKey='',sessionFocusKey='',fileFocusKey='';
  let opened=$derived.by(()=>{const ids=new Set(projects.map(item=>item.id));return ui.workspace.sessions.filter(item=>!item.closed&&ids.has(item.project));});
  let sessionLookup=$derived(new Map(ui.workspace.sessions.map(item=>[item.id,item])));
  let running=$derived(Object.values(ui.conversations).filter(c=>runActive(c.run)).length);
  const ACTIVITY_RENDER_BATCH=32;
  let activityOffsets=$state<Record<string,number|undefined>>({});
  const activityAnchors=new Map<string,string>();
  const activityFrontiers=new Map<string,{first:string;length:number}>();
  const operationActions:{control:keyof RunLegalControls;command:string;label:string}[]=[
    {control:'retry',command:'retry',label:'Exact retry'},
    {control:'accept',command:'accept',label:'Accept candidate'},
    {control:'commit',command:'commit',label:'Commit candidate'},
    {control:'export',command:'export',label:'Export candidate'},
    {control:'discard',command:'discard',label:'Discard candidate'},
    {control:'acknowledge',command:'acknowledge',label:'Acknowledge unknown outcome'},
  ];
  let sessionFiles=$derived(ui.files.filter(f=>f.session===ui.sessionId));
  let visibleFiles=$derived(sessionFiles.slice(fileTabStart,fileTabStart+TAB_RENDER_BATCH));
  let activitySource=$derived(current?.activities??[]);
  let activityKey=$derived(JSON.stringify([browserStorage.workspace,ui.sessionId]));
  let activityStart=$derived(Math.min(activityOffsets[activityKey]??Math.max(0,activitySource.length-ACTIVITY_RENDER_BATCH),Math.max(0,activitySource.length-1)));
  let activityEnd=$derived(Math.min(activitySource.length,activityStart+ACTIVITY_RENDER_BATCH));
  let activities=$derived(activitySource.slice(activityStart,activityEnd).filter(a=>ui.details||['user','assistant','error'].includes(a.kind)));
  let suggestions=$derived(draft.startsWith('/')&&!draft.includes(' ')?commands.filter(c=>c.slash.startsWith(draft)).slice(0,7):[]);
  let lineage=$derived.by(()=>{
    const result:Session[]=[];let cursor=activeSession;const seen=new Set<string>();
    while(cursor&&!seen.has(cursor.id)){seen.add(cursor.id);result.push(cursor);cursor=cursor.parent?sessionLookup.get(cursor.parent):undefined;}return result.reverse();
  });
  let rows=$derived.by(()=>{
    const byParent=new Map<string|null,Session[]>();
    for(const item of opened){if(item.project!==ui.projectId)continue;let group=byParent.get(item.parent);if(!group){group=[];byParent.set(item.parent,group);}group.push(item);}
    const groups=[byParent.get(null)??[]];for(const ancestor of lineage){const children=byParent.get(ancestor.id);if(children?.length)groups.push(children);}return groups;
  });
  function railKey(depth:number){return JSON.stringify([browserStorage.workspace,ui.projectId,rows[depth]?.[0]?.parent??null]);}
  let visibleRows=$derived(rows.map((items,depth)=>{
    const key=railKey(depth),start=Math.min(railOffsets[key]??0,Math.max(0,Math.floor((items.length-1)/TAB_RENDER_BATCH)*TAB_RENDER_BATCH));
    return {key,start,items,visible:items.slice(start,start+TAB_RENDER_BATCH)};
  }));
  $effect(()=>{
    const key=JSON.stringify([browserStorage.workspace,ui.projectId]),items=projects;
    if(key!==projectFocusKey){projectFocusKey=key;const index=items.findIndex(item=>item.id===ui.projectId);projectTabStart=Math.max(0,Math.floor(index/TAB_RENDER_BATCH)*TAB_RENDER_BATCH);}
    if(projectTabStart>=items.length)projectTabStart=Math.max(0,Math.floor((items.length-1)/TAB_RENDER_BATCH)*TAB_RENDER_BATCH);
  });
  $effect(()=>{
    const key=JSON.stringify([browserStorage.workspace,ui.projectId,ui.sessionId]),levels=rows;
    if(key!==sessionFocusKey){
      sessionFocusKey=key;const active=new Set(lineage.map(item=>item.id));
      for(let depth=0;depth<levels.length;depth++){const index=levels[depth]!.findIndex(item=>active.has(item.id));railOffsets[railKey(depth)]=Math.max(0,Math.floor(index/TAB_RENDER_BATCH)*TAB_RENDER_BATCH);}
    }
  });
  $effect(()=>{
    const key=JSON.stringify([browserStorage.workspace,ui.sessionId,ui.activeFile]),items=sessionFiles;
    if(key!==fileFocusKey){fileFocusKey=key;const index=items.findIndex(item=>item.path===ui.activeFile);fileTabStart=Math.max(0,Math.floor(index/TAB_RENDER_BATCH)*TAB_RENDER_BATCH);}
    if(fileTabStart>=items.length)fileTabStart=Math.max(0,Math.floor((items.length-1)/TAB_RENDER_BATCH)*TAB_RENDER_BATCH);
  });
  onMount(()=>{
    apple=/Mac|iPhone|iPad|iPod/.test(navigator.platform);
    void start();let disposed=false,timer:ReturnType<typeof setTimeout>;
    function update(){if(!document.hidden&&!ui.loading)poll();if(!disposed)timer=setTimeout(update,3500);}
    timer=setTimeout(update,3500);return()=>{disposed=true;clearTimeout(timer);cancelStartup();cancelObservations(false);};
  });
  $effect(()=>{if(!ui.loading)persist();});
  $effect(()=>{
    const p=ui.preferences;const root=document.documentElement;root.dataset.theme=p.theme;root.dataset.density=p.density;root.dataset.motion=p.motion?'full':'reduced';
    root.style.setProperty('--base-font',`${p.font_size}px`);root.style.setProperty('--font-body',p.font_family);root.style.setProperty('--font-mono',p.mono_family);root.style.setProperty('--explorer-width',`min(${p.explorer_width}px, 40vw)`);
    for(const role of ['background','panel','display','text','muted','accent','line']){if(p.tokens[role])root.style.setProperty(`--${role}`,p.tokens[role]!);else root.style.removeProperty(`--${role}`);}
  });
  $effect(()=>{const latest=activitySource.at(-1)?.id;if(following&&activityOffsets[activityKey]===undefined&&latest)void tick().then(()=>transcript?.scrollTo({top:transcript.scrollHeight}));});
  $effect(()=>{
    const key=activityKey,first=activitySource[0]?.id??'',length=activitySource.length,previous=activityFrontiers.get(key);
    const offset=untrack(()=>activityOffsets[key]);
    if(offset!==undefined&&previous&&(length<previous.length||first!==previous.first)){
      const anchor=activityAnchors.get(key),found=anchor?activitySource.findIndex(activity=>activity.id===anchor):-1;
      activityOffsets[key]=found>=0?found:Math.min(offset,Math.max(0,length-ACTIVITY_RENDER_BATCH));
    }
    activityFrontiers.set(key,{first,length});
  });
  async function revealEarlierActivities(){
    const key=activityKey,start=Math.max(0,activityStart-ACTIVITY_RENDER_BATCH);
    activityOffsets[key]=start;activityAnchors.set(key,activitySource[start]?.id??'');following=false;
    await tick();transcript?.scrollTo({top:0});
  }
  async function revealLaterActivities(latest=false){
    const key=activityKey,start=Math.min(activitySource.length,activityStart+ACTIVITY_RENDER_BATCH);
    if(latest||start+ACTIVITY_RENDER_BATCH>=activitySource.length){activityOffsets[key]=undefined;activityAnchors.delete(key);following=true;}
    else{activityOffsets[key]=start;activityAnchors.set(key,activitySource[start]?.id??'');following=false;}
    await tick();transcript?.scrollTo({top:following?transcript.scrollHeight:0});
  }
  function flushBrowserState(){flushPersist();flushFileDrafts();}
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

<svelte:window onkeydown={keyboard} onclick={physical} onpagehide={flushBrowserState} onbeforeunload={protectFileDrafts}/>
<a class="skip-link" href="#conversation-input">Skip to message composer</a>
<div class="console-shell">
  <header class="top-plate">
    <a class="brand" href="/" aria-label="Peritus control console"><span class="brand-mark"><svg viewBox="0 0 36 40" aria-hidden="true"><path d="M7 34V6h15a9 9 0 0 1 0 18H7m10-18v28"/></svg></span><span><strong>PERITUS</strong><small>HARNESS CONTROL CONSOLE</small></span></a>
    <div class="header-instruments"><Nixie value={projects.length} label="Projects"/><Nixie value={opened.length} label="Sessions"/><Nixie value={running} label="Running"/></div>
    <div class="header-actions"><button class="command-key key" aria-label="Command directory" onclick={()=>ui.palette=' '}><Icon name="terminal"/><span>Command</span>{#if commandShortcut}<kbd>{commandShortcut}</kbd>{/if}</button><button class="key icon-button" aria-label="Console settings" onclick={()=>void dispatch('settings')}><Icon name="settings"/></button></div>
  </header>

  <nav class="project-rail" aria-label="Open projects"><span class="rail-label">PROJECTS</span>{#if projects.length>TAB_RENDER_BATCH}<button class="flat icon-button" aria-label="Previous projects" disabled={!projectTabStart} onclick={()=>projectTabStart=Math.max(0,projectTabStart-TAB_RENDER_BATCH)}><Icon name="chevron" size={14}/></button>{/if}<div class="project-tabs">{#each visibleProjects as item,index(item.id)}<div class="project-tab-group"><button id={`project-${item.id}`} class="project-tab" class:active={item.id===ui.projectId} aria-current={item.id===ui.projectId?'page':undefined} title={`${item.root}${projectTabStart+index<9?` · Alt+${projectTabStart+index+1}`:''}`} onclick={()=>void attempt(()=>selectProject(item.id))}><span class="channel-number">{String(projectTabStart+index+1).padStart(2,'0')}</span><span>{item.name}</span><span class="project-indicator"></span></button><button class="flat icon-button project-close" aria-label={`Close project: ${item.name}`} title="Close project tab; keep sessions and work" onclick={()=>void attempt(()=>closeProjectTab(item.id))}><Icon name="close" size={14}/></button></div>{/each}</div>{#if projects.length>TAB_RENDER_BATCH}<span class="rail-label">{projectTabStart+1}–{projectTabStart+visibleProjects.length} / {projects.length.toLocaleString()}</span><button class="flat icon-button" aria-label="Next projects" disabled={projectTabStart+visibleProjects.length>=projects.length} onclick={()=>projectTabStart+=TAB_RENDER_BATCH}><Icon name="chevron" size={14}/></button>{/if}<button class="key small add-project" aria-label="Open project" onclick={()=>ui.overlay='open'}><Icon name="plus" size={16}/><span>Open project</span></button></nav>

  {#if ui.fatal}<main class="startup-error"><Icon name="bolt" size={40}/><h1>The console could not connect</h1><p>{ui.fatal}</p><button class="key primary" onclick={()=>{ui.fatal='';void start();}}>Reconnect to gateway</button><Recovery showAll/></main>
  {:else if ui.loading}<main class="startup-loading"><div class="skeleton-lines" aria-label="Loading workspace"><i></i><i></i><i></i></div><p>{ui.startupMessage}</p>{#if ui.startupActive}<button class="key small" onclick={cancelStartup}>Cancel gateway check</button>{:else}<button class="key primary" onclick={()=>void start()}>Reconnect to gateway</button>{/if}<Recovery showAll/></main>
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
          {#each visibleRows as rail,depth(rail.key)}
            <div class="session-level" style={`--level:${depth}`}><span class="level-connector">{#if depth===0}<Icon name="layers" size={15}/>{:else}<Icon name="branch" size={15}/>{/if}</span>{#if rail.items.length>TAB_RENDER_BATCH}<button class="flat icon-button" aria-label={`Previous sessions at level ${depth}`} disabled={!rail.start} onclick={()=>railOffsets[rail.key]=Math.max(0,rail.start-TAB_RENDER_BATCH)}><Icon name="chevron" size={14}/></button>{/if}<div class="session-tabs" style:grid-template-columns={rail.visible.length?`repeat(${rail.visible.length},max-content)`:'none'}>
              {#if rail.visible.length}<div class="session-tab-list" role="tablist" aria-label={depth?`Nested sessions level ${depth}`:'Sessions'}>
              {#each rail.visible as item,index(item.id)}<div class="session-tab" style:grid-column={index+1} class:active={lineage.some(a=>a.id===item.id)} class:current={item.id===ui.sessionId} class:drop-ready={dragged&&dragged!==item.id}>
                <button id={`session-${item.id}`} role="tab" aria-selected={lineage.some(a=>a.id===item.id)} tabindex={lineage.some(a=>a.id===item.id)?0:-1} title={`${item.title} · Drag onto another session to nest`} draggable="true" ondragstart={(event)=>{dragged=item.id;event.dataTransfer?.setData('text/peritus-session',item.id);}} ondragend={()=>dragged=''} ondragover={(event)=>event.preventDefault()} ondrop={(event)=>{event.preventDefault();const source=event.dataTransfer?.getData('text/peritus-session');if(source)void attempt(()=>editSession(source,{parent:item.id}));dragged='';}} onclick={()=>selectSession(item.id)} onkeydown={(event)=>tabKeys(event,rail.items,rail.start+index)}><span class="status-dot" class:busy={runActive(ui.conversations[item.id]?.run)}></span><span class="session-tab-title">{item.title}</span></button>
               </div>{/each}
              </div>
              {#each rail.visible as item,index(item.id)}<button class="flat icon-button session-tab-close" style:grid-column={index+1} aria-label={`Close session: ${item.title}`} title="Close tab; keep work running" onclick={()=>void attempt(()=>closeTab(item.id))}><Icon name="close" size={14}/></button>{/each}{/if}
            </div>{#if rail.items.length>TAB_RENDER_BATCH}<span class="rail-label">{rail.start+1}–{rail.start+rail.visible.length} / {rail.items.length.toLocaleString()}</span><button class="flat icon-button" aria-label={`Next sessions at level ${depth}`} disabled={rail.start+rail.visible.length>=rail.items.length} onclick={()=>railOffsets[rail.key]=rail.start+TAB_RENDER_BATCH}><Icon name="chevron" size={14}/></button>{/if}<button class="flat icon-button" aria-label={depth?'Add nested session':'Add session'} onclick={()=>void attempt(()=>newSession(depth>0))}><Icon name="plus" size={16}/></button></div>
          {/each}
        </div>
        {#if activeSession}
          <div class="session-heading"><div><h1>{activeSession.title}</h1><span class="session-target" title={activeProject?.root}><Icon name="folder" size={13}/>{activeProject?.root}</span></div><div class="button-cluster"><button class="key small nest-button" aria-label="Nest session" onclick={()=>void attempt(()=>newSession(true))}><Icon name="branch" size={15}/>Nest session</button><button class="key icon-button" aria-label="Organize active session" onclick={()=>ui.overlay='session'}><Icon name="more"/></button></div></div>
          <div class="content-tabs" aria-label="Session content"><button class:active={!ui.activeFile} onclick={()=>ui.activeFile=''}><Icon name="chat" size={15}/>Conversation</button>{#if sessionFiles.length>TAB_RENDER_BATCH}<button aria-label="Previous file tabs" disabled={!fileTabStart} onclick={()=>fileTabStart=Math.max(0,fileTabStart-TAB_RENDER_BATCH)}><Icon name="chevron" size={14}/></button>{/if}{#each visibleFiles as file(file.path)}<div class="content-file-tab" class:active={ui.activeFile===file.path}><button title={file.path} onclick={()=>ui.activeFile=file.path}><Icon name="file" size={14}/>{file.path.split('/').pop()}</button><button aria-label={`Close ${file.path}`} onclick={()=>closeFile(file.path)}><Icon name="close" size={12}/></button></div>{/each}{#if sessionFiles.length>TAB_RENDER_BATCH}<span class="rail-label">{fileTabStart+1}–{fileTabStart+visibleFiles.length} / {sessionFiles.length.toLocaleString()}</span><button aria-label="Next file tabs" disabled={fileTabStart+visibleFiles.length>=sessionFiles.length} onclick={()=>fileTabStart+=TAB_RENDER_BATCH}><Icon name="chevron" size={14}/></button>{/if}</div>
          {#if ui.activeFile}<FileViewer path={ui.activeFile} project={ui.projectId}/>
          {:else}
            {#if !admissionReady}<div class="connection-banner"><span><Icon name="link" size={15}/>{!ui.ready?ui.connectionMessage:ui.facts?.reason||ui.factsError||'Connect this project to start a conversation.'}</span><button class="key small" onclick={recheckObservations}>{ui.observationsPaused?'Resume checks':'Recheck'}</button>{#if ui.observing}<button class="key small" onclick={()=>cancelObservations()}>Cancel checks</button>{/if}<button class="key small" onclick={()=>void attempt(()=>dispatch('terminal'))}>Set up project<Icon name="arrow" size={14}/></button></div>{/if}
            <Recovery/>
            {#if current?.workbench}<div class="connection-banner" role="status"><span><Icon name="terminal" size={15}/><strong>{current.workbench.observation}</strong> {current.workbench.action}<small>{current.workbench.detail}</small></span><button class="key small" onclick={()=>void attempt(()=>dispatch('terminal'))}>Open Workbench<Icon name="arrow" size={14}/></button></div>{/if}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex (the scrollable transcript needs a keyboard focus target) -->
            <div class="transcript" role="region" aria-label="Conversation transcript" tabindex="0" bind:this={transcript} onscroll={()=>following=transcript.scrollHeight-transcript.scrollTop-transcript.clientHeight<80}>
              {#if !activitySource.length}
                <div class="ready-state" class:compact={!!draft||!!ui.attachments[ui.sessionId]?.length}>
                  <div class="ready-instrument"><Nixie value={Math.max(1,opened.findIndex(s=>s.id===ui.sessionId)+1)} label="Session channel" digits={3} large/><div class="instrument-legend"><span class="status-dot" class:offline={!messageReady}></span>{recoveryHold?'RECOVERY HOLD':!ui.connected?'AWAITING CONNECTION':messageReady?'CHANNEL READY':'NOT READY'}</div></div>
                  <h2>A clear channel.<br/>A new possibility.</h2><p>Bring a question, a stubborn bug, or your next big idea.<br class="desktop-break"/> Peritus takes it from here, with you at the controls.</p>
                  <div class="starter-commands"><button onclick={()=>{ui.modes[ui.sessionId]='plan';ui.drafts[ui.sessionId]='Help me understand this project and plan the next steps.';composer.focus();}}><Icon name="layers"/><span>Explore this project<small>Understand before changing</small></span><Icon name="arrow" size={16}/></button><button onclick={()=>{ui.modes[ui.sessionId]='review';ui.drafts[ui.sessionId]='Review this project for correctness and maintainability. Do not change files.';composer.focus();}}><Icon name="eye"/><span>Get a second opinion<small>Independent, read-only review</small></span><Icon name="arrow" size={16}/></button></div>
                  {#if !ui.connected&&ui.facts?.workspace}<div class="setup-prompt"><span>Start the harness to connect this channel.</span><button class="flat" onclick={()=>void attempt(()=>dispatch('terminal'))}>Open setup console<Icon name="arrow" size={14}/></button></div>{/if}
                </div>
              {:else}
                <div class="activity-history">{#if activityStart}<button class="key small" onclick={()=>void revealEarlierActivities()}>Show earlier activity <span>{activityStart.toLocaleString()} retained</span></button>{/if}<span>Activity {(activityStart+1).toLocaleString()}–{activityEnd.toLocaleString()} of {activitySource.length.toLocaleString()}</span></div>
                {#if !activities.length}<p class="small-empty">This page contains tool or status activity. Enable Activity to view those entries.</p>{/if}
                {#each activities as activity(activity.id)}<article class="message" class:user={activity.kind==='user'} class:system-message={!['user','assistant'].includes(activity.kind)}><div class="message-meta"><span class="message-avatar">{#if activity.kind==='assistant'}<Icon name="bolt" size={15}/>{:else if activity.kind==='user'}Y{:else}<Icon name="terminal" size={14}/>{/if}</span><strong>{activity.kind==='assistant'?'Peritus':activity.kind==='user'?'You':activity.kind}</strong><span class="message-sequence">{activity.id.padStart(3,'0')}</span></div><div class="message-content"><Markdown text={activity.text}/>{#if activity.detail}<ActivityDetails text={activity.detail}/>{/if}</div></article>{/each}
                {#if activityEnd<activitySource.length}<div class="activity-history"><button class="key small" onclick={()=>void revealLaterActivities()}>Show later activity</button><button class="key small" onclick={()=>void revealLaterActivities(true)}>Return to latest activity</button></div>{/if}
              {/if}
              {#if runActive(current?.run)}<div class="working-observation"><span class="status-dot busy"></span>Peritus is working<span>{current?.run?.operation.state}</span></div>{/if}
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
        <div class="connection-module"><span class="status-lamp" class:online={ui.ready}></span><div><strong>{!ui.connected?'Daemon offline':ui.ready?'Daemon ready':'Daemon connected · not ready'}</strong><small>{ui.readiness}</small></div><button class="flat icon-button" aria-label={ui.observationsPaused?'Resume daemon observations':ui.observing?'Cancel daemon observations':'Recheck daemon observations'} onclick={()=>ui.observing?cancelObservations():recheckObservations()}><Icon name={ui.observing?'stop':'refresh'} size={15}/></button></div>
        <div class="target-readout"><h3>Execution target</h3><span class="target-type"><Icon name="shield" size={14}/>{ui.facts?.workspace?.trust==='trusted'?'Trusted workspace':'Setup required'}</span><code>{ui.facts?.workspace?.execution||activeProject?.root}</code><button class="flat" onclick={()=>void attempt(()=>dispatch('workspaces'))}>Configure workspace<Icon name="arrow" size={13}/></button></div>
        <div class="run-controls"><div class="control-section-title"><h3>Session controls</h3><Icon name="bolt" size={14}/></div><button class="key stop-key" disabled={!current?.run?.operation.legalControls.stop} onclick={()=>void attempt(()=>dispatch('stop'))}><span class="stop-cap"><Icon name="stop" size={15}/></span>Stop work<kbd>/stop</kbd></button><div class="utility-keys">{#each [['details','Activity','layers'],['diff','Changes','git'],['runs','Runs','clock'],['terminal','Console','terminal']] as item}<button class="key" class:pressed={item[0]==='details'&&ui.details} onclick={()=>void attempt(()=>dispatch(item[0]!))}><Icon name={item[2]!}/>{item[1]}</button>{/each}</div></div>
        {#if current?.run}<div class="operation-observation" class:uncertain={!!current.run.operation.uncertainty}><div class="control-section-title"><h3>Authoritative operation</h3><span>{current.run.operation.kind}</span></div><strong>{current.run.operation.state}</strong><p><b>Known</b>{current.run.operation.known}</p>{#if current.run.operation.uncertainty}<p class="uncertainty"><b>Uncertain</b>{current.run.operation.uncertainty}</p>{/if}<code>{current.run.operation.identity}</code><div class="operation-actions">{#each operationActions.filter(action=>current!.run!.operation.legalControls[action.control]) as action}<button class="key small" onclick={()=>void attempt(()=>dispatch(action.command))}>{action.label}</button>{/each}{#if !Object.values(current.run.operation.legalControls).some(Boolean)}<small>No operation controls are legal at this observation.</small>{/if}</div></div>{/if}
        <div class="signal-readout"><div><span>State</span><strong>{current?.run?.operation.state||'Idle'}</strong></div><div><span>Input received</span><strong>{current?.received??'—'}</strong></div><div><span>Incorporated</span><strong>{current?.incorporated??'—'}</strong></div><p>Observed by the daemon. A received message may still be waiting for a model request.</p></div>
        <div class="advanced-controls"><h3>More control, when you need it.</h3><button class="flat" onclick={()=>void attempt(()=>dispatch('providers'))}><Icon name="settings" size={15}/>Providers & accounts<Icon name="chevron" size={13}/></button><button class="flat" onclick={()=>void attempt(()=>dispatch('sessions'))}><Icon name="layers" size={15}/>Session library<Icon name="chevron" size={13}/></button><button class="flat" onclick={()=>void attempt(()=>dispatch('improvements'))}><Icon name="layers" size={15}/>Improvement inbox<Icon name="chevron" size={13}/></button><button class="flat" onclick={()=>ui.palette=' '}><Icon name="terminal" size={15}/>All harness commands<Icon name="chevron" size={13}/></button></div>
        <div class="bank-nameplate"><span>PERITUS</span><small>LOCAL-FIRST / HUMAN-DIRECTED</small><i></i><i></i></div>
      </aside>
    </main>
  {/if}

  {#if ui.notice}<div class="notice" class:error={ui.noticeError} role={ui.noticeError?'alert':'status'}><Icon name={ui.noticeError?'help':'check'} size={17}/><span>{ui.notice}</span><button class="flat icon-button" aria-label="Dismiss notification" onclick={()=>ui.notice=''}><Icon name="close" size={15}/></button></div>{/if}
  <footer class="status-bar"><span><i class="status-dot" class:offline={!ui.connected}></i>{ui.connected?'Connected':'Offline'}<span class="status-separator">/</span><span>{activeProject?.name||'No project'}</span></span><button class="flat footer-git" onclick={()=>void attempt(()=>dispatch('git'))}><Icon name="git" size={13}/>{ui.git?.branch||'Repository not configured'}{#if ui.git?.changes.length}<span>{ui.git.changes.length} changes</span>{/if}</button><button class="flat shortcut-help" onclick={()=>ui.palette=' '}><Icon name="help" size={13}/>Keyboard & commands</button><span class="footer-version">PERITUS · 0.0.5</span></footer>
</div>
<Palette/><Overlays/>
