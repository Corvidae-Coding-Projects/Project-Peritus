<script lang="ts">
  import { ui,project,attempt,gitAction,loadGit,refresh,notify } from '../workspace.svelte';
  import { action } from '../api';
  import Icon from './Icon.svelte';
  import GitBranches from './GitBranches.svelte';
  import GitRemotes from './GitRemotes.svelte';
  import './git.css';
  let message=$state(''),settings=$state(false),repository=$state('');
  let staged=$derived(ui.git?.changes.filter(c=>![' ','?'].includes(c.code[0]!))??[]);
  let unstaged=$derived(ui.git?.changes.filter(c=>c.code[1]!==' ')??[]);
  async function commit(){await gitAction('commit',[],message);message='';}
  async function saveRepository(){await action('repository',{project:ui.projectId,path:repository});await refresh();await loadGit();settings=false;notify('Repository location saved.');}
</script>
<section class="git-panel" aria-label="Source control">
  <div class="drawer-heading"><h2>Source control</h2><div class="button-cluster"><button class="flat icon-button" aria-label="Configure Git repository" onclick={()=>{repository=project()?.repository??'';settings=!settings;}}><Icon name="settings" size={16}/></button><button class="flat icon-button" aria-label="Refresh Git status" onclick={()=>void loadGit()}><Icon name="refresh" size={16}/></button></div></div>
  <div class="git-location"><Icon name="git"/><strong>{ui.git?.branch||'No branch'}</strong><button class="flat" onclick={()=>{repository=project()?.repository??'';settings=!settings;}}>Repository</button></div>
  {#if settings||ui.gitError}
    <form class="repository-settings" onsubmit={(event)=>{event.preventDefault();void attempt(saveRepository);}}>
      <label for="repository-path">Repository directory</label><input id="repository-path" bind:value={repository} placeholder={project()?.repository}/>
      <p>Any directory inside this project. You can save a location before a Git repository exists there.</p>
      <button class="key small" type="submit">Save location</button>
    </form>
  {/if}
  {#if ui.gitError}<p class="inline-error">{ui.gitError}</p>{/if}
  {#if ui.git}
    <div class="git-transfer"><button class="key" disabled={ui.gitBusy} onclick={()=>void attempt(()=>gitAction('pull'))}><Icon name="download" size={16}/>Pull</button><button class="key" disabled={ui.gitBusy} onclick={()=>void attempt(()=>gitAction('push'))}><Icon name="upload" size={16}/>Push</button></div>
    <div class="git-scroll">
      {#key ui.projectId}<GitBranches/><GitRemotes/>{/key}
      <div class="list-heading"><h3>Staged changes <span>{staged.length}</span></h3><button class="flat" disabled={!staged.length||ui.gitBusy} onclick={()=>void attempt(()=>gitAction('unstage'))}>Unstage all</button></div>
      {#each staged as change(change.path)}<div class="change-row"><button class="flat change-path" title={change.path} onclick={()=>void attempt(()=>gitAction('diff-staged',[change.path]))}><span class="change-code">{change.code[0]}</span>{change.path}</button><button class="flat icon-button" aria-label={`Unstage ${change.path}`} disabled={ui.gitBusy} onclick={()=>void attempt(()=>gitAction('unstage',[change.path]))}><Icon name="close" size={14}/></button></div>{/each}
      {#if !staged.length}<p class="git-hint">Stage a file to include it in your next commit.</p>{/if}
      <div class="list-heading"><h3>Changes <span>{unstaged.length}</span></h3><button class="flat" disabled={!unstaged.length||ui.gitBusy} onclick={()=>void attempt(()=>gitAction('add'))}>Stage all</button></div>
      {#each unstaged as change(change.path)}<div class="change-row"><button class="flat change-path" title={change.path} onclick={()=>void attempt(()=>gitAction('diff',[change.path]))}><span class="change-code">{change.code==='??'?'U':change.code[1]}</span>{change.path}</button><button class="flat icon-button" aria-label={`Stage ${change.path}`} disabled={ui.gitBusy} onclick={()=>void attempt(()=>gitAction('add',[change.path]))}><Icon name="plus" size={14}/></button></div>{/each}
      {#if !ui.git.changes.length}<div class="clean-state"><Icon name="check" size={28}/><strong>Working tree clean</strong><p>No uncommitted changes.</p></div>{/if}
    </div>
    <form class="commit-form" onsubmit={(event)=>{event.preventDefault();void attempt(commit);}}><label for="commit-message">Commit message</label><textarea id="commit-message" bind:value={message} rows="3" placeholder="Describe the staged changes…"></textarea><button class="key primary" disabled={ui.gitBusy||!staged.length||!message.trim()}><Icon name="check" size={16}/>{ui.gitBusy?'Git operation in progress…':`Commit ${staged.length} staged`}</button></form>
    <details class="remote-details"><summary>Remotes & repository path</summary><pre>{ui.git.remotes||'No remotes configured.'}</pre><code>{ui.git.root}</code></details>
  {/if}
</section>
