<script lang="ts">
  import { manaSymbols, symbolUrl } from '../lib/util';
  let { cost, size = 15 }: { cost: string; size?: number } = $props();
  let symbols = $derived(manaSymbols(cost));
</script>

{#if symbols.length}
  <span class="mana" aria-label={cost}>
    {#each symbols as sym, i (i)}
      <img src={symbolUrl(sym)} alt="" width={size} height={size} loading="lazy" />
    {/each}
  </span>
{/if}

<style>
  .mana {
    display: inline-flex;
    gap: 1px;
    vertical-align: -2px;
    flex-shrink: 0;
  }
  img {
    display: block;
    border-radius: 50%;
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.35);
  }
</style>
