<script lang="ts">
  import { onMount } from 'svelte';
  import { query, action } from '../api';
  import { browserStorage } from '../storage.svelte';
  import { ui, attempt, openEvaluation, openRun, notify, runActive } from '../workspace.svelte';
  import ImprovementText from './ImprovementText.svelte';
  import type {
    ImprovementCandidate, ImprovementEvidence, ImprovementEvidencePage, ImprovementPage,
    Run, RunPage
  } from '../types';

  interface CandidateView extends ImprovementCandidate {
    evidence:ImprovementEvidence[];
    evidenceNext:string|null;evidenceStarted:boolean;evidenceLoading:boolean;
  }

  let workspace = $state(''), revision = $state(''), next = $state<string|null>(null);
  let candidates = $state<CandidateView[]>([]), runs = $state<Run[]>([]);
  let runsNext = $state<string|null>(null), runsStore=$state(''), runsPaging=$state(false);
  let proposal = $state(''), source = $state(''), target = $state('');
  let busy = $state(false), paging = $state(false), error = $state('');
  let showDismissed = $state(false);
  const project = ui.projectId;
  let generation=0,disposed=false,ownerWorkspace='';
  let inboxController:AbortController|undefined,runController:AbortController|undefined;
  const evidenceControllers=new Map<string,AbortController>();

  function owns(current:number,controller:AbortController) {
    return !disposed&&current===generation&&!controller.signal.aborted
      &&project===ui.projectId&&ownerWorkspace===browserStorage.workspace;
  }

  function cancelMetadata() {
    inboxController?.abort();runController?.abort();
    for(const controller of evidenceControllers.values())controller.abort();
    evidenceControllers.clear();
    paging=false;runsPaging=false;
    for(const item of candidates)item.evidenceLoading=false;
  }

  function candidateView(item:ImprovementCandidate):CandidateView {
    return {...item,evidence:[],evidenceNext:null,
      evidenceStarted:false,evidenceLoading:false};
  }

  function acceptPage(page:ImprovementPage, replace:boolean) {
    if(!replace&&(page.workspace!==workspace||page.revision!==revision))
      throw new Error('The suggestion history changed. Refresh it before loading another page.');
    workspace=page.workspace;revision=page.revision;next=page.next;
    const added=page.candidates.map(candidateView);
    candidates=replace?added:[...candidates,...added];
  }

  async function refresh() {
    const current=++generation;cancelMetadata();ownerWorkspace=browserStorage.workspace;error='';
    const inbox=new AbortController(),runPage=new AbortController();
    inboxController=inbox;runController=runPage;
    void query<ImprovementPage>('improvements',{project},{signal:inbox.signal}).then(page=>{
      if(owns(current,inbox))acceptPage(page,true);
    }).catch(value=>{
      if(owns(current,inbox))error=value instanceof Error?value.message:String(value);
    }).finally(()=>{if(owns(current,inbox)&&inboxController===inbox)inboxController=undefined;});
    void query<RunPage>('runs',{}, {signal:runPage.signal}).then(page=>{
      if(!owns(current,runPage))return;
      runs=page.runs;runsNext=page.cursor;runsStore=page.store;
    }).catch(value=>{
      if(owns(current,runPage))error=value instanceof Error?value.message:String(value);
    }).finally(()=>{if(owns(current,runPage)&&runController===runPage)runController=undefined;});
  }

  async function moreCandidates() {
    if(!next||paging)return;
    const current=generation,controller=new AbortController();inboxController?.abort();inboxController=controller;paging=true;
    try {const page=await query<ImprovementPage>('improvements',{project,cursor:next},{signal:controller.signal});if(owns(current,controller))acceptPage(page,false);}
    catch(e) {if(owns(current,controller))error=e instanceof Error?e.message:String(e);}
    finally {if(owns(current,controller)){paging=false;if(inboxController===controller)inboxController=undefined;}}
  }

  async function moreRuns() {
    if(!runsNext||runsPaging)return;
    const cursor=runsNext,store=runsStore,current=generation,controller=new AbortController();
    runController?.abort();runController=controller;runsPaging=true;
    try {
      const page=await query<RunPage>('runs',{cursor},{signal:controller.signal});
      if(!owns(current,controller))return;
      if(page.store!==store)throw new Error('Run history changed durable stores. Refresh the inbox.');
      const seen=new Set(runs.map(run=>run.id));
      if(page.runs.some(run=>seen.has(run.id)))throw new Error('The daemon returned an overlapping run page. Refresh the inbox.');
      runs=[...runs,...page.runs];runsNext=page.cursor;
    } catch(e) {if(owns(current,controller))error=e instanceof Error?e.message:String(e);}
    finally {if(owns(current,controller)){runsPaging=false;if(runController===controller)runController=undefined;}}
  }

  async function moreEvidence(item:CandidateView) {
    if(item.evidenceLoading||(item.evidenceStarted&&item.evidenceNext===null))return;
    const current=generation,controller=new AbortController();
    evidenceControllers.get(item.id)?.abort();evidenceControllers.set(item.id,controller);item.evidenceLoading=true;
    try {
      const page=await query<ImprovementEvidencePage>('improvement-evidence',{
        project,candidate:item.id,revision,cursor:item.evidenceNext??''
      },{signal:controller.signal});
      if(!owns(current,controller)||!candidates.includes(item))return;
      if(page.workspace!==workspace||page.candidate!==item.id||page.revision!==revision)
        throw new Error('The suggestion changed. Refresh it before inspecting evidence.');
      const added=page.evidence;
      item.evidence=[...item.evidence,...added];item.evidenceNext=page.next;item.evidenceStarted=true;
    } catch(e) {if(owns(current,controller)&&candidates.includes(item))error=e instanceof Error?e.message:String(e);}
    finally {if(owns(current,controller)&&candidates.includes(item)){item.evidenceLoading=false;if(evidenceControllers.get(item.id)===controller)evidenceControllers.delete(item.id);}}
  }

  async function mutate(input:Record<string,unknown>):Promise<ImprovementPage> {
    busy=true;
    try {
      const page=await action<ImprovementPage>('improvements',{project,...input});
      generation++;cancelMetadata();acceptPage(page,true);return page;
    }
    finally {busy=false;}
  }

  async function evaluate(item:CandidateView) {
    const page=await mutate({action:'evaluate',candidate:item.id,target});
    const evaluation=page.candidates.find(value=>value.id===item.id)?.evaluation;
    if(!evaluation)throw new Error('The accepted evaluation result did not contain its run binding.');
    if(evaluation)await openEvaluation(evaluation);
  }

  onMount(()=>{void refresh();return()=>{disposed=true;generation++;cancelMetadata();};});
