<script lang="ts">
  import ConnectionCheck from './ConnectionCheck.svelte'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { forgeLogin } from '../../credentials'
  import { writeClipboard } from '../../toast.svelte'

  /**
   * The forge's access: glab's or gh's own login, which Prism never sees. The command to run in a
   * terminal, copied in a click, and whom the CLI is signed in as (asked again with Test, after
   * logging in).
   */
  let { forge, host = '' }: { forge: 'gitlab' | 'github'; host?: string } = $props()
  const login = $derived(forgeLogin(forge, host))
  let copied = $state(false)
  async function copy() {
    await writeClipboard(login.command).catch(() => {})
    copied = true
    setTimeout(() => (copied = false), 1500)
  }
</script>

<div class="login">
  <span class="cmd">{t('In a terminal:')} <code>{login.command}</code><button class="copy" onclick={copy} title={t('Copy the command')}>{#if copied}<Icon name="check" />{:else}<Icon name="copy" />{/if}</button></span>
  <ConnectionCheck request={login.check} />
</div>

<style>
  .login { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
  .cmd { display: flex; align-items: center; gap: 6px; min-width: 0; font-size: var(--fs-meta); color: var(--text-muted); white-space: nowrap; }
  .cmd code { overflow: hidden; text-overflow: ellipsis; }
  code { padding: 1px 6px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text); font-family: var(--mono); font-size: var(--fs-mono); }
  .copy { padding: 1px 4px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .copy:hover { color: var(--text); }
</style>
