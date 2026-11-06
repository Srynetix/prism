<script lang="ts">
  import { t } from '../i18n.svelte'
  import { openOverlap } from '../overlap-view.svelte'
  import { filesSummary, overlapId, topLevel, type Seen } from '../overlaps'
  import Icon from './Icon.svelte'

  /**
   * "Overlap" on an agent, a worktree or an MR: the other side's name (and how many more), the
   * files shared on hover (their lines, at the same lines level), its tone the highest level. A
   * click opens the first one's panel. A span, not a button: it sits inside clickable rows.
   */
  let { seen, short = false }: { seen: Seen[]; short?: boolean } = $props()
  const level = $derived(topLevel(seen))
  const first = $derived(seen.find((x) => x.o.level === level) ?? seen[0])
  const title = $derived(
    seen
      .map((x) => `${x.o.merged ? t('Has to rebase on {name} (merged)', { name: x.otherName }) : x.o.level === 'lines' ? t('Same lines as {name}', { name: x.otherName }) : t('Same file as {name}', { name: x.otherName })}\n${filesSummary(x.o.files, x.side)}`)
      .join('\n\n'),
  )
  function open(e: Event) {
    e.stopPropagation()
    e.preventDefault()
    openOverlap(overlapId(first.o), first.side)
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') open(e)
  }
</script>

{#if first}
  <span class="ov {level === 'lines' ? 'ko' : 'warn'}" role="button" tabindex="0" {title} onclick={open} onkeydown={key}>
    <Icon name="overlap" size={11} />
    {#if !short}<span class="n">{first.o.merged ? t('rebase on {name}', { name: first.otherName }) : first.otherName}</span>{/if}
    {#if seen.length > 1}<span class="more">+{seen.length - 1}</span>{/if}
  </span>
{/if}

<style>
  .ov {
    display: inline-flex; align-items: center; gap: 4px; max-width: 220px; padding: 0 7px; border-radius: var(--r9);
    font-size: var(--fs-tag); font-weight: 500; line-height: 17px; cursor: pointer; white-space: nowrap; vertical-align: middle;
  }
  .ov.ko { color: var(--ko); background: var(--ko-bg); }
  .ov.warn { color: var(--warn); background: var(--warn-bg); }
  .ov:hover { filter: brightness(1.1); }
  .n { overflow: hidden; text-overflow: ellipsis; }
  .more { opacity: 0.8; }
</style>
