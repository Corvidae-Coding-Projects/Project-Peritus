<script lang="ts">
  import {recovery,forgetOperation} from '../operations.svelte';
  import {request} from '../api';
  import {ui,attempt,reconcileOperations,notify} from '../workspace.svelte';
  let checking=$state<Record<string,boolean>>({}),results=$state<Record<string,string>>({});
  let unresolved=$derived(recovery.pending.filter(item=>(item.session===ui.sessionId||item.project===ui.projectId)&&!ui.pending[item.session??'']&&!ui.gitBusy&&!ui.attaching[item.session??'']));
  async function check(operation:string){
    if(checking[operation])return;
    checking[operation]=true;results[operation]='';
    try{
      const outcome=await reconcileOperations(operation);
      results[operation]=outcome.failed?'Could not check the original outcome. The safety hold remains; reconnect and try again.':outcome.unresolved?'Still unresolved. Nothing was repeated and the safety hold remains. Inspect the original target before clearing it.':'The original outcome was recovered. Nothing was repeated.';
    }catch{results[operation]='Could not check the original outcome. The safety hold remains; reconnect and try again.';}
    finally{checking[operation]=false;}
  }
  async function reviewed(operation:string){
    if(!confirm('Have you inspected the conversation or file/repository to determine what happened? This only clears the safety hold. It will NOT resend, undo, or declare the original action successful.'))return;
    await request('/api/operation-review',{operation,confirmed:true});forgetOperation(operation);notify('Safety hold cleared after your review. The original action was not repeated.');
  }
</script>
{#each unresolved as item(item.operation)}<div class="recovery-banner" role="status"><div><strong>Unresolved {item.command} operation</strong><p>Your draft is retained. Check the original target before submitting again.</p><code>{item.operation}</code>{#if checking[item.operation]||results[item.operation]}<p class="recovery-check" aria-live="polite">{checking[item.operation]?'Checking the original outcome…':results[item.operation]}</p>{/if}</div><div class="button-cluster"><button class="key small" disabled={checking[item.operation]} aria-busy={checking[item.operation]||undefined} onclick={()=>void check(item.operation)}>{checking[item.operation]?'Checking…':'Check original outcome'}</button><button class="key small" disabled={checking[item.operation]} onclick={()=>void attempt(()=>reviewed(item.operation))}>I inspected the outcome</button></div></div>{/each}
