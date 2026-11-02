<script lang="ts">
  import { columnsOf } from '../../layout.svelte'
  import { views } from '../../views'
  import { stateOf, type StatusProps } from '../block'
  import { petShown } from './pet-store.svelte'
  import Pet from './Pet.svelte'

  /**
   * The status bar's item (`[pet] status_bar`): the Pet, small, while it is on a page but not on
   * the one shown (nor with Settings, Help or the Agents view open). Nothing when on no page.
   */
  let { snap }: StatusProps = $props()

  type Settings = { status_bar?: boolean }
  const settings = $derived(stateOf<Settings>({ id: 'pet' }, snap))
  const onAPage = $derived(views().some((v) => columnsOf(v).flat().includes('pet' as never)))
</script>

{#if settings && settings.status_bar !== false && onAPage && petShown.n === 0}
  <Pet state={settings as never} {snap} place="status" />
{/if}
