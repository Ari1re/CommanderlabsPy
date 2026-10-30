<script lang="ts">
  import { api, runJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Modal from './Modal.svelte';

  let { onclose, ondone }: { onclose: () => void; ondone: (name: string) => void } = $props();

  let mode = $state<'paste' | 'link'>('paste');
  let name = $state('');
  let text = $state('');
  let url = $state('');
  let busy = $state(false);

  let canSave = $derived(mode === 'paste' ? name.trim() && text.trim() : url.trim());

  async function save() {
    busy = true;
    try {
      if (mode === 'paste') {
        const res = await api<{ name: string }>('save_deck_text', { name, text });
        app.toast(`Saved ${res.name}.`, 'success');
        ondone(res.name);
      } else {
        const res = await runJob<{ name: string }>('import_url', { url, name: name || null });
        app.toast(`Imported ${res.name}.`, 'success');
        ondone(res.name);
      }
    } catch (err) {
      app.error(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Add a deck" {onclose}>
  <div class="stack">
    <div class="segmented" role="group" aria-label="How to add">
      <button aria-pressed={mode === 'paste'} onclick={() => (mode = 'paste')}>Paste a list</button>
      <button aria-pressed={mode === 'link'} onclick={() => (mode = 'link')}>Moxfield or Archidekt link</button>
    </div>

    {#if mode === 'paste'}
      <label class="field">
        <span>Deck name</span>
        <input class="input" bind:value={name} placeholder="Kefka spellslinger" />
      </label>
      <label class="field">
        <span>Decklist</span>
        <textarea
          class="input list"
          rows="12"
          bind:value={text}
          placeholder={'Commander\n1 Kefka, Court Mage\n\nDeck\n1 Sol Ring\n1 Arcane Signet\n…'}
        ></textarea>
      </label>
      <p class="small muted">
        Any common format works, including Moxfield and Archidekt exports with set codes and foil
        markers. Put the commander under a “Commander” heading, or mark it with *CMDR*.
      </p>
    {:else}
      <label class="field">
        <span>Deck link</span>
        <input class="input" bind:value={url} placeholder="https://www.moxfield.com/decks/…" />
      </label>
      <label class="field">
        <span>Deck name <span class="muted">(optional)</span></span>
        <input class="input" bind:value={name} placeholder="Uses the deck's own name" />
      </label>
      <p class="small muted">
        If Moxfield refuses the request, export the deck as text in Moxfield and paste it instead.
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!canSave || busy} onclick={save}>
      {busy ? 'Saving…' : mode === 'paste' ? 'Save deck' : 'Import deck'}
    </button>
  {/snippet}
</Modal>

<style>
  .list {
    font-size: 0.9rem;
  }
</style>
