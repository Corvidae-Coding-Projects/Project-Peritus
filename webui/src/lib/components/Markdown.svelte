<script lang="ts">
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  let { text }: {text:string} = $props();
  const renderer = new marked.Renderer();
  // Project markdown cannot fetch tracking images or navigate the console through raw HTML.
  renderer.image = ({text}) => `<span>[Image: ${DOMPurify.sanitize(text,{ALLOWED_TAGS:[]})}]</span>`;
  let html = $derived(DOMPurify.sanitize(marked.parse(text,{async:false,renderer}) as string, {
    FORBID_TAGS:['style','iframe','form','input','button','img','video','audio','svg'],
    FORBID_ATTR:['style','id'], ALLOW_DATA_ATTR:false,
  }));
</script>
<div class="markdown">{@html html}</div>
