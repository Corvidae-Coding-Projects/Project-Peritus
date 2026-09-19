<script lang="ts">
  let { name, size = 18 }: {name: string; size?: number} = $props();
  const paths: Record<string,string> = {
    plus:'M12 5v14M5 12h14', close:'m6 6 12 12M6 18 18 6', chevron:'m9 5 7 7-7 7', down:'m5 9 7 7 7-7',
    folder:'M3 7V5h6l2 2h10v13H3Z', file:'M6 3h8l4 4v14H6ZM14 3v5h4', code:'m8 7-5 5 5 5m8-10 5 5-5 5m-3-13-2 16',
    image:'M3 3h18v18H3Zm0 13 5-5 5 5 3-3 5 5M15 7h.01', audio:'M9 18V5l11-2v13M9 8l11-2M9 18c0 3-6 4-6 1s6-4 6-1Zm11-2c0 3-6 4-6 1s6-4 6-1Z',
    git:'M6 3v12a4 4 0 0 0 4 4h5M18 8v3a4 4 0 0 1-4 4H6M6 3h.01M18 5h.01M18 19h.01',
    send:'m4 4 17 8-17 8 3-8Zm3 8h14', stop:'M6 6h12v12H6Z', search:'M16 16 21 21M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0',
    settings:'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM12 2v3m0 14v3M2 12h3m14 0h3M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2',
    terminal:'m4 6 6 6-6 6m9 0h7', chat:'M3 4h18v13H9l-6 4Z', branch:'M6 3v18M6 10h7a5 5 0 0 0 5-5V3',
    refresh:'M20 7a9 9 0 0 0-15-2L2 8m0-6v6h6M4 17a9 9 0 0 0 15 2l3-3m0 6v-6h-6',
    check:'m4 12 5 5L20 6', arrow:'M4 12h16m-6-6 6 6-6 6', download:'M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5',
    upload:'M12 16V4m-5 5 5-5 5 5M4 16v5h16v-5', more:'M5 12h.01M12 12h.01M19 12h.01',
    layers:'m3 7 9-4 9 4-9 4Zm0 5 9 4 9-4M3 17l9 4 9-4', link:'m9 15 6-6M7 14l-2 2a3 3 0 0 0 4 4l3-3M12 7l3-3a3 3 0 0 1 5 5l-3 3',
    clock:'M12 7v5l3 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0', bolt:'m13 2-9 12h7l-1 8 10-13h-7Z',
    help:'M9 8a3 3 0 1 1 5 2c-2 1-2 2-2 3m0 4h.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0',
    eye:'M2 12s4-7 10-7 10 7 10 7-4 7-10 7-10-7-10-7Zm13 0a3 3 0 1 1-6 0 3 3 0 0 1 6 0',
    shield:'m12 3 9 4v6c0 5-9 9-9 9S3 18 3 13V7Zm-4 9 3 3 5-5', panel:'M3 4h18v16H3ZM9 4v16',
  };
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name] ?? paths.file}/></svg>
