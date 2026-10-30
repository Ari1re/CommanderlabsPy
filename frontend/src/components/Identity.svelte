<script lang="ts">
  import { MANA_NAME } from '../lib/util';
  let { identity, size = 12 }: { identity: string; size?: number } = $props();
  let colors = $derived(identity ? identity.split('') : ['C']);
</script>

<span class="identity" title={colors.map((c) => MANA_NAME[c]).join(', ')}>
  {#each colors as c (c)}
    <i style:--dot="var(--{c.toLowerCase()})" style:width="{size}px" style:height="{size}px"></i>
  {/each}
</span>

<style>
  .identity {
    display: inline-flex;
    gap: 3px;
    align-items: center;
  }
  i {
    display: block;
    border-radius: 50%;
    background: var(--dot);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
  }
</style>
