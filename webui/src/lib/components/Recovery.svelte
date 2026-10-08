<script lang="ts">
  import {onDestroy} from 'svelte';
  import {recovery,forgetOperation} from '../operations.svelte';
  import {cancelOperation,request,retryOperation} from '../api';
  import {browserStorage} from '../storage.svelte';
  import {ui,reconcileOperations,notify} from '../workspace.svelte';

  let {configurationOnly=false,showAll=false}=$props<{configurationOnly?:boolean;showAll?:boolean}>();
  let checking=$state<Record<string,boolean>>({}),reviewing=$state<Record<string,boolean>>({}),retrying=$state<Record<string,boolean>>({}),cancelling=$state<Record<string,boolean>>({}),results=$state<Record<string,string>>({});
  const checks=new Map<string,AbortController>();
  const RECOVERY_PAGE_ROWS=8;
  let pageStart=$state(0),pageAnchor=$state('');
  let pageScope='';
  const isConfiguration=(command:string)=>command==='config'||command==='preferences';
  const missingEvidence=(command:string)=>command==='recovery-unknown'||command==='recovery-orphan';
  let unresolved=$derived(recovery.pending.filter(item=>{
    if(showAll)return true;
    if(configurationOnly)return isConfiguration(item.command);
    if(missingEvidence(item.command))return true;
    return !isConfiguration(item.command)&&(item.session===ui.sessionId||item.project===ui.projectId);
  }));
  let visible=$derived(unresolved.slice(pageStart,pageStart+RECOVERY_PAGE_ROWS));
  $effect(()=>{
    const scope=JSON.stringify([browserStorage.workspace,configurationOnly,showAll,ui.projectId,ui.sessionId]),items=unresolved,anchor=pageAnchor;
    if(scope!==pageScope){pageScope=scope;pageStart=0;pageAnchor='';return;}
    if(anchor){const at=items.findIndex(item=>item.operation===anchor);if(at>=0)pageStart=at;}
    if(pageStart>=items.length)pageStart=Math.max(0,items.length-RECOVERY_PAGE_ROWS);
  });
  function movePage(direction:number){
    pageStart=Math.max(0,Math.min(Math.max(0,unresolved.length-1),pageStart+direction*RECOVERY_PAGE_ROWS));
    pageAnchor=unresolved[pageStart]?.operation??'';
  }

  function aborted(error:unknown):boolean {
    return error instanceof Error&&error.name==='AbortError';
  }
  function cancelCheck(operation:string){
    const controller=checks.get(operation);if(!controller)return;
    controller.abort();results[operation]='Check cancelled. The original operation and its safety hold were not changed.';
  }
  async function check(operation:string){
    if(checks.has(operation))return;
    const controller=new AbortController();checks.set(operation,controller);checking[operation]=true;results[operation]='';
    try{
      const outcome=await reconcileOperations(operation,{signal:controller.signal});
      results[operation]=outcome.failed?'Could not check the original outcome. The safety hold remains; recheck when the gateway is available.':outcome.unresolved?'Still unresolved. Nothing was repeated and the safety hold remains. Inspect the original target before clearing it.':outcome.recovered?'The original outcome was recovered. Nothing was repeated.':'This operation no longer has an unresolved receipt.';
    }catch(error){
      if(!aborted(error))results[operation]='Could not check the original outcome. The safety hold remains; recheck when the gateway is available.';
    }finally{
      if(checks.get(operation)===controller){checks.delete(operation);checking[operation]=false;}
    }
  }
  async function retry(operation:string,command:string){
    if(retrying[operation])return;
    if(!confirm(`Retry the exact original ${command} operation? The gateway will use its retained identity and will reject the retry unless the recorded outcome permits it.`))return;
    retrying[operation]=true;results[operation]='';
    try{
      await retryOperation(operation);
      retrying[operation]=false;
      await check(operation);
    }
    catch(error){results[operation]=`The retry did not receive an acknowledgement: ${error instanceof Error?error.message:String(error)}. The retained identity remains available to check again.`;}
    finally{retrying[operation]=false;}
  }
  async function cancel(operation:string){
    if(cancelling[operation])return;
    if(!confirm('Stop the exact owned Git process tree? A partially completed Git mutation can still require repository and remote inspection.'))return;
    cancelling[operation]=true;results[operation]='';
    try{
      await cancelOperation(operation);
      results[operation]='Cancellation was recorded for the owned Git tree. Check the original outcome before repeating the command.';
    }catch(error){results[operation]=`Cancellation could not be confirmed: ${error instanceof Error?error.message:String(error)}. The retained safety hold remains.`;}
    finally{cancelling[operation]=false;}
  }
  async function reviewed(operation:string,configuration:boolean){
    if(reviewing[operation])return;
    const command=recovery.pending.find(item=>item.operation===operation)?.command??'';
    const target=missingEvidence(command)?'original target':configuration?'configuration file':'conversation or file/repository';
    if(!confirm(`Have you inspected the ${target} to determine what happened? This only clears the safety hold. It will NOT resend, undo, or declare the original action successful.`))return;
    reviewing[operation]=true;results[operation]='';
    try{
      await request('/api/operation-review',{operation,workspace:browserStorage.workspace,confirmed:true});
      forgetOperation(operation);
      if(configuration&&ui.settings.activeOperation===operation)ui.settings.activeOperation='';
      notify('Safety hold cleared after your review. The original action was not repeated.');
    }catch(error){results[operation]=`The review was not acknowledged: ${error instanceof Error?error.message:String(error)}. The safety hold remains.`;}
    finally{reviewing[operation]=false;}
  }
  onDestroy(()=>{for(const controller of checks.values())controller.abort();checks.clear();});
