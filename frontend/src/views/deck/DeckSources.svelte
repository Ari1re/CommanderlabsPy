<script lang="ts">
  import { onMount } from 'svelte';
  import { api, runJob } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import type { Source } from '../../lib/types';
  import Icon from '../../components/Icon.svelte';

  let { name }: { name: string } = $props();

  let sources = $state<Source[] | null>(null);
  let url = $state('');
  let busy = $state(false);

  onMount(async () => {
    try {
      sources = await api<Source[]>('sources', { name });
    } catch (err) {
      app.error(err);
      sources = [];
    }
  });

  async function add() {
    busy = true;
    try {
      sources = await runJob<Source[]>('add_reference', { name, url });
      url = '';
      app.toast('Reference added. The AI Agent and upgrades will use it.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      busy = false;
    }
  }

  async function remove(s: Source) {
    try {
      sources = await api<Source[]>('remove_reference', { name, url: s.url });
    } catch (err) {
      app.error(err);
    }
  }

  let mine = $derived((sources ?? []).filter((s) => s.user_provided));
  let auto = $derived((sources ?? []).filter((s) => !s.user_provided));
</script>

<div class="stack wrap">
  <section class="stack">
    <div>
      <h2>Your references</h2>
      <p class="dim small">
        Add decks or articles you want this deck to learn from. A Moxfield or Archidekt link is imported as a full
        list; other pages are read for strategy. The AI Agent and upgrades weigh these above automatic research.
      </p>
    </div>
    <form class="row" onsubmit={(e) => { e.preventDefault(); add(); }}>
      <input class="input grow" bind:value={url} placeholder="https://www.moxfield.com/decks/…" aria-label="Reference link" />
      <button class="btn primary" disabled={!url.trim() || busy}><Icon name="plus" size={16} /> {busy ? 'Adding…' : 'Add reference'}</button>
    </form>
    {#if mine.length}
      <ul class="sources">
        {#each mine as s (s.url)}
          <li>
            <div>
              <a href={s.url} target="_blank" rel="noreferrer">{s.title || s.url}</a>
              <p class="small muted">{s.is_deck ? 'Reference deck' : 'Reference page'} · {s.detail}</p>
            </div>
            <button class="btn ghost small icon" aria-label="Remove reference" onclick={() => remove(s)}><Icon name="trash" size={15} /></button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="stack">
    <h2>Research sources</h2>
    {#if sources === null}
      <p class="muted">Loading…</p>
    {:else}
      <ul class="sources">
        {#each auto as s (s.url + s.title)}
          <li>
            <div>
              <a href={s.url} target="_blank" rel="noreferrer">{s.title}</a>
              <p class="small muted">{s.source}{s.detail ? ` · ${s.detail}` : ''}</p>
            </div>
            <Icon name="external" size={15} />
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .wrap {
    max-width: 56rem;
    gap: 2rem;
  }
  .grow {
    flex: 1 1 300px;
    width: auto;
  }
  .sources {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
  }
  .sources li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.65rem 0;
    border-bottom: 1px solid var(--line-soft);
    color: var(--muted);
  }
  .sources a {
    color: var(--text);
    text-decoration: none;
    font-weight: 700;
  }
  .sources a:hover {
    color: var(--brass-strong);
    text-decoration: underline;
  }
</style>
