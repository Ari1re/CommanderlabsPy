<script lang="ts">
  import { onMount } from 'svelte';
  import { api, runJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Icon from '../components/Icon.svelte';

  let checking = $state(false);

  let { onUpdateCards }: { onUpdateCards: () => void } = $props();

  let ai = $state<any>(null);
  let apiKey = $state('');
  let starting = $state(false);

  onMount(async () => {
    await app.refreshStatus();
    // Opening Settings always asks GitHub, so a new build shows up right away.
    checking = true;
    app.checkUpdate(true, false).finally(() => (checking = false));
    try {
      ai = await api('ai_status');
    } catch (err) {
      app.error(err);
    }
  });

  async function setTheme(theme: typeof app.theme) {
    app.theme = theme;
    app.applyTheme();
    await app.saveSetting('theme', theme);
  }

  async function setCurrency(currency: typeof app.currency) {
    app.currency = currency;
    await app.saveSetting('currency', currency);
  }

  async function setAi(patch: Record<string, unknown>) {
    try {
      ai = await api('ai_config_set', patch);
      app.toast('AI settings saved.', 'success');
    } catch (err) {
      app.error(err);
    }
  }

  async function startAi() {
    starting = true;
    try {
      await runJob('ai_start', {}, undefined, 1500);
      ai = await api('ai_status');
    } catch (err) {
      app.error(err);
    } finally {
      starting = false;
    }
  }

  async function stopAi() {
    ai = await api('ai_stop');
  }
</script>

<h1 class="title">Settings</h1>

<div class="sections">
  <section>
    <h2>Appearance</h2>
    <div class="setting">
      <span>Theme</span>
      <div class="segmented" role="group" aria-label="Theme">
        <button aria-pressed={app.theme === 'dark'} onclick={() => setTheme('dark')}>Dark</button>
        <button aria-pressed={app.theme === 'light'} onclick={() => setTheme('light')}>Light</button>
        <button aria-pressed={app.theme === 'system'} onclick={() => setTheme('system')}>Match system</button>
      </div>
    </div>
    <div class="setting">
      <span>Prices</span>
      <div class="segmented" role="group" aria-label="Currency">
        <button aria-pressed={app.currency === 'usd'} onclick={() => setCurrency('usd')}>US dollars</button>
        <button aria-pressed={app.currency === 'eur'} onclick={() => setCurrency('eur')}>Euros</button>
        <button aria-pressed={app.currency === 'tix'} onclick={() => setCurrency('tix')}>MTGO tix</button>
      </div>
    </div>
  </section>

  <section>
    <h2>Updates {#if app.update?.available}<span class="badge brass">New</span>{/if}</h2>
    <p class="dim">
      {#if app.update?.available}
        Commander Lab {app.update.latest_version} is available. You have {app.update.current_version}{app.update.current_build ? ` (build ${app.update.current_build})` : ''}.
      {:else if app.update}
        {app.update.install === 'none' ? app.update.how : `You have the latest version (${app.update.current_version}${app.update.current_build ? `, build ${app.update.current_build}` : ''}).`}
      {:else if app.updateError}
        Couldn't check GitHub for updates: {app.updateError}
      {:else}
        Commander Lab {app.status?.version}{app.status?.build ? ` (build ${app.status.build})` : ''}. It checks GitHub for new builds when it starts.
      {/if}
    </p>
    <div class="row">
      <button class="btn" disabled={checking} onclick={async () => { checking = true; const u = await app.checkUpdate(true); checking = false; if (u && !u.available) app.toast(u.install === 'none' ? 'Development build: no automatic updates.' : 'You have the latest version.', 'success'); }}>
        <Icon name="refresh" size={16} /> {checking ? 'Checking…' : 'Check for updates'}
      </button>
      <a class="btn ghost" href="https://github.com/Ari1re/CommanderlabsPy/releases/latest" target="_blank" rel="noreferrer">
        Releases <Icon name="external" size={14} />
      </a>
      {#if app.update?.available}
        <button class="btn primary" onclick={() => (app.updateOpen = true)}><Icon name="download" size={16} /> Update now</button>
      {/if}
    </div>
  </section>

  <section>
    <h2>Card data</h2>
    <div class="setting">
      <div>
        <span>Scryfall card database</span>
        <p class="small muted">
          {app.status?.card_data_ready ? `${app.status.cards.toLocaleString()} cards. Update for new sets and today's prices.` : 'Not downloaded yet.'}
        </p>
      </div>
      {#if app.updatingCards}
        <div class="upd">
          <div class="progress"><i style:width="{app.updatingCards.percent}%"></i></div>
          <span class="small dim">{app.updatingCards.step}</span>
        </div>
      {:else}
        <button class="btn" onclick={onUpdateCards}><Icon name="refresh" size={16} /> Update card data</button>
      {/if}
    </div>
  </section>

  {#if ai}
    <section>
      <h2>AI model</h2>
      {#if !ai.available}
        <p class="dim">The local AI model isn't available on Android. Deck building, analysis and the agent's simulations still work.</p>
      {:else}
        <p class="small dim">Optional. Powers the AI Agent's written plans and chat. Everything else works without it.</p>
        <div class="setting">
          <span>Provider</span>
          <div class="segmented" role="group" aria-label="AI provider">
            <button aria-pressed={ai.config.provider === 'llama_cpp'} onclick={() => setAi({ provider: 'llama_cpp' })}>Built-in (llama.cpp)</button>
            <button aria-pressed={ai.config.provider === 'ollama'} onclick={() => setAi({ provider: 'ollama' })}>Ollama</button>
            <button aria-pressed={ai.config.provider === 'api'} onclick={() => setAi({ provider: 'api' })}>Online API</button>
          </div>
        </div>
        {#if ai.config.provider === 'api'}
          <div class="setting">
            <label class="field grow"><span>API address</span><input class="input" value={ai.config.api_base_url} onchange={(e) => setAi({ api_base_url: (e.target as HTMLInputElement).value })} /></label>
            <label class="field grow"><span>Model name</span><input class="input" value={ai.config.api_model} onchange={(e) => setAi({ api_model: (e.target as HTMLInputElement).value })} /></label>
          </div>
          <div class="setting">
            <label class="field grow"><span>API key {ai.config.has_api_key ? '(saved)' : ''}</span>
              <input class="input" type="password" bind:value={apiKey} placeholder={ai.config.has_api_key ? '••••••••' : 'Paste your key'} />
            </label>
            <button class="btn" onclick={() => { setAi({ api_key: apiKey }); apiKey = ''; }}>Save key</button>
          </div>
        {:else}
          {@const presets = ai.config.provider === 'ollama' ? ai.presets.ollama : ai.presets.llama_cpp}
          <div class="setting">
            <span>Model</span>
            <select class="input model" value={ai.config.model} onchange={(e) => setAi({ model: (e.target as HTMLSelectElement).value })}>
              {#each presets as p (p.model)}<option value={p.model}>{p.name} — {p.description}</option>{/each}
              {#if !presets.some((p: any) => p.model === ai.config.model)}<option value={ai.config.model}>{ai.config.model}</option>{/if}
            </select>
          </div>
        {/if}
        <div class="setting">
          <span>Research the web while optimizing</span>
          <input type="checkbox" class="switch" checked={ai.config.web_research} onchange={(e) => setAi({ web_research: (e.target as HTMLInputElement).checked })} />
        </div>
        <div class="setting">
          <span>{ai.running ? ai.status : 'Not running'}</span>
          {#if ai.running}
            <button class="btn" onclick={stopAi}><Icon name="stop" size={15} /> Stop</button>
          {:else}
            <button class="btn" onclick={startAi} disabled={starting}><Icon name="play" size={15} /> {starting ? 'Starting…' : 'Start now'}</button>
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  <section>
    <h2>About</h2>
    <p class="small dim">
      Commander Lab {app.status?.version}. Your decks, card data and exports are stored in
      <b>{app.status?.data_dir}</b>.
    </p>
  </section>
</div>

<style>
  .title {
    margin-bottom: 1.5rem;
  }
  .sections {
    display: grid;
    gap: 2.25rem;
    max-width: 52rem;
  }
  section {
    display: grid;
    gap: 0.75rem;
  }
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    padding: 0.75rem 0;
    border-bottom: 1px solid var(--line-soft);
  }
  .grow {
    flex: 1 1 240px;
  }
  .model {
    width: auto;
    max-width: 100%;
  }
  .upd {
    display: grid;
    gap: 0.3rem;
    min-width: 220px;
  }
  .switch {
    accent-color: var(--brass);
    width: 20px;
    height: 20px;
  }
</style>
