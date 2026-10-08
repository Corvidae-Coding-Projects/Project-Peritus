<script lang="ts">
  import { onMount } from 'svelte';
  import { action,reviewTerminalInput,terminalInput,terminalInputAttempt,terminalInterrupt,terminalRead,terminalResize } from '../api';
  import type { RecoverableTerminalInput,TerminalInputAttempt } from '../api';
  import { ui,attempt,notify } from '../workspace.svelte';
  import Icon from './Icon.svelte';
  import '@xterm/xterm/css/xterm.css';
  let { id,workspace,suggestion='' }:{id:string;workspace:string;suggestion?:string}=$props();
  let element:HTMLDivElement;let text=$state(''),ended=$state(false),error=$state(''),sending=$state(false);
  let retainedInput:TerminalInputAttempt|undefined=$state();
  let inspectRecovery=$state(true);
  let sequence=Promise.resolve();
  function input(value:string,form=false){
    if(value==='\x03'){void terminalInterrupt(workspace,id).catch(e=>{error=`Interrupt was not confirmed: ${e.message}. Inspect the console before repeating it.`;});return;}
    const attempt=form&&retainedInput?.body===value?retainedInput:terminalInputAttempt(workspace,value,form);
    if(form){retainedInput=attempt;sending=true;}
    sequence=sequence.then(()=>terminalInput(id,attempt)).then(()=>{
      if(form&&retainedInput===attempt){retainedInput=undefined;sending=false;text='';error='';inspectRecovery=true;}
    }).catch(e=>{
      if(form&&retainedInput===attempt){sending=false;inspectRecovery=true;}
      error=`Input was not confirmed: ${e.message}. The exact original input is retained; inspect or retry that same input.`;
    });
  }
  async function reviewUnknown(){
    const original=retainedInput;if(!original||original.state!=='unknown'||sending)return;
    sending=true;
    try{await reviewTerminalInput(id,original);if(retainedInput===original){retainedInput=undefined;text='';error='';inspectRecovery=true;}}
    catch(e){error=e instanceof Error?e.message:String(e);}
    finally{sending=false;}
  }
  onMount(()=>{
    text=suggestion;
    const observations=new AbortController();
    let disposed=false,observer:ResizeObserver|undefined,timer:ReturnType<typeof setTimeout>|undefined,term:import('@xterm/xterm').Terminal|undefined;
    void(async()=>{
      const [{Terminal},{FitAddon}]=await Promise.all([import('@xterm/xterm'),import('@xterm/addon-fit')]);if(disposed)return;
      term=new Terminal({fontFamily:'ui-monospace, monospace',fontSize:13,scrollback:4_294_967_295,screenReaderMode:true,theme:{background:'#121718',foreground:'#dfdfd4',cursor:'#f2a260',selectionBackground:'#62452e'}});
      const fit=new FitAddon();term.loadAddon(fit);term.open(element);term.onData(input);
      observer=new ResizeObserver(()=>{if(disposed)return;fit.fit();void terminalResize(workspace,id,term!.cols,term!.rows,element.clientWidth,element.clientHeight).catch(e=>error=e.message);});observer.observe(element);
      let after='0';
      async function read(){
        try{const value=await terminalRead<{data:string;next:string;lost:boolean;ended:boolean;available:boolean;error?:string;inputRecoveryError?:string;recoverableInput?:RecoverableTerminalInput}>(workspace,id,after,inspectRecovery,{signal:observations.signal});if(disposed)return;
          inspectRecovery=false;
          if(value.lost)term!.writeln('\r\n[Earlier console output is no longer retained]\r\n');
          term!.write(Uint8Array.from(atob(value.data),c=>c.charCodeAt(0)));after=value.next;ended=value.ended;
          if(value.recoverableInput&&!sending&&(!retainedInput||retainedInput.input===value.recoverableInput.input)){
            retainedInput={workspace,form:true,...value.recoverableInput};
            text=value.recoverableInput.body.endsWith('\r')?value.recoverableInput.body.slice(0,-1):value.recoverableInput.body;
            if(value.recoverableInput.state==='unknown')error='This exact retained input may have reached the PTY and will not be sent again.';
          }
          if(value.error||!value.available)error=value.error??'The durable console process is unavailable for interactive I/O.';
          if(value.inputRecoveryError)error=`Retained input inspection failed: ${value.inputRecoveryError}. Console output remains available.`;
        }catch(e){if(!disposed)error=e instanceof Error?e.message:String(e);}
        if(!disposed)timer=setTimeout(()=>void read(),ended?1800:250);
      }void read();
    })().catch(e=>error=e.message);
    return()=>{disposed=true;observations.abort();clearTimeout(timer);observer?.disconnect();term?.dispose();};
  });
</script>
<div class="console-view">
  <p class="dialog-description">The installed Peritus CLI. Session consoles open the exact browser conversation; setup and scriptable consoles show their own selection below.</p>
  {#if suggestion}<p class="console-tip">Once setup is complete and the composer is ready, submit <code>{suggestion}</code> below. Chat and run controls target the selected browser conversation. Queue, context, goals, and other Workbench controls use the CLI’s /sessions selection; select or create that governed session first.</p>{/if}
  <div bind:this={element} class="terminal-surface" role="region" aria-label="Interactive Peritus terminal"></div>
  {#if error}<p class="inline-error">{error}</p>{/if}
  <div class="console-keypad" aria-label="Mouse-accessible terminal keys">
    {#each [['Esc','\x1b'],['↑','\x1b[A'],['↓','\x1b[B'],['←','\x1b[D'],['→','\x1b[C'],['Tab','\t'],['Space',' '],['Enter','\r'],['Ctrl+C','\x03']] as key}<button class="key small" disabled={ended} aria-label={`Send ${key[0]} key`} onclick={()=>input(key[1]!)}>{key[0]}</button>{/each}
  </div>
  <form class="console-input" onsubmit={(event)=>{event.preventDefault();input(text+'\r',true);}}><input aria-label="Text or slash command to send to CLI" bind:value={text} placeholder="Type text or a CLI slash command…" disabled={ended||sending||retainedInput!==undefined}/><button class="key primary" disabled={ended||sending||retainedInput?.state==='unknown'}>{retainedInput?.state==='unknown'?'Outcome unknown':retainedInput?'Retry exact input':'Send to CLI'}<Icon name="arrow" size={16}/></button></form>
  {#if retainedInput?.state==='unknown'}<button class="key small" disabled={sending} onclick={()=>void reviewUnknown()}>Keep unknown receipt and enter new input</button>{/if}
  <div class="console-foot"><span>{ended?'Console process exited; its complete output is retained.':'Closing this window keeps the independently owned console running.'}</span><button class="flat" onclick={()=>void attempt(async()=>{const disposition=ended?'dismiss':'terminate';await action('close-console',{id,disposition},{workspace});ui.consoles=ui.consoles.filter(c=>c.id!==id);ui.consoleId=ui.consoles[0]?.id??'';if(!ui.consoleId)ui.overlay='';notify(ended?'Console record dismissed.':'Console termination confirmed. Daemon-owned work is retained.');})}>{ended?'Dismiss console':'Terminate console'}</button></div>
</div>
