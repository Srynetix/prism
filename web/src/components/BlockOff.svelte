<script lang="ts">
  import BlockControls from './BlockControls.svelte'
  import Logo from './Logo.svelte'
  import { isCollapsed } from '../layout.svelte'
  import { openSettings } from '../settings.svelte'
  import type { BlockId } from '../views'
  import { t } from '../i18n.svelte'
  import { live } from '../live.svelte'

  /**
   * A block whose source is off (`[renovate] enabled`, `[docker] enabled`…): its title, and how
   * to turn it on, rather than waiting for data that will not come. `reads`: its id among the
   * blocks that read a repository (`Snapshot.repo_blocks`), which may be on with none to read.
   */
  let { id, title, logo, color, section, setting, reads }: { id: BlockId; title: string; logo: string; color: string; section: string; setting: string; reads?: string } = $props()
  const noRepo = $derived(!!reads && live.snapshot?.repo_blocks?.find((b) => b.id === reads)?.repos.length === 0)
</script>

<section class="panel" style="--src: {color}" class:collapsed={isCollapsed(id)}>
  <h2>
    <span class="title"><Logo name={logo as never} />{t(title)}</span>
    <BlockControls {id} />
  </h2>
  {#if noRepo}
    <div class="empty">{t('{title} reads a repository: none is chosen.', { title: t(title) })} <button class="link" onclick={() => openSettings('repo-blocks')}>{t('Choose one')}</button> {t('(Settings › Repositories), or turn it off:')} <code>{setting}</code></div>
  {:else}
    <div class="empty">{t('{title} is off.', { title: t(title) })} <button class="link" onclick={() => openSettings(section)}>{t('Turn it on')}</button> (<code>{setting}</code>).</div>
  {/if}
</section>

<style>
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; }
  .link:hover { text-decoration: underline; }
  code { font: var(--fs-meta) var(--mono); }
</style>
