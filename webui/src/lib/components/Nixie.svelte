<script lang="ts">
  let { value, label, digits = 2, large = false }: {value:number;label:string;digits?:number;large?:boolean} = $props();
  let text = $derived(Math.max(0,value).toString().padStart(digits,'0'));
</script>
<div class:large class="instrument" aria-label={`${label}: ${value}`}>
  <div class="tube-bank" aria-hidden="true">
    {#each text.split('') as number, index}
      <div class="tube"><div class="anode"></div><div class="cathodes">
        {#each '0123456789'.split('') as layer}<span class:lit={layer === number} style={`--depth:${Number(layer)}`}>{layer}</span>{/each}
      </div><i class="glass-shine"></i><i class="tube-base"></i></div>
    {/each}
  </div>
  <span class="instrument-label">{label}</span>
</div>
