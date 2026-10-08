<script lang="ts">
  import { ui,defaults,attempt,notify,clonePreferences,editSettingsPreferences,editSettingsSource,reconcileConfiguration,settingsSnapshot } from '../workspace.svelte';
  import { action } from '../api';
  import { recovery } from '../operations.svelte';
  import type { Preferences } from '../types';
  import Icon from './Icon.svelte';
  import Recovery from './Recovery.svelte';
  let fileInput=$state<HTMLInputElement>(null!);
  let held=$derived(recovery.pending.some(item=>item.command==='config'||item.command==='preferences'));
  let saving=$derived(!!ui.settings.activeOperation&&ui.settings.requestOperation===ui.settings.activeOperation&&recovery.pending.some(item=>item.operation===ui.settings.activeOperation));
  async function save(){
    if(saving)return;
    if(held)throw new Error('Resolve the original configuration operation before saving again.');
    const command=ui.settings.view==='dotfile'?'config':'preferences';
    const submitted=settingsSnapshot(),operation=crypto.randomUUID();
    ui.settings.activeOperation=operation;ui.settings.requestOperation=operation;
    try{
      if(command==='config'){
        const text=ui.settings.source;
        const value=await action<Preferences>('config',{text},{operation,settings:submitted});
        if(ui.settings.activeOperation!==operation)return;
        reconcileConfiguration(command,{text},value as unknown as Record<string,unknown>,submitted);
      }else{
        const preferences=clonePreferences(ui.preferences);
        const value=await action<{preferences:Preferences;config:string}>('preferences',{preferences},{operation,settings:submitted});
        if(ui.settings.activeOperation!==operation)return;
        reconcileConfiguration(command,{preferences},value as unknown as Record<string,unknown>,submitted);
      }
      notify('Console configuration saved.');
    }finally{
      if(ui.settings.requestOperation===operation)ui.settings.requestOperation='';
      if(ui.settings.activeOperation===operation&&!recovery.pending.some(item=>item.operation===operation))ui.settings.activeOperation='';
    }
  }
  async function reset(){
    ui.preferences=clonePreferences(defaults);editSettingsPreferences();ui.settings.view='controls';
    if(saving||held){notify('Defaults are retained as a new draft. Save them after resolving the current configuration operation.');return;}
    await save();
  }
  async function importConfig(file:File|undefined){
    if(!file)return;
    editSettingsSource(await file.text());
  }
  function download(){const link=document.createElement('a');const url=URL.createObjectURL(new Blob([ui.config],{type:'text/plain'}));link.href=url;link.download='webui.toml';link.click();URL.revokeObjectURL(url);}
</script>
<div class="settings-view">
  <p class="dialog-description">Tune the instrument to the way you work. Your TOML dotfile controls appearance, behavior, layout, shortcuts, and command aliases.</p>
  <Recovery configurationOnly/>
  <div class="segmented" aria-label="Settings view"><button class:active={ui.settings.view==='controls'} onclick={()=>ui.settings.view='controls'}>Controls</button><button class:active={ui.settings.view==='dotfile'} onclick={()=>ui.settings.view='dotfile'}>Dotfile</button></div>
  {#if ui.settings.view==='controls'}
    <div class="settings-grid" oninput={editSettingsPreferences}>
      <fieldset><legend>Material & reading</legend><label>Console finish<select bind:value={ui.preferences.theme}><option value="nixie">Nixie · blackened steel</option><option value="daylight">Daylight · ceramic panel</option><option value="blueprint">Blueprint · blue steel</option></select></label><label>Density<select bind:value={ui.preferences.density}><option value="comfortable">Comfortable</option><option value="compact">Compact</option></select></label><label>Text size <span>{ui.preferences.font_size}px</span><input type="number" min="0" step="any" bind:value={ui.preferences.font_size}/></label><label>Reading font<input bind:value={ui.preferences.font_family}/></label><label>Code font<input bind:value={ui.preferences.mono_family}/></label></fieldset>
      <fieldset><legend>Physical response</legend><label class="switch-row"><span><strong>Mechanical motion</strong><small>Spring-settling tabs and changing cathodes</small></span><input type="checkbox" role="switch" bind:checked={ui.preferences.motion}/></label><label class="switch-row"><span><strong>Control sounds</strong><small>A quiet switch click on direct actions</small></span><input type="checkbox" role="switch" bind:checked={ui.preferences.sound}/></label><p class="setting-note">Your system’s reduced-motion preference always takes priority.</p><label class="switch-row"><span>Wrap code lines</span><input type="checkbox" role="switch" bind:checked={ui.preferences.word_wrap}/></label><label class="switch-row"><span>Preview Markdown by default</span><input type="checkbox" role="switch" bind:checked={ui.preferences.markdown_preview}/></label></fieldset>
      <fieldset><legend>Panel arrangement</legend><label class="switch-row"><span>Show file drawer</span><input type="checkbox" role="switch" bind:checked={ui.preferences.explorer_visible}/></label><label class="switch-row"><span>Show control bank</span><input type="checkbox" role="switch" bind:checked={ui.preferences.controls_visible}/></label><label>File drawer width <span>{ui.preferences.explorer_width}px</span><input type="number" min="0" step="any" bind:value={ui.preferences.explorer_width}/></label></fieldset>
      <fieldset><legend>Keyboard bindings</legend>{#each Object.keys(ui.preferences.shortcuts) as command}<label class="shortcut-setting"><span>{command}</span><input aria-label={`${command} shortcut`} bind:value={ui.preferences.shortcuts[command]}/></label>{/each}<p class="setting-note">Mod is Ctrl on Linux/Windows and Command on macOS. Add command IDs and aliases in the dotfile.</p></fieldset>
    </div>
  {:else}
    <div class="dotfile-toolbar"><code>{ui.configPath}</code><button class="flat" onclick={()=>fileInput.click()}>Import</button><button class="flat" onclick={download}>Export</button></div>
    <input bind:this={fileInput} type="file" accept=".toml,text/plain" hidden onchange={(event)=>{const input=event.currentTarget;void attempt(async()=>{try{await importConfig(input.files?.[0]);}finally{input.value='';}});}}/>
    <textarea class="dotfile-editor" aria-label="Console TOML configuration" value={ui.settings.source} oninput={(event)=>editSettingsSource(event.currentTarget.value)} spellcheck="false"></textarea>
    <details class="config-reference"><summary>Command aliases & color overrides</summary><pre>{'[aliases]\nship = "/git push"\ninspect = "/review"\n\n[tokens]\naccent = "#f2a260"\n\n[shortcuts]\ngit = "Mod+Shift+g"\ninspect = "Mod+Alt+r"'}</pre><p>Aliases resolve through the same dispatcher as buttons and slash commands. Color roles: background, panel, display, text, muted, accent, line. Invalid configuration is rejected with an explanation.</p></details>
  {/if}
  <div class="dialog-actions"><button class="flat" onclick={()=>void attempt(reset)}>Reset to defaults</button><button class="key primary" disabled={saving||held} onclick={()=>void attempt(save)}><Icon name="check"/>{saving?'Saving…':held?'Resolve pending save':'Save configuration'}</button></div>
</div>
