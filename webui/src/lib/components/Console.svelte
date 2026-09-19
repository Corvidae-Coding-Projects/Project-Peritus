<script lang="ts">
  import { onMount } from 'svelte';
  import { request,action } from '../api';
  import { ui,attempt,notify } from '../workspace.svelte';
  import Icon from './Icon.svelte';
  import '@xterm/xterm/css/xterm.css';
  let { id,suggestion='' }:{id:string;suggestion?:string}=$props();
  let element:HTMLDivElement;let text=$state(''),ended=$state(false),error=$state('');
  let sequence=Promise.resolve();
  function input(value:string){sequence=sequence.then(()=>request(`/api/terminal/${id}`,{text:value})).then(()=>{}).catch(e=>{error=`Input was not confirmed: ${e.message}. Inspect the console before repeating it.`;});}
  onMount(()=>{
    text=suggestion;
    let disposed=false,observer:ResizeObserver|undefined,timer:ReturnType<typeof setTimeout>|undefined,term:import('@xterm/xterm').Terminal|undefined;
    void(async()=>{
      const [{Terminal},{FitAddon}]=await Promise.all([import('@xterm/xterm'),import('@xterm/addon-fit')]);if(disposed)return;
      term=new Terminal({fontFamily:'ui-monospace, monospace',fontSize:13,scrollback:4000,screenReaderMode:true,theme:{background:'#121718',foreground:'#dfdfd4',cursor:'#f2a260',selectionBackground:'#62452e'}});
      const fit=new FitAddon();term.loadAddon(fit);term.open(element);term.onData(input);
      observer=new ResizeObserver(()=>{if(disposed)return;fit.fit();void request(`/api/terminal/${id}`,{cols:term!.cols,rows:term!.rows}).catch(e=>error=e.message);});observer.observe(element);
      let after=0;
      async function read(){
        try{const value=await request<{data:string;next:number;lost:boolean;ended:boolean}>(`/api/terminal/${id}?after=${after}`);if(disposed)return;
          if(value.lost)term!.writeln('\r\n[Earlier console output is no longer retained]\r\n');
          term!.write(Uint8Array.from(atob(value.data),c=>c.charCodeAt(0)));after=value.next;ended=value.ended;
        }catch(e){if(!disposed)error=e instanceof Error?e.message:String(e);}
        if(!disposed)timer=setTimeout(()=>void read(),ended?1800:250);
      }void read();
    })().catch(e=>error=e.message);
    return()=>{disposed=true;clearTimeout(timer);observer?.disconnect();term?.dispose();};
  });
</script>
<div class="console-view">
  <p class="dialog-description">The installed Peritus CLI, running in this project. Its current selection is shown in the console. Setup, approvals, and advanced workbench flows retain their original behavior.</p>
  {#if suggestion}<p class="console-tip">Once setup is complete and the composer is ready, submit <code>{suggestion}</code> below. Choose the intended CLI conversation first.</p>{/if}
  <div bind:this={element} class="terminal-surface" role="region" aria-label="Interactive Peritus terminal"></div>
  {#if error}<p class="inline-error">{error}</p>{/if}
  <div class="console-keypad" aria-label="Mouse-accessible terminal keys">
    {#each [['Esc','\x1b'],['↑','\x1b[A'],['↓','\x1b[B'],['←','\x1b[D'],['→','\x1b[C'],['Tab','\t'],['Space',' '],['Enter','\r'],['Ctrl+C','\x03']] as key}<button class="key small" disabled={ended} aria-label={`Send ${key[0]} key`} onclick={()=>input(key[1]!)}>{key[0]}</button>{/each}
  </div>
  <form class="console-input" onsubmit={(event)=>{event.preventDefault();input(text+'\r');text='';}}><input aria-label="Text or slash command to send to CLI" bind:value={text} placeholder="Type text or a CLI slash command…" disabled={ended}/><button class="key primary" disabled={ended}>Send to CLI<Icon name="arrow" size={16}/></button></form>
  <div class="console-foot"><span>{ended?'Console process exited.':'Closing this window keeps the console running.'}</span><button class="flat" onclick={()=>void attempt(async()=>{await action('close-console',{id});ui.consoles=ui.consoles.filter(c=>c.id!==id);ui.consoleId=ui.consoles[0]?.id??'';if(!ui.consoleId)ui.overlay='';notify('Console process closed. Daemon-owned work is retained.');})}>Terminate console</button></div>
</div>
