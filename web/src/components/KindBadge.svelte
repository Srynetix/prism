<script lang="ts">
  import Icon from './Icon.svelte'
  import { kindColor } from '../agents.svelte'
  import { kindIcon } from '../agents-list'
  import { t } from '../i18n.svelte'

  /** An agent's kind, as a badge: its colour from its place in `[[agents.kinds]]`, its icon from its name. */
  let { name, title }: { name: string; title?: string } = $props()
</script>

<span class="pill src kind" style:--c={kindColor(name)} title={title ?? t('Kind: {name}', { name })}><Icon name={kindIcon(name)} size={10} />{name}</span>

<style>
  .kind { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; font-weight: 600; }
</style>
