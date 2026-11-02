<script lang="ts">
  import { agentAttachment, withoutMention } from '../../agents.svelte'
  import { base, promptImages } from './turns'
  import { t } from '../../i18n.svelte'

  /**
   * The images an agent's draft attaches (its `@path`s), above its Chat box: each a thumbnail, ×
   * takes it out of the draft. Read from the draft, they stay over a reload, and go once it is sent.
   */
  let { id, text, write }: { id: string; text: string; write: (t: string) => void } = $props()

  const paths = $derived(promptImages(text).images)
</script>

{#if paths.length}
  <div class="strip">
    {#each paths as path (path)}
      <span class="thumb" title={path}>
        {#await agentAttachment(id, path)}
          <span class="ph"></span>
        {:then url}
          <img src={url} alt={base(path)} />
        {:catch}
          <span class="nm">{base(path)}</span>
        {/await}
        <button type="button" class="x" onclick={() => write(withoutMention(text, path))} aria-label={t('Remove {name}', { name: base(path) })}>×</button>
      </span>
    {/each}
  </div>
{/if}

<style>
  .strip { display: flex; flex-wrap: wrap; gap: 6px; padding: 2px 0 4px; }
  .thumb { position: relative; display: grid; place-items: center; width: 52px; height: 52px; border-radius: 8px; overflow: hidden; background: var(--surface-muted); border: 1px solid var(--border); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; }
  .ph { width: 100%; height: 100%; }
  .nm { padding: 2px; font-size: 9px; color: var(--text-muted); overflow-wrap: anywhere; text-align: center; }
  .x { position: absolute; top: 2px; right: 2px; display: grid; place-items: center; width: 18px; height: 18px; padding: 0; border: none; border-radius: 50%; background: rgba(0, 0, 0, 0.6); color: #fff; font-size: 13px; line-height: 1; cursor: pointer; }
</style>
