<script lang="ts">
  import {safeTextOffset,TEXT_PAGE_UNITS} from '../files/text';
  let {text,label='Text',className=''}:{text:string;label?:string;className?:string}=$props();
  let page=$state(0);
  let source='';
  let start=$derived(safeTextOffset(text,page*TEXT_PAGE_UNITS));
  let end=$derived(safeTextOffset(text,Math.min(text.length,(page+1)*TEXT_PAGE_UNITS)));
  $effect.pre(()=>{
    const current=text;if(current===source)return;
    const previousStart=safeTextOffset(source,page*TEXT_PAGE_UNITS);
    const previousEnd=safeTextOffset(source,Math.min(source.length,(page+1)*TEXT_PAGE_UNITS));
    if(current.length<previousEnd||safeTextOffset(current,page*TEXT_PAGE_UNITS)!==previousStart
      ||current.slice(previousStart,previousEnd)!==source.slice(previousStart,previousEnd))page=0;
    source=current;
  });
</script>
{#if text.length>TEXT_PAGE_UNITS}<div class="activity-history">
  <button class="key small" disabled={!page} onclick={()=>page--}>Previous text page</button>
  <span>Characters {(start+1).toLocaleString()}–{end.toLocaleString()} of {text.length.toLocaleString()}</span>
  <button class="key small" disabled={end>=text.length} onclick={()=>page++}>Next text page</button>
</div>{/if}
<!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to scroll the read-only text page.) -->
<pre class={className} role="region" aria-label={label} tabindex="0">{text.slice(start,end)}</pre>
