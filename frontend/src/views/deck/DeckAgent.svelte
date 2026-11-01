<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, runJob, type Job } from '../../lib/api';
  import { app } from '../../lib/store.svelte';
  import { BRACKETS } from '../../lib/util';
  import Icon from '../../components/Icon.svelte';

  let { name, bracket, onsaved }: { name: string; bracket: number; onsaved: (name: string) => void } = $props();

  interface ChatMessage {
    role: 'user' | 'assistant';
    content: string;
  }

  let ai = $state<any>(null);
  let chat = $state<ChatMessage[]>([]);
  let agent = $state<any>(null);
  let brief = $state('');
  let goal = $state<'fast' | 'stax'>('fast');
  let target = $state(3);
  let intensity = $state<'quick' | 'standard' | 'thorough' | 'exhaustive'>('standard');
  let job = $state<Job | null>(null);
  let starting = $state(false);
  let message = $state('');
  let thinking = $state(false);
  let chatBox: HTMLDivElement | undefined = $state();

  onMount(async () => {
    target = bracket;
    try {
      const [status, state] = await Promise.all([api('ai_status'), api('agent_state', { name })]);
      ai = status;
      chat = state.chat ?? [];
      agent = state.agent;
      brief = state.brief ?? '';
      if (agent?.target_bracket) target = agent.target_bracket;
    } catch (err) {
      app.error(err);
    }
    scrollChat();
  });

  async function scrollChat() {
    await tick();
    chatBox?.scrollTo({ top: chatBox.scrollHeight });
  }

  async function startAi() {
    starting = true;
    try {
      await runJob('ai_start', {}, undefined, 1500);
      ai = await api('ai_status');
      app.toast('AI model is running.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      starting = false;
    }
  }

  async function run() {
    job = { id: 0, kind: 'agent', subject: name, status: 'running', percent: 0, step: 'Starting', detail: '', log: [], result: null, error: null };
    try {
      agent = await runJob('agent_run', { name, bracket: target, goal, intensity, brief }, (j) => (job = j), 1000);
      app.toast('The agent finished testing.', 'success');
    } catch (err) {
      app.error(err);
    } finally {
      job = null;
    }
  }

  async function send() {
    const text = message.trim();
    if (!text) return;
    chat = [...chat, { role: 'user', content: text }];
    message = '';
    thinking = true;
    scrollChat();
    try {
      const res = await runJob<{ chat: ChatMessage[] }>('agent_chat', { name, message: text });
      chat = res.chat;
    } catch (err) {
      app.error(err);
      chat = chat.slice(0, -1);
      message = text;
    } finally {
      thinking = false;
      scrollChat();
    }
  }

  async function clearChat() {
    chat = (await api<{ chat: ChatMessage[] }>('agent_chat_clear', { name })).chat;
  }

  async function save(iteration?: number) {
    try {
      const res = await api<{ name: string }>('agent_save', { name, iteration });
      app.toast(`Saved as ${res.name}.`, 'success');
      onsaved(res.name);
    } catch (err) {
      app.error(err);
    }
  }

  function turn(n: number | null | undefined) {
    return n ? `turn ${n}` : '—';
  }

  let iterations = $derived<any[]>(agent?.top_iterations?.length ? agent.top_iterations : agent ? [{ round: 0, detected_bracket: agent.target_bracket, objective_score: null, deck: agent.best, testing: agent.final_testing, best: true }] : []);
</script>

<div class="layout">
  <section class="stack">
    <div>
      <h2>Optimize with the AI Agent</h2>
      <p class="dim small">
        Describe what you want. The agent researches the commander and reference decks, rebuilds candidate lists,
        goldfishes each one, and ranks them. Your deck isn't changed until you save a result.
      </p>
    </div>

    {#if ai && !ai.available}
      <p class="note small">The local AI model isn't available on Android. The agent still runs its research and simulations without it.</p>
    {:else if ai && !ai.running}
      <div class="note small row">
        <span>The AI model isn't running. The agent works without it, but writes better plans with it.</span>
        <button class="btn small" onclick={startAi} disabled={starting}>{starting ? 'Starting… (first start downloads the model)' : 'Start AI model'}</button>
      </div>
    {/if}

    <label class="field">
      <span>What should the deck do better?</span>
      <textarea class="input" rows="4" bind:value={brief} placeholder="Win faster without relying on the commander. Keep the graveyard package. Use this list as a reference: https://www.moxfield.com/decks/…"></textarea>
    </label>

    <div class="opts">
      <label class="field"><span>Goal</span>
        <select class="input" bind:value={goal}>
          <option value="fast">Win as fast as possible</option>
          <option value="stax">Lock the table (stax)</option>
        </select>
      </label>
      <label class="field"><span>Bracket</span>
        <select class="input" bind:value={target}>
          {#each BRACKETS as b (b.n)}<option value={b.n}>{b.n} · {b.name}</option>{/each}
        </select>
      </label>
      <label class="field"><span>Testing depth</span>
        <select class="input" bind:value={intensity}>
          <option value="quick">Quick (1 round)</option>
          <option value="standard">Standard (3 rounds)</option>
          <option value="thorough">Thorough (5 rounds)</option>
          <option value="exhaustive">Exhaustive (8 rounds)</option>
        </select>
      </label>
    </div>

    {#if job}
      <div class="running panel">
        <div class="row"><b>{job.step}</b><span class="spacer"></span><span class="num small">{job.percent}%</span></div>
        <div class="progress"><i style:width="{job.percent}%"></i></div>
        <p class="small dim">{job.detail}</p>
        <ol class="log small">{#each job.log.slice(-6) as l, i (i)}<li>{l}</li>{/each}</ol>
      </div>
    {:else}
      <div><button class="btn primary" onclick={run}><Icon name="sparkle" size={17} /> Run the agent</button></div>
    {/if}

    {#if agent}
      <h3>Results</h3>
      <p class="small dim">Ranked by the agent's objective score at Bracket {agent.target_bracket}.</p>
      <ol class="results">
        {#each iterations as it, i (i)}
          {@const g = it.testing?.goldfish}
          <li class="panel res">
            <div class="row">
              <b>{i === 0 ? 'Best version' : `Round ${it.round}`}</b>
              <span class="badge">Bracket {it.detected_bracket}</span>
              <span class="spacer"></span>
              <button class="btn small" onclick={() => save(it.best ? undefined : i)}>Save as new deck</button>
            </div>
            {#if g}
              <div class="stats small">
                <span>Most likely win: <b>{turn(g.most_likely_combo_turn)}</b></span>
                <span>Median win: <b>{turn(g.median_combo_turn)}</b></span>
                <span>Keepable hands: <b class="num">{Math.round(g.keepable_pct)}%</b></span>
                {#if it.stax?.median_lock_turn}<span>Median lock: <b>{turn(it.stax.median_lock_turn)}</b></span>{/if}
              </div>
            {/if}
          </li>
        {/each}
      </ol>
      {#if agent.rounds?.length}
        <details>
          <summary class="small dim">What the agent tried ({agent.rounds.length} rounds)</summary>
          <ul class="rounds small">
            {#each agent.rounds as r (r.round)}
              <li>
                <b>Round {r.round}</b> {r.accepted ? '· kept' : '· rejected'} — {r.summary}
                {#if r.added_cards?.length}<div class="add">+ {r.added_cards.join(', ')}</div>{/if}
                {#if r.removed_cards?.length}<div class="rem">− {r.removed_cards.join(', ')}</div>{/if}
              </li>
            {/each}
          </ul>
        </details>
      {/if}
    {/if}
  </section>

  <section class="chat panel">
    <div class="row chat-head">
      <h3>Ask about this deck</h3>
      <span class="spacer"></span>
      {#if chat.length}<button class="btn ghost small" onclick={clearChat}>Clear</button>{/if}
    </div>
    <div class="messages" bind:this={chatBox}>
      {#if !chat.length}
        <p class="small muted">
          Ask how to pilot it, what to cut, or why a card is in. Your chat direction is included the next time you run the agent.
        </p>
      {/if}
      {#each chat as m, i (i)}
        <div class="msg {m.role}">{m.content}</div>
      {/each}
      {#if thinking}<div class="msg assistant muted">Thinking…</div>{/if}
    </div>
    <form class="composer" onsubmit={(e) => { e.preventDefault(); send(); }}>
      <textarea
        class="input"
        rows="2"
        bind:value={message}
        placeholder="How should I mulligan this deck?"
        onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); send(); } }}
      ></textarea>
      <button class="btn primary icon" aria-label="Send" disabled={thinking || !message.trim()}><Icon name="play" size={16} /></button>
    </form>
  </section>
</div>

<style>
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(300px, 420px);
    gap: 2rem;
    align-items: start;
  }
  .note {
    padding: 0.7rem 0.9rem;
    border-radius: var(--r-sm);
    border: 1px dashed var(--line);
    color: var(--text-2);
  }
  .opts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 0.75rem;
  }
  .running {
    display: grid;
    gap: 0.5rem;
    padding: 1rem;
  }
  .log {
    margin: 0;
    padding-left: 1.1rem;
    color: var(--muted);
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }
  .res {
    padding: 0.8rem 1rem;
    display: grid;
    gap: 0.4rem;
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem 1.2rem;
    color: var(--text-2);
  }
  .rounds {
    margin: 0.5rem 0 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.4rem;
  }
  .add {
    color: var(--ok);
  }
  .rem {
    color: var(--danger);
  }
  .chat {
    position: sticky;
    top: 4.5rem;
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: min(70vh, 640px);
    overflow: hidden;
  }
  .chat-head {
    padding: 0.8rem 1rem 0.4rem;
  }
  .messages {
    overflow: auto;
    padding: 0.5rem 1rem;
    display: grid;
    gap: 0.6rem;
    align-content: start;
  }
  .msg {
    padding: 0.55rem 0.8rem;
    border-radius: var(--r-md);
    white-space: pre-wrap;
    max-width: 92%;
    font-size: 0.93rem;
  }
  .msg.user {
    justify-self: end;
    background: color-mix(in srgb, var(--brass) 18%, var(--panel));
  }
  .msg.assistant {
    background: var(--panel-2);
  }
  .composer {
    display: flex;
    gap: 0.5rem;
    padding: 0.75rem;
    border-top: 1px solid var(--line-soft);
    align-items: end;
  }
  .composer textarea {
    resize: none;
  }
  @media (max-width: 1000px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
    .chat {
      position: static;
      height: 70vh;
    }
  }
</style>
