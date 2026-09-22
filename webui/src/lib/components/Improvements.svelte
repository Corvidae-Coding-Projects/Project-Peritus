<script lang="ts">
  import { onMount } from 'svelte';
  import { query, action } from '../api';
  import { ui, attempt, openRun, notify } from '../workspace.svelte';
  import type { ImprovementInbox, ImprovementCandidate, Run } from '../types';
  let inbox = $state<ImprovementInbox>({workspace:'',candidates:[]});
  let runs = $state<Run[]>([]), proposal = $state(''), source = $state('');
  let target = $state(''), busy = $state(false), error = $state(''), showDismissed = $state(false);
  const project = ui.projectId;
  async function refresh() {
    error='';
    try { inbox=await query<ImprovementInbox>('improvements',{project}); runs=await query<Run[]>('runs'); }
    catch(e) { error=e instanceof Error?e.message:String(e); }
  }
  onMount(()=>{void refresh();});
  async function mutate(input:Record<string,unknown>) {
    busy=true;
    try { inbox=await action<ImprovementInbox>('improvements',{project,...input}); }
    finally {busy=false;}
  }
  async function evaluate(item:ImprovementCandidate) {
    await mutate({action:'evaluate',candidate:item.id,target});
    const run=inbox.candidates.find(c=>c.id===item.id)?.evaluation;
    if(run) await openRun(run);
  }
</script>

<p class="dialog-description">Collect evidence-backed suggestions here. Generate a patch and run tests only when you choose to evaluate one.</p>
{#if error}<p role="alert">{error}</p><button class="key" onclick={()=>void refresh()}>Retry loading inbox</button>{/if}
<form onsubmit={(event)=>{event.preventDefault();void attempt(async()=>{await mutate({action:'suggest',run:source,proposal});proposal='';notify('Suggestion saved. No evaluation started.');});}}>
  <label for="improvement-source">Evidence run</label><select id="improvement-source" bind:value={source} required><option value="">Select a completed run</option>{#each runs.filter(r=>r.workspace===inbox.workspace&&!r.busy) as run}<option value={run.id}>{run.task.slice(0,90)} · {run.phase}</option>{/each}</select>
  <label>Improvement suggestion<textarea bind:value={proposal} required maxlength="4096" rows="3" placeholder="What might improve the harness, and why does this run support investigating it?"></textarea></label>
  <button class="key" disabled={busy||!source||!proposal.trim()}>Collect suggestion</button>
</form>
{#if inbox.candidates.filter(c=>!c.dismissed).length>=32}<p role="status">The inbox is full. Dismiss a suggestion to make room; additional run evidence remains in run history.</p>{/if}
<div class="evaluation-target"><label for="improvement-target">Peritus source workspace</label><select id="improvement-target" bind:value={target}><option value="">Choose where to generate and test patches</option>{#each ui.workspace.projects.filter(p=>!p.closed) as project}<option value={project.id}>{project.name} · {project.root}</option>{/each}</select><p class="setting-note">Select a registered Git checkout of Peritus. Evaluation uses its configured provider and normal run limits. You can stop the run, inspect its patch and checks, then explicitly export or commit it. The running harness is never updated automatically.</p></div>
<label class="dismissed"><input type="checkbox" bind:checked={showDismissed}/> Show dismissed suggestions</label>
{#each inbox.candidates.filter(c=>showDismissed||!c.dismissed) as item(item.id)}
  <article class="improvement">
    <div class="candidate-status">{item.dismissed?'Dismissed':item.evaluation?'Evaluation requested':'Untested suggestion'} · {item.evidence.length} supporting run{item.evidence.length===1?'':'s'}</div>
    <p>{item.proposal}</p>
    <details><summary>Inspect evidence</summary>{#each item.evidence as evidence}<div class="evidence"><button class="flat" onclick={()=>void attempt(()=>openRun(evidence.run))}>Open run {evidence.run.slice(0,8)}</button><pre>{evidence.summary}</pre><small>Observation {evidence.digest}</small></div>{/each}</details>
    <div class="dialog-actions">
      {#if item.evaluation}<button class="key" onclick={()=>void attempt(()=>openRun(item.evaluation!))}>Open evaluation / review patch</button>{/if}
      {#if !item.dismissed}<button class="key" disabled={busy||!target} onclick={()=>void attempt(()=>evaluate(item))}>{item.evaluation?'Resume evaluation request':'Generate patch & evaluate'}</button><button class="flat" disabled={busy} onclick={()=>void attempt(()=>mutate({action:'dismiss',candidate:item.id}))}>Dismiss</button>{/if}
    </div>
  </article>
{:else}<p class="small-empty">No suggestions in this view. Failed runs and repeated review/fix cycles supply investigation candidates; you can also collect a suggestion from a completed run above.</p>{/each}

<style>
  form,.evaluation-target{display:grid;gap:12px;margin:18px 0;}label{display:grid;gap:7px;}select,textarea{width:100%;}textarea{resize:vertical}.dismissed{display:flex;flex-direction:row;justify-content:flex-start;align-items:center}.dismissed input{width:auto}.improvement{border-top:1px solid var(--line);padding:18px 0}.candidate-status{font-family:var(--font-mono);font-size:12px;color:var(--muted)}.evidence{padding:12px 0}.evidence pre{white-space:pre-wrap;overflow-wrap:anywhere;max-height:260px;overflow:auto}.evidence small{overflow-wrap:anywhere;color:var(--muted)}
</style>
