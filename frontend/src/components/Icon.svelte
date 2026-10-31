<script lang="ts">
  // Small stroke icon set; 24px grid.
  let { name, size = 20 }: { name: string; size?: number } = $props();

  const paths: Record<string, string> = {
    library: 'M4 5h4v14H4zM10 5h4v14h-4zM16.5 5.5l3.8 1-3.6 13.5-3.8-1z',
    build: 'M12 3l2.2 5.3L20 9l-4.4 3.8L17 18.5 12 15.6 7 18.5l1.4-5.7L4 9l5.8-.7z',
    settings:
      'M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM19.4 13a7.5 7.5 0 0 0 0-2l2-1.6-2-3.4-2.4 1a7.6 7.6 0 0 0-1.7-1L15 3.5h-4l-.4 2.5a7.6 7.6 0 0 0-1.7 1l-2.4-1-2 3.4L4.6 11a7.5 7.5 0 0 0 0 2l-2 1.6 2 3.4 2.4-1a7.6 7.6 0 0 0 1.7 1l.4 2.5h4l.4-2.5a7.6 7.6 0 0 0 1.7-1l2.4 1 2-3.4z',
    plus: 'M12 5v14M5 12h14',
    minus: 'M5 12h14',
    close: 'M6 6l12 12M18 6L6 18',
    back: 'M15 5l-7 7 7 7',
    search: 'M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM20 20l-4.8-4.8',
    copy: 'M9 9h10v10H9zM5 15V5h10',
    download: 'M12 4v11M7 10l5 5 5-5M5 20h14',
    trash: 'M5 7h14M10 7V4h4v3M7 7l1 13h8l1-13',
    refresh: 'M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6',
    external: 'M14 4h6v6M20 4l-9 9M18 14v6H4V6h6',
    more: 'M5 12h.01M12 12h.01M19 12h.01',
    edit: 'M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4',
    sparkle: 'M12 3v4M12 17v4M3 12h4M17 12h4M6 6l2.5 2.5M15.5 15.5L18 18M18 6l-2.5 2.5M8.5 15.5L6 18',
    combo: 'M8 8a4 4 0 1 0 0 8M16 8a4 4 0 1 1 0 8M8 12h8',
    print: 'M7 9V4h10v5M7 17H5v-6h14v6h-2M7 14h10v6H7z',
    link: 'M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1',
    chat: 'M4 5h16v11H9l-5 4z',
    chart: 'M5 20V10M12 20V4M19 20v-7',
    cards: 'M8 4h11v15H8zM5 7v13h11',
    swap: 'M7 7h12l-3-3M17 17H5l3 3',
    check: 'M5 12l5 5 9-10',
    sideboard: 'M4 6h10M4 12h10M4 18h6M18 9v10M15 16l3 3 3-3',
    crown: 'M4 18h16l1-10-5 4-4-7-4 7-5-4z',
    play: 'M8 5l11 7-11 7z',
    stop: 'M7 7h10v10H7z',
    life: 'M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={name === 'more' ? 3.4 : 1.7}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name] ?? ''} />
</svg>