</script>

{#if browserStorage.error}<div class="recovery-banner" role="alert"><div><strong>Browser persistence needs attention</strong><p>{browserStorage.error}</p></div></div>{/if}
{#if unresolved.length>RECOVERY_PAGE_ROWS}<div class="activity-history" role="status">
  <button class="key small" disabled={!pageStart} onclick={()=>movePage(-1)}>Previous operations</button>
  <span>Unresolved operations {pageStart+1}–{pageStart+visible.length} of {unresolved.length.toLocaleString()}</span>
  <button class="key small" disabled={pageStart+visible.length>=unresolved.length} onclick={()=>movePage(1)}>Next operations</button>
</div>{/if}
{#each visible as item(item.operation)}
  <div class="recovery-banner" role="status">
    <div>
      <strong>{missingEvidence(item.command)?'Operation evidence needs review':`Unresolved ${item.command} operation`}</strong>
      <p>{item.command==='recovery-orphan'?'The original native request survived, but its parent operation record is missing. Inspect the target before reviewing this request; the gateway will retain its evidence without reconstructing or repeating the parent action.':missingEvidence(item.command)?'Only the original operation identity survived. Inspect the target before clearing this safety hold; the gateway cannot reconstruct or retry the lost input.':`The exact effect identity is retained. Check the original ${isConfiguration(item.command)?'configuration file':'target'} before choosing a retry or clearing its safety hold.`}</p>
      <code>{item.operation}</code>
      {#if checking[item.operation]||retrying[item.operation]||reviewing[item.operation]||cancelling[item.operation]||results[item.operation]}<p class="recovery-check" aria-live="polite">{checking[item.operation]?'Checking the original outcome…':retrying[item.operation]?'Submitting the exact retry…':reviewing[item.operation]?'Waiting for the review acknowledgement…':cancelling[item.operation]?'Stopping the owned Git tree…':results[item.operation]}</p>{/if}
    </div>
    <div class="button-cluster">
      {#if !missingEvidence(item.command)}
        <button class="key small" aria-busy={checking[item.operation]||undefined} onclick={()=>checking[item.operation]?cancelCheck(item.operation):void check(item.operation)}>{checking[item.operation]?'Cancel check':'Check original outcome'}</button>
        {#if item.command==='git'}<button class="key small" disabled={cancelling[item.operation]} aria-busy={cancelling[item.operation]||undefined} onclick={()=>void cancel(item.operation)}>{cancelling[item.operation]?'Stopping…':'Stop Git process'}</button>{/if}
        {#if item.command!=='git'}<button class="key small" disabled={retrying[item.operation]} aria-busy={retrying[item.operation]||undefined} onclick={()=>void retry(item.operation,item.command)}>{retrying[item.operation]?'Retrying…':'Retry original action'}</button>{/if}
      {/if}
      <button class="key small" disabled={reviewing[item.operation]} aria-busy={reviewing[item.operation]||undefined} onclick={()=>void reviewed(item.operation,isConfiguration(item.command))}>{reviewing[item.operation]?'Waiting…':'I inspected the outcome'}</button>
    </div>
  </div>
{/each}
