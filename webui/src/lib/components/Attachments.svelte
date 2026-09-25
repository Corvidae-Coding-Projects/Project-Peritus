<script lang="ts">
  import {ui,dispatch,attempt,openFile} from '../workspace.svelte';
  import Icon from './Icon.svelte';
  let files=$derived(ui.attachments[ui.sessionId]??[]);
</script>
{#if files.length||ui.attaching[ui.sessionId]}<div class="attachment-tray" aria-label="Attachments for next message">
  <div class="attachment-heading"><span>Attached to next message · saved snapshots</span><small>UTF-8 text · 64 KiB combined message limit</small></div>
  {#each files as file(file.id)}<div class="attachment-chip"><button type="button" class="flat" title={`View current source: ${file.path}\nThe attached snapshot remains unchanged. SHA-256: ${file.digest}`} onclick={()=>openFile(file.path)}><Icon name="file" size={14}/><span>{file.path}</span><small>{file.bytes<1024?`${file.bytes} B`:`${(file.bytes/1024).toLocaleString(undefined,{maximumFractionDigits:1})} KiB`}</small></button><button type="button" class="flat icon-button" aria-label={`Remove attachment ${file.path}`} onclick={()=>ui.attachments[ui.sessionId]=files.filter(item=>item.id!==file.id)}><Icon name="close" size={14}/></button></div>{/each}
  {#if ui.attaching[ui.sessionId]}<span role="status">Taking file snapshot…</span>{/if}
</div>{/if}
<div class="attachment-hint"><button type="button" class="flat" onclick={()=>void attempt(()=>dispatch('files'))}><Icon name="plus" size={14}/>Attach text file</button><span>Drag from explorer, or use file actions.</span></div>
