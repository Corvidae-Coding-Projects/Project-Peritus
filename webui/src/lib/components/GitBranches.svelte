<script lang="ts">
  import {ui, attempt, gitAction} from '../workspace.svelte';
  let selected=$state(''), name=$state(''), start=$state(''), rename=$state('');
  let branch=$derived(ui.git?.branches.find(item=>item.ref===selected));
  const run=(kind:string,options:Record<string,unknown>)=>gitAction(kind,[],'',options);
  async function create(){await run('branch-create',{branch:name.trim(),start});name='';selected=`refs/heads/${ui.git?.branch}`;}
  async function remove(){if(branch&&confirm(`Delete local branch ${branch.name}? Git will refuse if it contains unmerged work.`)){await run('branch-delete',{branch:branch.name,confirmed:true});selected='';}}
</script>
<details class="git-workflow">
  <summary>Branches <span>{ui.git?.branch||'Detached HEAD'}</span></summary>
  <div class="git-workflow-body">
    <label for="git-branch">Switch branch</label>
    <select id="git-branch" bind:value={selected} disabled={ui.gitBusy}>
      <option value="">Choose a branch…</option>
      {#each ui.git?.branches??[] as item(item.ref)}<option value={item.ref}>{item.name}{item.current?' (current)':item.remote?' (remote)':''}</option>{/each}
    </select>
    {#if branch?.upstream}<p>Tracks {branch.upstream}</p>{/if}
    <button class="key small" disabled={ui.gitBusy||!branch||branch.current} onclick={()=>void attempt(()=>run('branch-switch',{branch:selected}))}>{branch?.remote?'Track remote branch':'Switch branch'}</button>
    <form onsubmit={(event)=>{event.preventDefault();void attempt(create);}}>
      <label for="new-branch">New branch</label><input id="new-branch" bind:value={name} required placeholder="feature/my-change" autocomplete="off"/>
      <label for="branch-start">Start from</label><select id="branch-start" bind:value={start}><option value="">Current HEAD</option>{#each ui.git?.branches??[] as item(item.ref)}<option value={item.ref}>{item.name}</option>{/each}</select>
      <button class="key small" disabled={ui.gitBusy||!name.trim()}>Create & switch</button>
    </form>
    {#if branch&&!branch.remote}
      <form onsubmit={(event)=>{event.preventDefault();void attempt(async()=>{await run('branch-rename',{branch:branch!.name,name:rename.trim()});selected=`refs/heads/${rename.trim()}`;rename='';});}}>
        <label for="rename-branch">Rename {branch.name}</label><input id="rename-branch" bind:value={rename} required placeholder="New branch name"/>
        <button class="key small" disabled={ui.gitBusy||!rename.trim()}>Rename branch</button>
      </form>
      <button class="flat" disabled={ui.gitBusy||branch.current} onclick={()=>void attempt(remove)}>Delete merged branch…</button>
    {/if}
  </div>
</details>
