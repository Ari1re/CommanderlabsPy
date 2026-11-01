<script lang="ts">
  import { api, runJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Modal from './Modal.svelte';
  import QrScanner from './QrScanner.svelte';

  let { onclose, ondone }: { onclose: () => void; ondone: (name: string) => void } = $props();

  let mode = $state<'paste' | 'link' | 'precon' | 'share'>('paste');
  let name = $state('');
  let text = $state('');
  let url = $state('');
  let code = $state('');
  let scanning = $state(false);
  let preview = $state<{ name: string; commanders: string[]; cards: number } | null>(null);
  let busy = $state(false);

  interface Precon {
    file: string;
    name: string;
    set_code: string;
    release_date: string;
  }
  let precons = $state<Precon[] | null>(null);
  let preconError = $state<string | null>(null);
  let preconQuery = $state('');
  let precon = $state<Precon | null>(null);

  async function choosePrecons() {
    mode = 'precon';
    if (precons) return;
    try {
      precons = await api<Precon[]>('precons');
    } catch (err) {
      preconError = (err as Error).message;
    }
  }

  let preconMatches = $derived.by(() => {
    const q = preconQuery.trim().toLowerCase();
    const all = precons ?? [];
    return (q ? all.filter((p) => `${p.name} ${p.set_code} ${p.release_date.slice(0, 4)}`.toLowerCase().includes(q)) : all).slice(0, 60);
  });

  let canSave = $derived(
    mode === 'paste' ? name.trim() && text.trim() : mode === 'link' ? url.trim() : mode === 'precon' ? !!precon : !!preview,
  );

  async function checkCode(value: string) {
    code = value;
    preview = null;
    if (!/CLD[12][.:]/.test(value)) return;
    try {
      preview = await api('share_preview', { code: value });
      scanning = false;
    } catch (err) {
      app.error(err);
    }
  }

  async function save() {
    busy = true;
    try {
      let res: { name: string };
      if (mode === 'paste') {
        res = await api('save_deck_text', { name, text });
      } else if (mode === 'link') {
        res = await runJob('import_url', { url, name: name || null });
      } else if (mode === 'precon') {
        res = await runJob('import_precon', { file: precon!.file, name: name || null });
      } else {
        res = await api('import_share', { code, name: name || null });
      }
      app.toast(`Added ${res.name}.`, 'success');
      ondone(res.name);
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
      <button aria-pressed={mode === 'link'} onclick={() => (mode = 'link')}>Deck link</button>
      <button aria-pressed={mode === 'precon'} onclick={choosePrecons}>Precon</button>
      <button aria-pressed={mode === 'share'} onclick={() => (mode = 'share')}>Share code</button>
    </div>

    {#if mode === 'paste'}
      <label class="field">
        <span>Deck name</span>
        <input class="input" bind:value={name} placeholder="Kefka spellslinger" />
      </label>
      <label class="field">
        <span>Decklist</span>
        <textarea class="input list" rows="12" bind:value={text} placeholder={'Commander\n1 Kefka, Court Mage\n\nDeck\n1 Sol Ring (C21) 263\n1 Arcane Signet\n…'}></textarea>
      </label>
      <p class="small muted">
        Any common format works, including Moxfield and Archidekt exports. Set codes, collector numbers and *F* foil
        markers are kept. Put the commander under a “Commander” heading, or mark it with *CMDR*.
      </p>
    {:else if mode === 'link'}
      <label class="field">
        <span>Moxfield or Archidekt link</span>
        <input class="input" bind:value={url} placeholder="https://www.moxfield.com/decks/…" />
      </label>
      <label class="field">
        <span>Deck name <span class="muted">(optional)</span></span>
        <input class="input" bind:value={name} placeholder="Uses the deck's own name" />
      </label>
      <p class="small muted">If Moxfield refuses the request, export the deck as text in Moxfield and paste it instead.</p>
    {:else if mode === 'precon'}
      {#if preconError}
        <p class="small err">Couldn't load the precon list: {preconError}</p>
      {:else if !precons}
        <p class="small dim">Loading every Commander precon…</p>
      {:else}
        <label class="field">
          <span>Find a precon</span>
          <input class="input" bind:value={preconQuery} placeholder="Name, set code or year" />
        </label>
        <ul class="precons" role="listbox" aria-label="Commander precons">
          {#each preconMatches as p (p.file)}
            <li>
              <button role="option" aria-selected={precon?.file === p.file} class:chosen={precon?.file === p.file} onclick={() => (precon = p)}>
                <b>{p.name}</b>
                <span class="small dim">{p.set_code} · {p.release_date.slice(0, 4)}</span>
              </button>
            </li>
          {:else}
            <li class="small muted">No precon matches.</li>
          {/each}
        </ul>
        <label class="field">
          <span>Save as <span class="muted">(optional)</span></span>
          <input class="input" bind:value={name} placeholder={precon?.name ?? 'Uses the precon\'s name'} />
        </label>
        <p class="small muted">Decklists come from MTGJSON, with each card's printing from the precon.</p>
      {/if}
    {:else}
      {#if scanning}
        <QrScanner onfound={checkCode} />
      {:else}
        <button class="btn" onclick={() => (scanning = true)}>Scan a QR code</button>
      {/if}
      <label class="field">
        <span>Or paste a share code</span>
        <textarea class="input code" rows="3" value={code} oninput={(e) => checkCode((e.target as HTMLTextAreaElement).value)} placeholder="CLD2.…"></textarea>
      </label>
      {#if preview}
        <div class="preview panel">
          <b>{preview.name}</b>
          <span class="small dim">{preview.commanders.join(' + ') || 'No commander'} · {preview.cards} cards</span>
        </div>
        <label class="field">
          <span>Save as <span class="muted">(optional)</span></span>
          <input class="input" bind:value={name} placeholder={preview.name} />
        </label>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <button class="btn ghost" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!canSave || busy} onclick={save}>
      {busy ? 'Adding…' : mode === 'link' || mode === 'precon' ? 'Import deck' : 'Add deck'}
    </button>
  {/snippet}
</Modal>

<style>
  .precons {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 18rem;
    overflow: auto;
    display: grid;
    gap: 0.25rem;
  }
  .precons button {
    width: 100%;
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-sm);
    background: var(--panel);
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .precons button.chosen {
    border-color: var(--brass);
  }
  .err {
    color: var(--danger);
  }
  .list {
    font-size: 0.9rem;
  }
  .code {
    font-size: 0.8rem;
    word-break: break-all;
  }
  .preview {
    display: grid;
    gap: 0.15rem;
    padding: 0.75rem 1rem;
    border-color: var(--brass);
  }
</style>
