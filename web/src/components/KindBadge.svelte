<script lang="ts">
  import Icon from './Icon.svelte'
  import { kindColor } from '../agents.svelte'
  import { kindIcon } from '../agents-list'
  import { t } from '../i18n.svelte'

  /** An agent's kind, as a badge: its colour from its place in `[[agents.kinds]]`, its icon from its name. */
  let { name, title }: { name: string; title?: string } = $props()
</script>

<span class="kind" style:--k={kindColor(name)} title={title ?? t('Kind: {name}', { name })}><Icon name={kindIcon(name)} size={10} />{name}</span>

<style>
  .kind { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; height: 17px; padding: 0 6px 0 4px; border-radius: var(--r5); background: color-mix(in srgb, var(--k) 14%, transparent); color: var(--k); font-size: var(--fs-tag); font-weight: 600; white-space: nowrap; }
</style>
