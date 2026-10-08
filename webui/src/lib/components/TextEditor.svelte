<script lang="ts">
  import {onMount,tick} from 'svelte';
  import {touchFileDraft,noteFileDraftChange,type FileDraft} from '../files/drafts.svelte';
  import {
    applyChange,deletionChange,difference,findText,literalEquals,locateText,offsetForLine,replaceAllText,replacementChange,safeTextOffset,TEXT_PAGE_UNITS,
    type Change,type TextIndex,
  } from '../files/text';
  import {recordTextDelta,stepTextHistory} from '../files/history';
  let {draft,path,index,onsave}:{draft:FileDraft;path:string;index:TextIndex;onsave:()=>Promise<void>}=$props();
  const view=draft.view;
  let windowStart=$state(safeTextOffset(draft.text,view.windowStart??0));
  let windowEnd=$state(safeTextOffset(draft.text,Math.min(draft.text.length,windowStart+TEXT_PAGE_UNITS)));
  let windowText=$derived(draft.text.slice(windowStart,windowEnd));
  let field=$state<HTMLTextAreaElement>(),findInput=$state<HTMLInputElement>();
  let composing=$state(false),composition:EditAnchor|undefined,pendingInput:EditAnchor|undefined;
  let compositionCommitValue:string|undefined;
  let cursorRestoring=false,cursorRestoreEpoch=0;
  let searchBusy=$state(false),navigationBusy=$state(false),locating=$state(false),location=$state({line:1,column:1});
  let searchController:AbortController|undefined,navigationController:AbortController|undefined;

  interface EditAnchor {before:string;offset:number;start:number;end:number;inputType:string;logical?:{start:number;end:number;removed:string}}
  function changed(){touchFileDraft(draft);}
  function cancelled(error:unknown){return error instanceof Error&&error.name==='AbortError';}
  function cancelSearch(){searchController?.abort();searchController=undefined;searchBusy=false;}
  function cancelNavigation(){navigationController?.abort();navigationController=undefined;navigationBusy=false;}
  function extendedSelection(){return view.selectionStart<windowStart||view.selectionEnd>windowEnd;}
  function logicalSelection(){return extendedSelection()?{start:view.selectionStart,end:view.selectionEnd,removed:draft.text.slice(view.selectionStart,view.selectionEnd)}:undefined;}
  function rememberCursor(persist=true){
    if(!field||cursorRestoring||composing)return;
    const clippedStart=Math.max(0,Math.min(field.value.length,view.selectionStart-windowStart));
    const clippedEnd=Math.max(0,Math.min(field.value.length,view.selectionEnd-windowStart));
    if(!extendedSelection()||field.selectionStart!==clippedStart||field.selectionEnd!==clippedEnd){
      view.selectionStart=windowStart+field.selectionStart;view.selectionEnd=windowStart+field.selectionEnd;
      view.selectionDirection=field.selectionDirection as 'forward'|'backward'|'none';
    }
    view.windowStart=windowStart;
    view.scrollTop=field.scrollTop;view.scrollLeft=field.scrollLeft;
    if(persist)changed();
  }
  function restoreCursor(focus=false){
    if(!field)return;
    cursorRestoring=true;const epoch=++cursorRestoreEpoch;
    const start=Math.min(draft.text.length,view.selectionStart),end=Math.min(draft.text.length,Math.max(start,view.selectionEnd));
    field.setSelectionRange(Math.max(0,start-windowStart),Math.max(0,end-windowStart),view.selectionDirection);field.scrollTop=view.scrollTop;field.scrollLeft=view.scrollLeft;
    if(focus)field.focus();
    queueMicrotask(()=>{if(epoch===cursorRestoreEpoch)cursorRestoring=false;});
  }
  function revealSelection(target=view.selectionStart){
    cursorRestoring=true;
    const start=Math.min(draft.text.length,target);
    const boundedEnd=safeTextOffset(draft.text,Math.min(draft.text.length,windowStart+TEXT_PAGE_UNITS));
    if(start<windowStart||start>boundedEnd){
      windowStart=safeTextOffset(draft.text,Math.floor(start/TEXT_PAGE_UNITS)*TEXT_PAGE_UNITS);
      view.windowStart=windowStart;view.scrollTop=0;view.scrollLeft=0;
    }
    windowEnd=safeTextOffset(draft.text,Math.min(draft.text.length,windowStart+TEXT_PAGE_UNITS));
    void tick().then(()=>restoreCursor());
  }
  async function moveWindow(previous=false){
    if(composing)return;
    cancelSearch();cancelNavigation();
    const start=previous?Math.max(0,windowStart-TEXT_PAGE_UNITS):windowEnd;
    windowStart=safeTextOffset(draft.text,start);windowEnd=safeTextOffset(draft.text,Math.min(draft.text.length,windowStart+TEXT_PAGE_UNITS));
    view.windowStart=windowStart;view.selectionStart=windowStart;view.selectionEnd=windowStart;view.selectionDirection='none';view.scrollTop=0;view.scrollLeft=0;
    changed();await tick();restoreCursor(true);
  }
  function retainWindowChange(before:string,after:string,change?:Change){
    const local=change??difference(before,after),global={...local,at:windowStart+local.at};
    const source=draft.text;draft.text=applyChange(source,global);noteFileDraftChange(draft,source,global);windowEnd=windowStart+after.length;
    return global;
  }
  function boundInputWindow(){
    if(composing||windowEnd-windowStart<=TEXT_PAGE_UNITS)return;
    const cursor=Math.min(draft.text.length,view.selectionEnd);
    windowStart=safeTextOffset(draft.text,Math.max(0,cursor-Math.floor(TEXT_PAGE_UNITS/2)));
    windowEnd=safeTextOffset(draft.text,Math.min(draft.text.length,windowStart+TEXT_PAGE_UNITS));
    view.windowStart=windowStart;view.scrollTop=0;view.scrollLeft=0;
    void tick().then(()=>restoreCursor());
  }
  function beforeInput(event:InputEvent){
    if(!field||composing)return;
    if(event.inputType==='historyUndo'||event.inputType==='historyRedo'){
      event.preventDefault();void history(event.inputType==='historyUndo');return;
    }
    pendingInput={before:windowText,offset:windowStart,start:field.selectionStart,end:field.selectionEnd,inputType:event.inputType,logical:logicalSelection()};
  }
  function input(event:InputEvent){
    if(!field)return;
    cancelSearch();cancelNavigation();
    const after=field.value;
    // Native composition owns its provisional textarea bytes. Publish one exact
    // document change only when composition ends, rather than rebasing each
    // provisional update against a clipped document selection.
    if(composing)return;
    if(compositionCommitValue===after){compositionCommitValue=undefined;pendingInput=undefined;return;}
    compositionCommitValue=undefined;
    const anchor=pendingInput;
    const change=anchor?((anchor.logical?replacementChange(anchor.before,after,anchor.start,anchor.end):inputChange(anchor,after,field.selectionStart))??difference(anchor.before,after)):undefined;
    let global:Change;
    if(anchor?.logical){
      global={at:anchor.logical.start,removed:anchor.logical.removed,inserted:change!.inserted};
      const source=draft.text;draft.text=applyChange(source,global);noteFileDraftChange(draft,source,global);
      windowEnd=windowStart+after.length;view.selectionStart=global.at+global.inserted.length;view.selectionEnd=view.selectionStart;view.selectionDirection='none';
    }else global=retainWindowChange(windowText,after,change);
    recordTextDelta(draft.history,global);
    pendingInput=undefined;if(!anchor?.logical)rememberCursor(false);
    if(anchor?.logical){revealSelection();void tick().then(()=>restoreCursor());}
    boundInputWindow();changed();
  }
  function inputChange(anchor:EditAnchor,after:string,afterStart:number):Change|undefined {
    if(anchor.inputType.startsWith('delete'))return deletionChange(anchor.before,after,anchor.start,afterStart);
    if(anchor.inputType.startsWith('insert')&&!['insertTranspose','insertFromDrop'].includes(anchor.inputType))return replacementChange(anchor.before,after,anchor.start,anchor.end);
    return undefined;
  }
  function startComposition(){
    if(!field)return;
    cancelSearch();cancelNavigation();composing=true;compositionCommitValue=undefined;pendingInput=undefined;composition={before:windowText,offset:windowStart,start:field.selectionStart,end:field.selectionEnd,inputType:'composition',logical:logicalSelection()};
  }
  function endComposition(event:CompositionEvent){
    const after=field?.value??windowText,anchor=composition;
    composing=false;composition=undefined;pendingInput=undefined;compositionCommitValue=after;
    if(anchor&&after===anchor.before&&!event.data){
      view.selectionStart=anchor.logical?.start??anchor.offset+anchor.start;view.selectionEnd=anchor.logical?.end??anchor.offset+anchor.end;
      revealSelection();changed();return;
    }
    if(anchor){
      const change=replacementChange(anchor.before,after,anchor.start,anchor.end)??difference(anchor.before,after);
      const global=anchor.logical?{at:anchor.logical.start,removed:anchor.logical.removed,inserted:change.inserted}:{...change,at:anchor.offset+change.at};
      if(global.removed!==global.inserted){
        const source=draft.text;draft.text=applyChange(source,global);noteFileDraftChange(draft,source,global);
        recordTextDelta(draft.history,global);
      }
      view.selectionStart=global.at+global.inserted.length;view.selectionEnd=view.selectionStart;view.selectionDirection='none';
    }
    revealSelection();boundInputWindow();changed();
  }
  async function history(back:boolean) {
    cancelSearch();cancelNavigation();
    const before=draft.text,step=stepTextHistory(draft.history,before,back);if(!step)return;
    const {change}=step;draft.text=step.text;
    noteFileDraftChange(draft,before,back?{at:change.at,removed:change.inserted,inserted:change.removed}:change);
    const end=change.at+(back?change.removed:change.inserted).length;
    view.selectionStart=end;view.selectionEnd=end;view.selectionDirection='none';revealSelection();changed();
    await tick();restoreCursor(true);
  }
  async function find(previous=false) {
    if(!view.needle||!field)return;
    cancelSearch();const controller=new AbortController();searchController=controller;searchBusy=true;
    const source=draft.text,from=previous?view.selectionStart:view.selectionEnd;
    view.searchNote='Searching…';changed();
    try{
      const match=await findText(source,view.needle,from,previous,view.matchCase,controller.signal);
      if(controller.signal.aborted||source!==draft.text)return;
      if(!match){view.searchNote='No matches';changed();return;}
      view.selectionStart=match.at;view.selectionEnd=match.at+match.length;view.selectionDirection='none';revealSelection();await tick();restoreCursor(true);
      const [found,first]=await Promise.all([locateText(source,match.at,index,controller.signal),locateText(source,windowStart,index,controller.signal)]);
      if(controller.signal.aborted)return;
      field.scrollTop=Math.max(0,(found.line-first.line)*lineHeight()-field.clientHeight/2);rememberCursor(false);
      view.searchNote='Match selected';changed();
    }catch(error){if(!cancelled(error))throw error;}
    finally{if(searchController===controller){searchController=undefined;searchBusy=false;}}
  }
  async function replace(all=false) {
    if(!view.needle||!field||searchBusy)return;
    const before=draft.text;
    if(all){
      cancelSearch();const controller=new AbortController();searchController=controller;searchBusy=true;
      view.searchNote='Replacing…';changed();
      try{
        const result=await replaceAllText(before,view.needle,view.replacement,view.matchCase,controller.signal);
        if(controller.signal.aborted||draft.text!==before)return;
        if(result.change)recordTextDelta(draft.history,result.change);
        draft.text=result.text;if(result.change)noteFileDraftChange(draft,before,result.change);view.selectionStart=Math.min(view.selectionStart,result.text.length);view.selectionEnd=Math.min(view.selectionEnd,result.text.length);
        revealSelection();view.searchNote=`Replaced ${result.count} match${result.count===1?'':'es'}`;changed();await tick();restoreCursor(true);
      }catch(error){if(!cancelled(error))throw error;}
      finally{if(searchController===controller){searchController=undefined;searchBusy=false;}}
      return;
    }
    const start=view.selectionStart,end=view.selectionEnd,selected=before.slice(start,end);
    if(!literalEquals(selected,view.needle,view.matchCase)){await find();return;}
    const change={at:start,removed:selected,inserted:view.replacement};
    recordTextDelta(draft.history,change);draft.text=applyChange(before,change);noteFileDraftChange(draft,before,change);
    const cursor=start+view.replacement.length;view.selectionStart=cursor;view.selectionEnd=cursor;view.selectionDirection='none';revealSelection();changed();
    await tick();restoreCursor(true);await find();
  }
  async function jump(){
    if(!field)return;
    cancelNavigation();const controller=new AbortController();navigationController=controller;navigationBusy=true;
    const source=draft.text;
    try{
      const at=await offsetForLine(source,view.goLine,index,controller.signal);if(controller.signal.aborted||source!==draft.text)return;
      view.selectionStart=at;view.selectionEnd=at;view.selectionDirection='none';
      revealSelection();await tick();restoreCursor(true);
      const first=await locateText(source,windowStart,index,controller.signal);if(controller.signal.aborted)return;
      field.scrollTop=Math.max(0,(view.goLine-first.line)*lineHeight());rememberCursor(false);changed();
    }catch(error){if(!cancelled(error))throw error;}
    finally{if(navigationController===controller){navigationController=undefined;navigationBusy=false;}}
  }
  async function search(){view.searching=true;changed();await tick();findInput?.focus();findInput?.select();}
  async function selectAll(){
    cancelSearch();cancelNavigation();view.selectionStart=0;view.selectionEnd=draft.text.length;view.selectionDirection='forward';
    revealSelection();changed();await tick();restoreCursor(true);
  }
  async function replaceSelection(inserted:string){
    const source=draft.text,change={at:view.selectionStart,removed:source.slice(view.selectionStart,view.selectionEnd),inserted};
    recordTextDelta(draft.history,change);draft.text=applyChange(source,change);noteFileDraftChange(draft,source,change);
    view.selectionStart=change.at+inserted.length;view.selectionEnd=view.selectionStart;view.selectionDirection='none';
    revealSelection();changed();await tick();restoreCursor(true);
  }
  function copySelection(event:ClipboardEvent,cut=false){
    if(!extendedSelection()||!event.clipboardData)return;
    event.preventDefault();event.clipboardData.setData('text/plain',draft.text.slice(view.selectionStart,view.selectionEnd));
    if(cut&&!draft.saving&&!searchBusy&&!composing)void replaceSelection('');
  }
  async function collapseSelection(start:boolean){
    const at=start?view.selectionStart:view.selectionEnd;view.selectionStart=at;view.selectionEnd=at;view.selectionDirection='none';
    revealSelection();changed();await tick();restoreCursor(true);
  }
  function searchChanged(){cancelSearch();view.searchNote='';changed();}
  function lineHeight(){const value=field?parseFloat(getComputedStyle(field).lineHeight):NaN;return Number.isFinite(value)?value:20;}
  function adjacentOffset(at:number,previous:boolean){
    if(previous){
      let next=Math.max(0,at-1);
      if(next>0&&(draft.text[next]==='\n'&&draft.text[next-1]==='\r'
        ||draft.text.charCodeAt(next)>=0xdc00&&draft.text.charCodeAt(next)<=0xdfff&&draft.text.charCodeAt(next-1)>=0xd800&&draft.text.charCodeAt(next-1)<=0xdbff))next--;
      return next;
    }
    let next=Math.min(draft.text.length,at+1);
    if(draft.text[at]==='\r'&&draft.text[next]==='\n')next++;
    return safeTextOffset(draft.text,next);
  }
  function documentKey(event:KeyboardEvent):boolean {
    if(!field||event.target!==field||event.altKey||composing)return false;
    const key=event.key,control=event.ctrlKey||event.metaKey;
    if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End','PageUp','PageDown'].includes(key))return false;
    if(control&&!['Home','End'].includes(key))return false;
    const active=view.selectionDirection==='backward'?view.selectionStart:view.selectionEnd;
    const local=Math.max(0,Math.min(windowText.length,active-windowStart));
    const firstLine=windowText.slice(0,local).lastIndexOf('\n')<0;
    const lastLine=windowText.indexOf('\n',local)<0;
    const crosses=key==='ArrowLeft'&&active<=windowStart&&windowStart>0
      ||key==='ArrowRight'&&active>=windowEnd&&windowEnd<draft.text.length
      ||['ArrowUp','Home'].includes(key)&&firstLine&&windowStart>0
      ||['ArrowDown','End'].includes(key)&&lastLine&&windowEnd<draft.text.length
      ||['PageUp','PageDown'].includes(key)&&(windowStart>0||windowEnd<draft.text.length);
    if(!extendedSelection()&&!crosses&&!(control&&['Home','End'].includes(key)))return false;
    event.preventDefault();event.stopPropagation();
    if(!event.shiftKey&&view.selectionStart!==view.selectionEnd&&['ArrowLeft','ArrowRight'].includes(key)){
      void collapseSelection(key==='ArrowLeft');return true;
    }
    const anchor=event.shiftKey?(view.selectionDirection==='backward'?view.selectionEnd:view.selectionStart):active;
    cancelSearch();cancelNavigation();const controller=new AbortController();navigationController=controller;navigationBusy=true;
    const source=draft.text;
    void (async()=>{
      let at=active;
      if(control)at=key==='Home'?0:source.length;
      else if(key==='ArrowLeft'||key==='ArrowRight')at=adjacentOffset(active,key==='ArrowLeft');
      else {
        const current=await locateText(source,active,index,controller.signal);
        const distance=key==='PageUp'||key==='PageDown'?Math.max(1,Math.floor(field!.clientHeight/lineHeight())):1;
        const targetLine=['ArrowUp','PageUp'].includes(key)?Math.max(1,current.line-distance)
          :['ArrowDown','PageDown'].includes(key)?current.line+distance:current.line;
        const start=await offsetForLine(source,targetLine,index,controller.signal);
        let end=await offsetForLine(source,targetLine+1,index,controller.signal);
        if(end>start&&source[end-1]==='\n'){end--;if(end>start&&source[end-1]==='\r')end--;}
        else if(end>start&&source[end-1]==='\r')end--;
        at=key==='Home'?start:key==='End'?end:safeTextOffset(source,Math.min(end,start+current.column-1));
      }
      if(controller.signal.aborted||source!==draft.text)return;
      view.selectionStart=event.shiftKey?Math.min(anchor,at):at;view.selectionEnd=event.shiftKey?Math.max(anchor,at):at;
      view.selectionDirection=event.shiftKey&&at!==anchor?(at<anchor?'backward':'forward'):'none';
      revealSelection(at);changed();await tick();restoreCursor(true);
    })().catch(error=>{if(!cancelled(error))throw error;}).finally(()=>{
      if(navigationController===controller){navigationController=undefined;navigationBusy=false;}
    });
    return true;
  }
  async function boundaryKey(event:KeyboardEvent):Promise<void>{
    if(!field||event.target!==field||event.ctrlKey||event.metaKey||event.altKey||event.shiftKey
      ||draft.saving||searchBusy||field.selectionStart!==field.selectionEnd)return;
    const before=field.selectionStart===0&&windowStart>0;
    const after=field.selectionEnd===field.value.length&&windowEnd<draft.text.length;
    if(!(before&&['Backspace','ArrowLeft'].includes(event.key)||after&&['Delete','ArrowRight'].includes(event.key)))return;
    event.preventDefault();event.stopPropagation();cancelNavigation();
    const at=before?windowStart:windowEnd;
    let other=at+(before?-1:1);
    if(before&&other>0&&draft.text.charCodeAt(other)>=0xdc00&&draft.text.charCodeAt(other)<=0xdfff
      &&draft.text.charCodeAt(other-1)>=0xd800&&draft.text.charCodeAt(other-1)<=0xdbff)other--;
    if(after)other=safeTextOffset(draft.text,other);
    let cursor=other;
    if(event.key==='Backspace'||event.key==='Delete'){
      const start=Math.min(at,other),end=Math.max(at,other),change={at:start,removed:draft.text.slice(start,end),inserted:''};
      const source=draft.text;recordTextDelta(draft.history,change);draft.text=applyChange(source,change);noteFileDraftChange(draft,source,change);cursor=start;
    }
    view.selectionStart=cursor;view.selectionEnd=cursor;view.selectionDirection='none';
    // Recenter at the boundary so the next key observes adjacent document text.
    windowStart=safeTextOffset(draft.text,Math.max(0,cursor-Math.floor(TEXT_PAGE_UNITS/2)));
    windowEnd=safeTextOffset(draft.text,Math.min(draft.text.length,windowStart+TEXT_PAGE_UNITS));
    view.windowStart=windowStart;view.scrollTop=0;view.scrollLeft=0;changed();await tick();restoreCursor(true);
  }
  function keys(event:KeyboardEvent){
    if(event.isComposing||composing)return;
    if(documentKey(event))return;
    if(event.target===field&&extendedSelection()&&!event.ctrlKey&&!event.metaKey&&!event.altKey&&!event.shiftKey&&['ArrowLeft','ArrowRight'].includes(event.key)){
      event.preventDefault();event.stopPropagation();void collapseSelection(event.key==='ArrowLeft');return;
    }
    void boundaryKey(event);
    if(event.ctrlKey||event.metaKey){const key=event.key.toLowerCase();
      if(key==='a'&&event.target===field){event.preventDefault();event.stopPropagation();void selectAll();}
      if(key==='s'){event.preventDefault();event.stopPropagation();void onsave();}
      if(key==='f'||key==='h'){event.preventDefault();event.stopPropagation();void search();}
      if((key==='z'||key==='y')&&event.target===field){event.preventDefault();event.stopPropagation();if(!draft.saving&&!searchBusy)void history(key==='z'&&!event.shiftKey);}
    }
    if(event.key==='Escape'&&view.searching){event.preventDefault();cancelSearch();view.searching=false;changed();field?.focus();}
  }
  $effect(()=>{
    const source=draft.text,at=view.selectionStart,current=index,controller=new AbortController();locating=true;
    void locateText(source,at,current,controller.signal).then(value=>{if(!controller.signal.aborted){location=value;locating=false;}}).catch(error=>{if(!cancelled(error))throw error;});
    return()=>controller.abort();
  });
  onMount(()=>{
    revealSelection();void tick().then(()=>restoreCursor());
    return()=>{cancelSearch();cancelNavigation();rememberCursor();};
  });
