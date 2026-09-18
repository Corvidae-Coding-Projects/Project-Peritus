<script lang="ts">
  import {ui,attempt,gitAction} from '../workspace.svelte';
  let selected=$state(''),name=$state(''),url=$state(''),rename=$state(''),pushUrl=$state(false);
  let remote=$derived(ui.git?.remoteDetails.find(item=>item.name===selected));
  const run=(kind:string,options:Record<string,unknown>)=>gitAction(kind,[],'',options);
  async function save(){await run(remote?'remote-url':'remote-add',{remote:remote?.name??name.trim(),url:url.trim(),pushUrl});if(!remote)selected=name.trim();name='';url='';}
  async function remove(){if(remote&&confirm(`Remove remote ${remote.name} from this repository? The remote server will not be changed.`)){await run('remote-remove',{remote:remote.name,confirmed:true});selected='';}}
</script>
<details class="git-workflow">
  <summary>Remotes <span>{ui.git?.remoteDetails.length??0} configured</span></summary>
  <div class="git-workflow-body">
    <label for="git-remote">Remote</label><select id="git-remote" bind:value={selected} onchange={()=>{url='';rename='';pushUrl=false;}}><option value="">Add a remote…</option>{#each ui.git?.remoteDetails??[] as item(item.name)}<option value={item.name}>{item.name}</option>{/each}</select>
    {#if remote}
      <div class="remote-address"><span>Fetch</span><code>{remote.fetch.join('\n')}</code><span>Push</span><code>{remote.push.join('\n')}</code></div>
      <button class="key small" disabled={ui.gitBusy} onclick={()=>void attempt(()=>run('fetch',{remote:selected}))}>Fetch & prune {selected}</button>
      <button class="key small" disabled={ui.gitBusy||!ui.git?.branch} onclick={()=>void attempt(()=>run('push',{remote:selected,branch:ui.git?.branch,setUpstream:true}))}>Publish current branch</button>
      <button class="key small" disabled={ui.gitBusy||!ui.git?.branch} onclick={()=>void attempt(()=>run('pull',{remote:selected,branch:ui.git?.branch}))}>Pull current branch</button>
    {/if}
    <form onsubmit={(event)=>{event.preventDefault();void attempt(save);}}>
      {#if !remote}<label for="remote-name">Remote name</label><input id="remote-name" bind:value={name} required placeholder="origin"/>{/if}
      <label for="remote-url">{remote?'New URL':'Repository URL'}</label><input id="remote-url" bind:value={url} required placeholder="git@host:team/project.git" autocomplete="off"/>
      {#if remote}<label class="inline-checkbox"><input type="checkbox" bind:checked={pushUrl}/>Change push URL only</label>{/if}
      <button class="key small" disabled={ui.gitBusy||!url.trim()||(!remote&&!name.trim())}>{remote?'Save URL':'Add remote'}</button>
    </form>
    {#if remote}
      <form onsubmit={(event)=>{event.preventDefault();void attempt(async()=>{await run('remote-rename',{remote:selected,name:rename.trim()});selected=rename.trim();rename='';});}}><label for="rename-remote">Rename remote</label><input id="rename-remote" bind:value={rename} required/><button class="key small" disabled={ui.gitBusy||!rename.trim()}>Rename remote</button></form>
      <button class="flat" disabled={ui.gitBusy} onclick={()=>void attempt(remove)}>Remove remote…</button>
    {/if}
  </div>
</details>