</script>

<p class="dialog-description">Collect evidence-backed suggestions here. Generate a patch and run tests only when you choose to evaluate one.</p>
{#if error}<p role="alert">{error}</p><button class="key" onclick={()=>void refresh()}>Retry loading inbox</button>{/if}
<form onsubmit={(event)=>{event.preventDefault();void attempt(async()=>{await mutate({action:'suggest',run:source,proposal});proposal='';notify('Suggestion saved. No evaluation started.');});}}>
  <label for="improvement-source">Evidence run</label><select id="improvement-source" bind:value={source} required><option value="">Select a completed run</option>{#each runs.filter(run=>run.workspace===workspace&&!runActive(run)) as run}<option value={run.id}>{run.task.slice(0,90)} · {run.phase}</option>{/each}</select>
  {#if runsNext}<button type="button" class="flat" disabled={runsPaging} onclick={()=>void moreRuns()}>{runsPaging?'Loading runs…':'Load more evidence runs'}</button>{/if}
  <label>Improvement suggestion<textarea bind:value={proposal} required rows="3" placeholder="What might improve the harness, and why does this run support investigating it?"></textarea></label>
  <button class="key" disabled={busy||!source||!proposal.trim()}>Collect suggestion</button>
</form>
<div class="evaluation-target"><label for="improvement-target">Peritus source workspace</label><select id="improvement-target" bind:value={target}><option value="">Choose where to generate and test patches</option>{#each ui.workspace.projects.filter(value=>!value.closed) as targetProject}<option value={targetProject.id}>{targetProject.name} · {targetProject.root}</option>{/each}</select><p class="setting-note">Select a registered Git checkout of Peritus. Evaluation uses its configured provider and the runner's resource controls. You can stop the run, inspect its patch and checks, then explicitly export or commit it. The running harness is never updated automatically.</p></div>
<label class="dismissed"><input type="checkbox" bind:checked={showDismissed}/> Show dismissed suggestions</label>
{#each candidates.filter(item=>showDismissed||!item.dismissed) as item(item.id)}
  <article class="improvement">
    <div class="candidate-status">{item.dismissed?'Dismissed':item.evaluation?'Evaluation requested':'Untested suggestion'} · {item.evidenceCount} supporting run{item.evidenceCount==='1'?'':'s'}</div>
    <ImprovementText {project} candidate={item.id} reference={item.proposal}/>
    <details ontoggle={(event)=>{if(event.currentTarget.open)void moreEvidence(item);}}><summary>Inspect evidence</summary>
      {#each item.evidence as evidence(evidence.run)}<div class="evidence"><button class="flat" onclick={()=>void attempt(()=>openRun(evidence.run))}>Open run {evidence.run.slice(0,8)}</button><ImprovementText {project} candidate={item.id} run={evidence.run} reference={evidence.summary}/><small>Observation {evidence.summary.digest}</small></div>{/each}
      {#if item.evidenceNext!==null}<button class="flat" disabled={item.evidenceLoading} onclick={()=>void moreEvidence(item)}>Load more evidence</button>{/if}
      {#if item.evidenceLoading}<p class="setting-note">Loading evidence…</p>{/if}
    </details>
    <div class="dialog-actions">
      {#if item.evaluation}<button class="key" onclick={()=>void attempt(()=>openEvaluation(item.evaluation!))}>Open evaluation workbench / review patch</button>{/if}
      {#if !item.dismissed}<button class="key" disabled={busy||!target} onclick={()=>void attempt(()=>evaluate(item))}>{item.evaluation?'Resume evaluation request':'Generate patch & evaluate'}</button><button class="flat" disabled={busy} onclick={()=>void attempt(()=>mutate({action:'dismiss',candidate:item.id}))}>Dismiss</button>{/if}
    </div>
  </article>
{:else}<p class="small-empty">No suggestions in this view. Failed runs and repeated review/fix cycles supply investigation candidates; you can also collect a suggestion from a completed run above.</p>{/each}
{#if next!==null}<button class="flat" disabled={paging} onclick={()=>void moreCandidates()}>{paging?'Loading suggestions…':'Load more suggestions'}</button>{/if}

<style>
  form,.evaluation-target{display:grid;gap:12px;margin:18px 0;}label{display:grid;gap:7px;}select,textarea{width:100%;}textarea{resize:vertical}.dismissed{display:flex;flex-direction:row;justify-content:flex-start;align-items:center}.dismissed input{width:auto}.improvement{border-top:1px solid var(--line);padding:18px 0}.candidate-status{font-family:var(--font-mono);font-size:12px;color:var(--muted)}.evidence{padding:12px 0}.evidence pre{white-space:pre-wrap;overflow-wrap:anywhere;max-height:260px;overflow:auto}.evidence small{overflow-wrap:anywhere;color:var(--muted)}.loading{color:var(--muted)}
</style>
