<script lang="ts">
  import {tick} from 'svelte';
  import type {FileDraft} from '../files/drafts.svelte';
  import {applyChange,difference,type Change} from '../files/text';
  let {draft,path,onsave}:{draft:FileDraft;path:string;onsave:()=>Promise<void>}=$props();
  let field=$state<HTMLTextAreaElement>(),findInput=$state<HTMLInputElement>();
  let wrap=$state(true),searching=$state(false),needle=$state(''),replacement=$state(''),matchCase=$state(false),searchNote=$state('');
  let position=$state(0),selection=$state(0),goLine=$state(1),composing=false,compositionBefore='';
  let undo=$state<Change[]>([]),redo=$state<Change[]>([]);
  let location=$derived.by(()=>{let line=1,start=0;for(let i=0;i<position;i++)if(draft.text[i]==='\n'){line++;start=i+1;}return {line,column:position-start+1};});
  function cursor(){if(field){position=field.selectionStart;selection=field.selectionEnd-position;}}
  function record(before:string,after:string) {
    if(before===after)return;
    undo.push(difference(before,after));redo=[];
    let budget=0;
    for(let i=undo.length-1;i>=0;i--){budget+=undo[i]!.removed.length+undo[i]!.inserted.length;if(budget>2_000_000||undo.length-i>200){undo=undo.slice(i+1);break;}}
  }
  function input(){if(!field)return;const next=field.value;if(!composing)record(draft.text,next);draft.text=next;cursor();}
  async function history(back:boolean) {
    const change=(back?undo:redo).pop();if(!change)return;
    draft.text=applyChange(draft.text,change,back);(back?redo:undo).push(change);
    await tick();field?.focus();const end=change.at+(back?change.removed:change.inserted).length;field?.setSelectionRange(end,end);cursor();
  }
  async function find(previous=false) {
    if(!needle||!field)return;
    const text=matchCase?draft.text:draft.text.toLocaleLowerCase(),term=matchCase?needle:needle.toLocaleLowerCase();
    let at=previous?text.lastIndexOf(term,Math.max(-1,field.selectionStart-1)):text.indexOf(term,field.selectionEnd);
    if(at<0)at=previous?text.lastIndexOf(term):text.indexOf(term);
    if(at<0){searchNote='No matches';return;}
    field.focus();field.setSelectionRange(at,at+needle.length);cursor();searchNote='Match selected';
    // Browser selection is authoritative; approximate scrolling without a duplicate text layout.
    const line=draft.text.slice(0,at).split('\n').length-1;
    field.scrollTop=line*parseFloat(getComputedStyle(field).lineHeight)-field.clientHeight/2;
  }
  async function replace(all=false) {
    if(!needle||!field)return;
    const before=draft.text;
    if(all){
      const escaped=needle.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');let count=0;
      const after=before.replace(new RegExp(escaped,matchCase?'g':'gi'),()=>{count++;return replacement;});
      record(before,after);draft.text=after;searchNote=`Replaced ${count} match${count===1?'':'es'}`;
    }else{
      const selected=before.slice(field.selectionStart,field.selectionEnd);
      if((matchCase?selected:selected.toLocaleLowerCase())!==(matchCase?needle:needle.toLocaleLowerCase())){await find();return;}
      const start=field.selectionStart;
      const after=before.slice(0,start)+replacement+before.slice(field.selectionEnd);
      record(before,after);draft.text=after;await tick();field.setSelectionRange(start+replacement.length,start+replacement.length);await find();
    }
  }
  function jump(){if(!field)return;let at=0;for(let line=1;line<goLine;line++){const next=draft.text.indexOf('\n',at);if(next<0)break;at=next+1;}field.focus();field.setSelectionRange(at,at);cursor();field.scrollTop=(goLine-1)*parseFloat(getComputedStyle(field).lineHeight);}
  async function search(){searching=true;await tick();findInput?.focus();findInput?.select();}
  function keys(event:KeyboardEvent){
    if(event.isComposing)return;
    if(event.ctrlKey||event.metaKey){const key=event.key.toLowerCase();
      if(key==='s'){event.preventDefault();event.stopPropagation();void onsave();}
      if(key==='f'||key==='h'){event.preventDefault();event.stopPropagation();void search();}
      if((key==='z'||key==='y')&&event.target===field){event.preventDefault();event.stopPropagation();if(!draft.saving)void history(key==='z'&&!event.shiftKey);}
    }
    if(event.key==='Escape'&&searching){event.preventDefault();searching=false;field?.focus();}
  }
</script>
<svelte:window onkeydown={keys}/>
<div class="text-editor">
  <div class="editor-tools">
    <button class="key small" disabled={!undo.length||draft.saving} onclick={()=>void history(true)}>Undo</button>
    <button class="key small" disabled={!redo.length||draft.saving} onclick={()=>void history(false)}>Redo</button>
    <button class="key small" aria-expanded={searching} onclick={()=>searching?searching=false:void search()}>Find / replace</button>
    <label><input type="checkbox" bind:checked={wrap}/> Wrap</label>
    <form onsubmit={event=>{event.preventDefault();jump();}}><label>Line <input type="number" aria-label="Go to line" min="1" max="100000000" bind:value={goLine}/></label><button class="key small">Go</button></form>
  </div>
  {#if searching}<div class="editor-search">
    <input bind:this={findInput} aria-label="Find text" placeholder="Find text" bind:value={needle} onkeydown={event=>{if(event.key==='Enter'){event.preventDefault();void find(event.shiftKey);}}}/>
    <button class="key small" disabled={!needle} onclick={()=>void find(true)}>Previous</button><button class="key small" disabled={!needle} onclick={()=>void find()}>Next</button>
    <label><input type="checkbox" bind:checked={matchCase}/> Match case</label>
    <input aria-label="Replace with" placeholder="Replace with" bind:value={replacement}/>
    <button class="key small" disabled={!needle||draft.saving} onclick={()=>void replace()}>Replace</button><button class="key small" disabled={!needle||draft.saving} onclick={()=>void replace(true)}>Replace all</button>
    <span role="status">{searchNote}</span>
  </div>{/if}
  <textarea class="file-textarea" bind:this={field} aria-label={`Edit ${path}`} value={draft.text} wrap={wrap?'soft':'off'} spellcheck="false" autocapitalize="off" autocomplete="off" readonly={draft.saving}
    oninput={input} onselect={cursor} onclick={cursor} onkeyup={cursor}
    oncompositionstart={()=>{composing=true;compositionBefore=draft.text;}}
    oncompositionend={()=>{composing=false;record(compositionBefore,field?.value??draft.text);draft.text=field?.value??draft.text;}}></textarea>
  <div class="editor-status"><span>Ln {location.line}, Col {location.column}{selection?` · ${selection} selected`:''}</span><span>UTF-8{draft.bom?' BOM':''} · {draft.newline.toUpperCase()} · Tab moves focus</span></div>
</div>
