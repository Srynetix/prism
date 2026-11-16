<script lang="ts">
  import { t } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import Icon from '../../components/Icon.svelte'
  import Drawer from '../../components/ui/Drawer.svelte'
  import State from '../../components/ui/State.svelte'
  import { changesView, closeChanges } from '../../changes-view.svelte'
  import ChangesView from './ChangesView.svelte'
  import type { ChangesState } from './changes'

  /** "What changed": the Changes timeline in a drawer, around a crash or on an environment. */
  let { phone = false }: { phone?: boolean } = $props()
  const journal = $derived(live.snapshot?.blocks?.changes as ChangesState | undefined)
  const error = $derived(live.snapshot?.errors?.['changes'] as string | undefined)
  const focusName = $derived(changesView.focus?.env ? `${changesView.focus.env}${changesView.focus.app ? ` · ${changesView.focus.app}` : ''}` : '')
</script>

<Drawer open={changesView.open} key="changes" identity={focusName} crumb={focusName || t('What changed')} label={t('What changed')} size={phone ? 'full' : 'l'} onclose={closeChanges}>
  {#snippet head()}
    <Icon name="activity" />
    <span class="what">{t('What changed')}</span>
    {#if focusName}<span class="muted">{focusName}</span>{/if}
  {/snippet}
  {#if journal}
    <ChangesView state={journal} {error} focus={changesView.focus} />
  {:else}
    <State kind="off" title={t('The Changes block is off. Turn it on in Settings › Infra › Changes ([changes] enabled).')} />
  {/if}
</Drawer>

<style>
  .what { color: var(--text); font-weight: 600; white-space: nowrap; }
  .muted { color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
