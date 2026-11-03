<!--
Installs an app update from GitHub (per platform, including the Android
APK install permission flow).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invokeNative, openExternal, runJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Modal from './Modal.svelte';
  import Icon from './Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let u = $derived(app.update);
  let progress = $state<{ percent: number; detail: string } | null>(null);
  let done = $state<string | null>(null);
  let opened = $state(false);
  /** Android: the downloaded APK, once it's here. */
  let apkPath = $state<string | null>(null);
  /** Android: waiting for the user to allow installing apps. */
  let needsPermission = $state(false);
  let installerOpen = $state(false);

  let label = $derived(
    u?.kind === 'android'
      ? apkPath
        ? 'Install'
        : 'Download and install'
      : u?.kind === 'source'
        ? 'Update and rebuild'
        : u?.install === 'auto'
          ? 'Update and restart'
          : 'Download update',
  );

  onMount(() => {
    // Back from Android's "install unknown apps" screen: carry on.
    const onVisible = () => {
      if (document.visibilityState === 'visible' && needsPermission && apkPath) installApk();
    };
    document.addEventListener('visibilitychange', onVisible);
    return () => document.removeEventListener('visibilitychange', onVisible);
  });

  async function installApk() {
    if (!apkPath) return;
    try {
      const res = await invokeNative<{ status: string }>('install_apk', { path: apkPath });
      needsPermission = res.status === 'permission';
      installerOpen = res.status === 'started';
    } catch (err) {
      app.error(err);
    }
  }

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
    if (apkPath) return installApk();
    if (u.install === 'download') {
      if (u.download_url) {
        await openExternal(u.download_url);
        opened = true;
      }
      return;
    }
    progress = { percent: 0, detail: '' };
    try {
      const res = await runJob<{ message: string; apk?: string; restarting: boolean }>(
        'update_install',
        {},
        (j) => (progress = { percent: j.percent, detail: j.detail }),
        500,
      );
      if (res.apk) {
        apkPath = res.apk;
        progress = null;
        await installApk();
      } else {
        done = res.message;
      }
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
        <p class="small dim">
          {u.kind === 'source' ? 'Updating… (rebuilding takes a few minutes)' : 'Downloading…'} {progress.detail}
        </p>
      {/if}
      {#if needsPermission}
        <p class="small">
          Android needs your OK once: turn on <b>Allow from this source</b> for Commander Lab on the screen that opened, then
          come back. The installer opens by itself, or tap <b>Install</b>.
        </p>
      {:else if installerOpen}
        <p class="small">
          Android's installer is open. Tap <b>Install</b> (or <b>Update</b>); Commander Lab closes, updates and keeps your decks.
          Didn't see it? Tap <b>Install</b> again.
        </p>
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
        {progress ? 'Updating…' : label}
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
