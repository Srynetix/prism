<script lang="ts">
  import { t } from '../i18n.svelte'
  import { openOverlap } from '../overlap-view.svelte'
  import { filesSummary, overlapId, topLevel, type Seen } from '../overlaps'
  import CountChip from './CountChip.svelte'

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
  const open = () => openOverlap(overlapId(first.o), first.side)
</script>

{#if first}
  <CountChip tone={level === 'lines' ? 'ko' : 'warn'} icon="overlap" {title} go={open}>
    {#if !short}<span class="n">{first.o.merged ? t('rebase on {name}', { name: first.otherName }) : first.otherName}</span>{/if}
    {#if seen.length > 1}<span class="more">+{seen.length - 1}</span>{/if}
  </CountChip>
{/if}

<style>
  .n { overflow: hidden; text-overflow: ellipsis; }
  .more { opacity: 0.8; }
</style>
