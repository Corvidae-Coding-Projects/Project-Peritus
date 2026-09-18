<script lang="ts">
  import { tick } from 'svelte';
  import {commands} from '../commands/catalog';
  import {ui,attempt,dispatch} from '../workspace.svelte';
  import Icon from './Icon.svelte';
  let dialog:HTMLDialogElement,input:HTMLInputElement,search=$state(''),selected=$state(0);
  let results=$derived(commands.filter(c=>`${c.label} ${c.detail} ${c.slash}`.toLowerCase().includes(search.toLowerCase().replace(/^\//,''))));
  $effect(()=>{if(ui.palette){search=ui.palette.trim();selected=0;dialog.showModal();void tick().then(()=>input.focus());}else dialog?.close();});
  $effect(()=>{
    const active=results[selected]?.id;
    if(ui.palette&&active)void tick().then(()=>document.getElementById(`command-option-${active}`)?.scrollIntoView({block:'nearest'}));
  });
  function choose(id:string){ui.palette='';void attempt(()=>dispatch(id));}
</script>
<dialog bind:this={dialog} class="command-palette" aria-label="Command directory" onclose={()=>ui.palette=''} onclick={(event)=>{if(event.target===dialog)ui.palette='';}}>
  <div class="palette-search"><Icon name="search" size={22}/><input bind:this={input} role="combobox" aria-label="Search commands" aria-autocomplete="list" aria-controls="command-results" aria-expanded={!!ui.palette} aria-activedescendant={results[selected]?`command-option-${results[selected]!.id}`:undefined} bind:value={search} placeholder="What would you like to do?" oninput={()=>selected=0} onkeydown={(event)=>{if(event.key==='ArrowDown'){event.preventDefault();selected=Math.max(0,Math.min(selected+1,results.length-1));}if(event.key==='ArrowUp'){event.preventDefault();selected=Math.max(0,selected-1);}if(event.key==='Enter'&&results[selected]){event.preventDefault();choose(results[selected]!.id);}}}/><button class="flat" onclick={()=>ui.palette=''}><kbd>Esc</kbd></button></div>
  <div class="palette-results" id="command-results" role="listbox" aria-label="Available commands">{#each results as command,index(command.id)}<button id={`command-option-${command.id}`} role="option" aria-selected={index===selected} tabindex="-1" class:highlighted={index===selected} class="command-result" onclick={()=>choose(command.id)}><span><strong>{command.label}</strong><small>{command.detail}</small></span><code>{command.slash}</code></button>{/each}
  </div>
  {#if !results.length}<p class="small-empty" role="status">No commands match “{search}”. Try a task like “Git” or “model”.</p>{/if}
  <div class="palette-foot"><span><kbd>↑</kbd><kbd>↓</kbd> Navigate <kbd>Enter</kbd> Run</span><span>{results.length} commands · one dispatcher</span></div>
</dialog>
