<script lang="ts">
  import {ui,attempt,dispatch} from '../workspace.svelte';
  import {query} from '../api';
  let catalogs=$state<Record<string,{configured:string;error:string;models:{id:string;label:string}[]}>>({});
  let loading=$state(false);
  async function load(role:string){const profile=ui.providers[role]??ui.facts?.providers[0]?.id;if(!profile)return;loading=true;try{catalogs[role]=await query('models',{profile});}finally{loading=false;}}
  $effect(()=>{for(const role of ['writer','reviewer','fixer'])if(!ui.models[role])ui.models[role]={id:'',manual:false,effort:'default'};});
</script>
<p class="dialog-description">Model choices are explicit for each role. An empty choice retains the configured provider model. Manual IDs are marked unverified.</p>
{#if !ui.facts?.providers.length}<div class="small-empty"><p>No configured providers were found.</p><button class="key primary" onclick={()=>void attempt(()=>dispatch('providers'))}>Open provider setup</button></div>{/if}
<div class="role-settings">{#each ['writer','reviewer','fixer'] as role}<fieldset><legend>{role==='writer'?'Conversation / writer':role==='reviewer'?'Independent reviewer':'Fixer'}</legend>
  <label>Provider<select value={ui.providers[role]??ui.facts?.providers[0]?.id??''} onchange={(event)=>{ui.providers[role]=event.currentTarget.value;void attempt(()=>load(role));}}><option value="">Configured default</option>{#each ui.facts?.providers??[] as provider}<option value={provider.id}>{provider.kind} · {provider.model}</option>{/each}</select></label>
  {#if ui.models[role]}<label>Model<div class="model-input"><input aria-label={`${role} model`} list={`models-${role}`} bind:value={ui.models[role]!.id} placeholder={catalogs[role]?.configured||'Keep configured model'}/><button class="key small" disabled={loading} onclick={()=>void attempt(()=>load(role))}>Discover</button></div><datalist id={`models-${role}`}>{#each catalogs[role]?.models??[] as model}<option value={model.id}>{model.label}</option>{/each}</datalist></label>
  <label class="checkbox-row"><input type="checkbox" bind:checked={ui.models[role]!.manual}/>Use an unverified manual model ID</label>
  <label>Reasoning effort<select bind:value={ui.models[role]!.effort}>{#each ['default','minimal','low','medium','high','xhigh','max','ultra'] as effort}<option value={effort}>{effort}</option>{/each}</select></label>{/if}
  {#if catalogs[role]?.error}<p class="inline-error">{catalogs[role]!.error}</p>{/if}
</fieldset>{/each}</div>
<p class="setting-note">These choices accompany your next message. No provider requests are made by merely opening these controls.</p>
