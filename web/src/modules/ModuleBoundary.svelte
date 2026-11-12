<script lang="ts">
  import type { Snippet } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import { blockLabel } from '../views'
  import { problems } from '../problems.svelte'
  import { t } from '../i18n.svelte'
  import { OWNER } from '../blocks/runtime'

  /**
   * A module's block, run apart: what it throws (rendering, an effect) stays in its place, said
   * there with Retry, and in the Problems list; the rest of the window goes on. `status`: its
   * status-bar item, a small mark. `missing`: its component did not load (its module's reason).
   * `active` false: not a module's block, rendered as is (one call site for both).
   */
  let { id, active = true, status = false, missing, children }: { id: string; active?: boolean; status?: boolean; missing?: string; children?: Snippet } = $props()

  const module = $derived(OWNER[id] ?? id)

  function report(error: unknown) {
    const message = error instanceof Error ? error.message : String(error)
    console.error(`module ${module}: ${id}`, error)
    const where = t('module {name}', { name: module })
    const same = problems.failed.find((p) => p.where === where && p.message === message)
    problems.failed = [{ level: 'ko' as const, where, message, at: Date.now(), count: (same?.count ?? 0) + 1 }, ...problems.failed.filter((p) => p !== same)].slice(0, 30)
  }
</script>

{#snippet broken(message: string, retry?: () => void)}
  {#if status}
    <button class="mark" title={t('{name}: {error}', { name: blockLabel(id), error: message })} onclick={() => retry?.()}><Icon name="warning" /></button>
  {:else}
    <section class="panel">
      <h2><span class="title"><Icon name="warning" />{blockLabel(id)}</span></h2>
      <div class="empty">
        <p>{t('This block of the module {name} failed: {error}', { name: module, error: message })}</p>
        {#if retry}<button class="btn" onclick={retry}>{t('Retry')}</button>{/if}
      </div>
    </section>
  {/if}
{/snippet}

{#if !active}
  {@render children?.()}
{:else if missing !== undefined || !children}
  {@render broken(missing || t('its component did not load'))}
{:else}
  <svelte:boundary onerror={report}>
    {@render children()}
    {#snippet failed(error, reset)}
      {@render broken(error instanceof Error ? error.message : String(error), reset)}
    {/snippet}
  </svelte:boundary>
{/if}

<style>
  .empty { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  .empty p { margin: 0; overflow-wrap: anywhere; }
  .mark { color: var(--ko); background: none; border: 0; padding: 0 4px; cursor: pointer; }
</style>
