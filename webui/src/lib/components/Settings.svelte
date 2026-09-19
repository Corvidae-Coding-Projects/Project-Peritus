<script lang="ts">
  import { ui,defaults,attempt,notify } from '../workspace.svelte';
  import { action } from '../api';
  import type { Preferences } from '../types';
  import Icon from './Icon.svelte';
  let tab=$state('controls'),saving=$state(false),source=$state(ui.config);
  let fileInput=$state<HTMLInputElement>(null!);
  async function save(){saving=true;try{
    if(tab==='dotfile'){ui.preferences=await action<Preferences>('config',{text:source});ui.config=source;}
    else{const value=await action<{preferences:Preferences;config:string}>('preferences',{preferences:ui.preferences});ui.preferences=value.preferences;ui.config=value.config;source=value.config;}
    ui.configError='';notify('Console configuration saved.');
  }finally{saving=false;}}
  async function reset(){ui.preferences={...defaults};tab='controls';await save();}
  function download(){const link=document.createElement('a');const url=URL.createObjectURL(new Blob([ui.config],{type:'text/plain'}));link.href=url;link.download='webui.toml';link.click();URL.revokeObjectURL(url);}
</script>
<div class="settings-view">
  <p class="dialog-description">Tune the instrument to the way you work. Your TOML dotfile controls appearance, behavior, layout, shortcuts, and command aliases.</p>
  <div class="segmented" aria-label="Settings view"><button class:active={tab==='controls'} onclick={()=>tab='controls'}>Controls</button><button class:active={tab==='dotfile'} onclick={()=>{tab='dotfile';source=ui.config;}}>Dotfile</button></div>
  {#if tab==='controls'}
    <div class="settings-grid">
      <fieldset><legend>Material & reading</legend><label>Console finish<select bind:value={ui.preferences.theme}><option value="nixie">Nixie · blackened steel</option><option value="daylight">Daylight · ceramic panel</option><option value="blueprint">Blueprint · blue steel</option></select></label><label>Density<select bind:value={ui.preferences.density}><option value="comfortable">Comfortable</option><option value="compact">Compact</option></select></label><label>Text size <span>{ui.preferences.font_size}px</span><input type="range" min="12" max="22" bind:value={ui.preferences.font_size}/></label><label>Reading font<input bind:value={ui.preferences.font_family}/></label><label>Code font<input bind:value={ui.preferences.mono_family}/></label></fieldset>
      <fieldset><legend>Physical response</legend><label class="switch-row"><span><strong>Mechanical motion</strong><small>Spring-settling tabs and changing cathodes</small></span><input type="checkbox" role="switch" bind:checked={ui.preferences.motion}/></label><label class="switch-row"><span><strong>Control sounds</strong><small>A quiet switch click on direct actions</small></span><input type="checkbox" role="switch" bind:checked={ui.preferences.sound}/></label><p class="setting-note">Your system’s reduced-motion preference always takes priority.</p><label class="switch-row"><span>Wrap code lines</span><input type="checkbox" role="switch" bind:checked={ui.preferences.word_wrap}/></label><label class="switch-row"><span>Preview Markdown by default</span><input type="checkbox" role="switch" bind:checked={ui.preferences.markdown_preview}/></label></fieldset>
      <fieldset><legend>Panel arrangement</legend><label class="switch-row"><span>Show file drawer</span><input type="checkbox" role="switch" bind:checked={ui.preferences.explorer_visible}/></label><label class="switch-row"><span>Show control bank</span><input type="checkbox" role="switch" bind:checked={ui.preferences.controls_visible}/></label><label>File drawer width <span>{ui.preferences.explorer_width}px</span><input type="range" min="180" max="480" step="8" bind:value={ui.preferences.explorer_width}/></label></fieldset>
      <fieldset><legend>Keyboard bindings</legend>{#each Object.keys(ui.preferences.shortcuts) as command}<label class="shortcut-setting"><span>{command}</span><input aria-label={`${command} shortcut`} bind:value={ui.preferences.shortcuts[command]}/></label>{/each}<p class="setting-note">Mod is Ctrl on Linux/Windows and Command on macOS. Add command IDs and aliases in the dotfile.</p></fieldset>
    </div>
  {:else}
    <div class="dotfile-toolbar"><code>{ui.configPath}</code><button class="flat" onclick={()=>fileInput.click()}>Import</button><button class="flat" onclick={download}>Export</button></div>
    <input bind:this={fileInput} type="file" accept=".toml,text/plain" hidden onchange={(event)=>void attempt(async()=>{const file=event.currentTarget.files?.[0];if(file)source=await file.text();})}/>
    <textarea class="dotfile-editor" aria-label="Console TOML configuration" bind:value={source} spellcheck="false"></textarea>
    <details class="config-reference"><summary>Command aliases & color overrides</summary><pre>{'[aliases]\nship = "/git push"\ninspect = "/review"\n\n[tokens]\naccent = "#f2a260"\n\n[shortcuts]\ngit = "Mod+Shift+g"\ninspect = "Mod+Alt+r"'}</pre><p>Aliases resolve through the same dispatcher as buttons and slash commands. Color roles: background, panel, display, text, muted, accent, line. Invalid configuration is rejected with an explanation.</p></details>
  {/if}
  <div class="dialog-actions"><button class="flat" onclick={()=>void attempt(reset)}>Reset to defaults</button><button class="key primary" disabled={saving} onclick={()=>void attempt(save)}><Icon name="check"/>{saving?'Saving…':'Save configuration'}</button></div>
</div>
