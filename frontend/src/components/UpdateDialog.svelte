<script lang="ts">
  import { openExternal, runJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let u = $derived(app.update);
  let progress = $state<{ percent: number; detail: string } | null>(null);
  let done = $state<string | null>(null);
  let opened = $state(false);

  /** The release notes are the changelog entry: keep the bullet lines. */
  let notes = $derived(
    (u?.notes ?? '')
      .split('\n---')[0]
      .split('\n')
      .map((l) => l.replace(/\*\*/g, '').replace(/`/g, ''))
      .filter((l) => l.trim() && !l.startsWith('## '))
      .slice(0, 30),
  );

  async function install() {
    if (!u) return;
    if (u.install === 'download') {
      if (u.download_url) {
        await openExternal(u.download_url);
        opened = true;
      }
      return;
    }
    progress = { percent: 0, detail: '' };
    try {
      const res = await runJob<{ message: string }>('update_install', {}, (j) => (progress = { percent: j.percent, detail: j.detail }), 500);
      done = res.message;
    } catch (err) {
      app.error(err);
      progress = null;
    }
  }
</script>

<Modal title={u?.available ? 'Update available' : 'Updates'} {onclose}>
  {#if !u}
    <p class="muted">Checking for updates…</p>
  {:else}
    <div class="stack">
      <p>
        {#if u.available}
          <b>Commander Lab {u.latest_version}</b> is out. You have {u.current_version}{u.current_build ? ` (build ${u.current_build})` : ''}.
        {:else if u.install === 'none'}
          {u.how}
        {:else}
          You have the latest version ({u.current_version}{u.current_build ? `, build ${u.current_build}` : ''}).
        {/if}
      </p>
      {#if u.available && notes.length}
        <div class="notes panel">
          <b class="small">What's new</b>
          <ul class="small">
            {#each notes as line, i (i)}<li class:sub={/^\s{2,}/.test(line)}>{line.replace(/^\s*[-*]\s*/, '')}</li>{/each}
          </ul>
        </div>
      {/if}
      {#if u.available}<p class="small dim">{u.how}</p>{/if}
      {#if progress && !done}
        <div class="progress"><i style:width="{progress.percent}%"></i></div>
        <p class="small dim">Downloading… {progress.detail}</p>
      {/if}
      {#if done}<p class="ok">{done}</p>{/if}
      {#if opened}
        <p class="small">
          The download has started. Open it when it finishes and tap <b>Install</b>; Android updates the app in place and keeps your decks.
          If Android asks, allow installs from your browser.
        </p>
      {/if}
    </div>
  {/if}
  {#snippet footer()}
    {#if u}
      <a class="btn ghost" href={u.page_url} target="_blank" rel="noreferrer" onclick={(e) => { e.preventDefault(); openExternal(u!.page_url); }}>
        Release page <Icon name="external" size={14} />
      </a>
    {/if}
    {#if u?.available && !done}
      <button class="btn primary" onclick={install} disabled={!!progress || (u.install === 'download' && !u.download_url)}>
        <Icon name="download" size={16} />
        {u.install === 'auto' ? (progress ? 'Updating…' : 'Update and restart') : 'Download update'}
      </button>
    {:else}
      <button class="btn" onclick={onclose}>Close</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .notes {
    padding: 0.75rem 1rem;
    max-height: 40vh;
    overflow: auto;
  }
  .notes ul {
    margin: 0.35rem 0 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.2rem;
  }
  .notes li.sub {
    margin-left: 1rem;
    list-style: circle;
  }
  .ok {
    color: var(--ok);
    font-weight: 700;
  }
</style>
