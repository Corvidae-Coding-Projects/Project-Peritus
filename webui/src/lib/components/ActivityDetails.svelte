<script lang="ts">
  import TextBody from './TextBody.svelte';
  let {text}:{text:string}=$props();
  let open=$state(false);
</script>
<details ontoggle={event=>open=event.currentTarget.open}>
  <summary>Details</summary>
  {#if open}
    <TextBody {text} label="Activity details"/>
  {/if}
</details>
