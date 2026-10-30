<script lang="ts">
  import { onMount } from 'svelte';
  import { api, copyText } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';

  let { name, onclose }: { name: string; onclose: () => void } = $props();

  let share = $state<{ code: string; svg: string | null; error: string | null } | null>(null);

  onMount(async () => {
    try {
      share = await api('share', { name });
    } catch (err) {
      app.error(err);
      onclose();
    }
  });

  async function copy() {
    if (share && (await copyText(share.code))) app.toast('Share code copied.', 'success');
  }

  async function nativeShare() {
    if (!share) return;
    try {
      await navigator.share({ title: name, text: `${name} — open in Commander Lab: ${share.code}` });
    } catch {
      /* cancelled */
    }
  }
</script>

<Modal title="Share {name}" {onclose}>
  {#if !share}
    <p class="muted">Making a share code…</p>
  {:else}
    <div class="stack">
      {#if share.svg}
        <div class="qr">{@html share.svg}</div>
        <p class="dim small center">
          On the other device, open Commander Lab → <b>Add a deck</b> → <b>Scan or paste a share code</b> and point the camera here.
        </p>
      {:else}
        <p class="dim">{share.error}</p>
      {/if}
      <label class="field">
        <span>Share code</span>
        <textarea class="input code" rows="3" readonly value={share.code}></textarea>
      </label>
      <p class="small muted">The code holds the whole list, printings included. Send it in any chat; it doesn't need an account or server.</p>
    </div>
  {/if}
  {#snippet footer()}
    {#if typeof navigator !== 'undefined' && 'share' in navigator}
      <button class="btn" onclick={nativeShare}>Share…</button>
    {/if}
    <button class="btn primary" onclick={copy} disabled={!share}><Icon name="copy" size={16} /> Copy code</button>
  {/snippet}
</Modal>

<style>
  .qr {
    width: min(320px, 100%);
    margin: 0 auto;
    padding: 10px;
    background: #fff;
    border-radius: var(--r-md);
  }
  .qr :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }
  .center {
    text-align: center;
  }
  .code {
    font-size: 0.8rem;
    word-break: break-all;
  }
</style>
