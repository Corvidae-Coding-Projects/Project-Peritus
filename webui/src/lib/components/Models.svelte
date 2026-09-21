<script lang="ts">
  import {ui,attempt,dispatch,session,refresh,notify,observeConversation} from '../workspace.svelte';
  import {query,action} from '../api';
  import type {ModelChoice,Session,Conversation} from '../types';
  const id=ui.sessionId;
  const selected=session();
  let models=$state<Record<string,ModelChoice>>(structuredClone($state.snapshot(ui.conversations[id]?.models??selected?.settings?.models??{})));
  let providers=$state<Record<string,string>>(structuredClone($state.snapshot(ui.conversations[id]?.run?.providers??selected?.settings?.providers??{})));
  for(const role of ['writer','reviewer','fixer'])models[role]??={id:'',manual:false,effort:'default'};
  let catalogs=$state<Record<string,{configured:string;error:string;models:{id:string;label:string}[]}>>({});
  let loading=$state(false),saving=$state(false);
  async function load(role:string){const profile=providers[role]||ui.facts?.providers[0]?.id;if(!profile)return;loading=true;try{catalogs[role]=await query('models',{profile});}finally{loading=false;}}
  async function save(){
    saving=true;
    try {
      const value=await action<{session:Session;conversation:Conversation|null}>('session-settings',{session:id,settings:{models,providers},existing:!!ui.conversations[id]?.run});
      if(value.conversation)observeConversation(id,value.conversation);
      await refresh();ui.overlay='';notify('Model choices saved for this conversation.');
    }finally{saving=false;}
  }
</script>
<p class="dialog-description">Save model and effort choices for this conversation. Existing conversations also update the daemon, so the CLI sees the same selection.</p>
{#if !ui.facts?.providers.length}<div class="small-empty"><p>No configured providers were found.</p><button class="key primary" onclick={()=>void attempt(()=>dispatch('providers'))}>Open provider setup</button></div>{/if}
<div class="role-settings">{#each ['writer','reviewer','fixer'] as role}<fieldset disabled={saving}><legend>{role==='writer'?'Conversation / writer':role==='reviewer'?'Independent reviewer':'Fixer'}</legend>
  <label>Provider<select disabled={!!ui.conversations[id]?.run} value={providers[role]??''} onchange={(event)=>{providers[role]=event.currentTarget.value;models[role]={id:'',manual:false,effort:'default'};void attempt(()=>load(role));}}><option value="">Configured default</option>{#each ui.facts?.providers??[] as provider}<option value={provider.id}>{provider.kind} · {provider.model}</option>{/each}</select></label>
  <label>Model<div class="model-input"><input aria-label={`${role} model`} list={`models-${role}`} bind:value={models[role]!.id} placeholder={catalogs[role]?.configured||'Keep configured model'}/><button class="key small" disabled={loading} onclick={()=>void attempt(()=>load(role))}>Discover</button></div><datalist id={`models-${role}`}>{#each catalogs[role]?.models??[] as model}<option value={model.id}>{model.label}</option>{/each}</datalist></label>
  <label class="checkbox-row"><input type="checkbox" bind:checked={models[role]!.manual}/>Use an unverified manual model ID</label>
  <label>Reasoning effort<select bind:value={models[role]!.effort}>{#each ['default','minimal','low','medium','high','xhigh','max','ultra'] as effort}<option value={effort}>{effort}</option>{/each}</select></label>
  {#if catalogs[role]?.error}<p class="inline-error">{catalogs[role]!.error}</p>{/if}
</fieldset>{/each}</div>
<p class="setting-note">Choose providers before the first message; existing conversations retain their original provider routes. Discover queries the selected provider; opening this panel does not.</p>
<div class="dialog-actions"><button class="key primary" disabled={saving} onclick={()=>void attempt(save)}>{saving?'Saving…':'Save choices'}</button></div>
