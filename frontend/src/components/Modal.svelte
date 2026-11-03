<!--
Shared modal dialog (title, body, footer buttons; Escape closes).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    title,
    onclose,
    children,
    footer,
    wide = false,
  }: { title: string; onclose: () => void; children: Snippet; footer?: Snippet; wide?: boolean } = $props();

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="scrim" onclick={onclose} role="presentation"></div>
<div class="modal" class:wide role="dialog" aria-modal="true" aria-label={title}>
  <header>
    <h2>{title}</h2>
    <button class="btn ghost icon" onclick={onclose} aria-label="Close"><Icon name="close" /></button>
  </header>
  <div class="content">{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(10, 6, 16, 0.6);
    z-index: 80;
  }
  .modal {
    position: fixed;
    z-index: 81;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(560px, calc(100vw - 2rem));
    max-height: calc(100vh - 2rem);
    display: grid;
    grid-template-rows: auto 1fr auto;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    box-shadow: var(--shadow);
  }
  .modal.wide {
    width: min(760px, calc(100vw - 2rem));
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1rem 0.5rem 1.4rem;
  }
  .content {
    min-width: 0;
    overflow-wrap: anywhere;
    overflow: auto;
    padding: 0.5rem 1.4rem 1.2rem;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.6rem;
    padding: 0.9rem 1.4rem;
    border-top: 1px solid var(--line-soft);
  }
  @media (max-width: 640px) {
    .modal,
    .modal.wide {
      top: auto;
      bottom: 0;
      left: 0;
      transform: none;
      width: 100%;
      max-height: 92vh;
      border-radius: var(--r-md) var(--r-md) 0 0;
    }
    footer {
      padding-bottom: calc(0.9rem + var(--safe-bottom));
    }
  }
</style>
