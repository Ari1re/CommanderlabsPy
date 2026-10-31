<script lang="ts">
  import { onMount } from 'svelte';
  import { api, copyText } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';

  let { name, onclose }: { name: string; onclose: () => void } = $props();

  let share = $state<{ code: string; svg: string | null; error: string | null; has_extras: boolean } | null>(null);
  let extras = $state(false);

  async function load() {
    try {
      share = await api('share', { name, extras });
    } catch (err) {
      app.error(err);
      onclose();
    }
  }

  onMount(load);

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
      {#if share.has_extras}
        <label class="check">
          <input type="checkbox" bind:checked={extras} onchange={load} />
          Include printings and sideboard <span class="muted small">(bigger code)</span>
        </label>
      {/if}
      <label class="field">
        <span>Share code</span>
        <textarea class="input code" rows="3" readonly value={share.code}></textarea>
      </label>
      <p class="small muted">
        {extras ? 'Includes the commander, every card, printings and the sideboard.' : 'Just what rebuilds the deck: the commander and every card with its count.'}
        Send the code in any chat; no account or server needed.
      </p>
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
    width: min(260px, 72vw);
    margin: 0 auto;
    padding: 10px;
    background: #fff;
    border-radius: var(--r-md);
  }
  .qr :global(svg) {
    display: block;
    width: 100%;
    height: auto;
    aspect-ratio: 1;
  }
  .center {
    text-align: center;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 40px;
    cursor: pointer;
  }
  .check input {
    accent-color: var(--brass);
    width: 20px;
    height: 20px;
  }
  .code {
    width: 100%;
    max-width: 100%;
    font-size: 0.8rem;
    word-break: break-all;
    overflow-wrap: anywhere;
  }
</style>