</script>
<svelte:window onkeydown={keys}/>
<div class="text-editor">
  <div class="editor-tools">
    {#if draft.text.length>TEXT_PAGE_UNITS||windowStart>0}<button class="key small" disabled={windowStart===0||composing||draft.saving||searchBusy} onclick={()=>void moveWindow(true)}>Previous text page</button><span role="status">Editing characters {windowStart+1}–{windowEnd} of {draft.text.length.toLocaleString()}. Find and line navigation use the complete draft.</span><button class="key small" disabled={windowEnd>=draft.text.length||composing||draft.saving||searchBusy} onclick={()=>void moveWindow()}>Next text page</button>{/if}
    <button class="key small" disabled={!draft.history.undo.length||draft.saving||searchBusy||composing} onclick={()=>void history(true)}>Undo</button>
    <button class="key small" disabled={!draft.history.redo.length||draft.saving||searchBusy||composing} onclick={()=>void history(false)}>Redo</button>
    <button class="key small" disabled={composing} aria-expanded={view.searching} onclick={()=>{if(view.searching){cancelSearch();view.searching=false;changed();}else void search();}}>Find / replace</button>
    <label><input type="checkbox" bind:checked={view.wrap} onchange={changed}/> Wrap</label>
    <form onsubmit={event=>{event.preventDefault();if(!composing)void jump();}}><label>Line <input type="number" aria-label="Go to line" min="1" bind:value={view.goLine} oninput={changed}/></label><button class="key small" disabled={navigationBusy||composing}>{navigationBusy?'Locating…':'Go'}</button></form>
  </div>
  {#if view.searching}<div class="editor-search">
    <input bind:this={findInput} aria-label="Find text" placeholder="Find text" bind:value={view.needle} oninput={searchChanged} onkeydown={event=>{if(event.key==='Enter'){event.preventDefault();void find(event.shiftKey);}}}/>
    <button class="key small" disabled={!view.needle||searchBusy} onclick={()=>void find(true)}>Previous</button><button class="key small" disabled={!view.needle||searchBusy} onclick={()=>void find()}>Next</button>
    <label><input type="checkbox" bind:checked={view.matchCase} onchange={searchChanged}/> Match case</label>
    <input aria-label="Replace with" placeholder="Replace with" bind:value={view.replacement} oninput={changed}/>
    <button class="key small" disabled={!view.needle||draft.saving||searchBusy||composing} onclick={()=>void replace()}>Replace</button><button class="key small" disabled={!view.needle||draft.saving||searchBusy||composing} onclick={()=>void replace(true)}>Replace all</button>
    <span role="status">{view.searchNote}</span>
  </div>{/if}
  <textarea class="file-textarea" bind:this={field} aria-label={`Edit ${path}`} value={windowText} wrap={view.wrap?'soft':'off'} spellcheck="false" autocapitalize="off" autocomplete="off" readonly={draft.saving||searchBusy}
    onbeforeinput={beforeInput} oninput={input} onselect={()=>rememberCursor()} onclick={()=>rememberCursor()} onkeyup={()=>rememberCursor()} onscroll={()=>rememberCursor()} oncopy={event=>copySelection(event)} oncut={event=>copySelection(event,true)}
    oncompositionstart={startComposition} oncompositionend={endComposition}></textarea>
  <div class="editor-status"><span>{locating?'Locating cursor…':`Ln ${location.line}, Col ${location.column}`}{view.selectionEnd>view.selectionStart?` · ${view.selectionEnd-view.selectionStart} selected`:''}</span><span>UTF-8{draft.bom?' BOM':''} · {draft.newline.toUpperCase()} · Tab moves focus</span></div>
</div>
