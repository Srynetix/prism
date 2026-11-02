<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { invoke, isDesktop } from '../bridge'
  import BlockControls from './BlockControls.svelte'
  import Icon from './Icon.svelte'
  import { isCollapsed, isShort } from '../layout.svelte'
  import { openSettings } from '../settings.svelte'
  import { ago } from '../time'
  import { toast } from '../toast.svelte'
  import type { MailMessage, MailState } from '../types'

  /**
   * Mail: the unread messages of the Microsoft 365 or Gmail inbox, newest first, and the flagged
   * (starred) ones. A click opens the message in Outlook or Gmail on the web. Signed out, the block
   * leads to Settings.
   */
  let { mail, enabled, error }: { mail: MailState | null | undefined; enabled: boolean; error?: string } = $props()

  const gmail = $derived(mail?.provider === 'gmail')
  const account = $derived(gmail ? 'Google' : 'Microsoft 365')

  const messages = $derived(mail?.messages ?? [])
  const flagged = $derived(messages.filter((m) => m.flagged && !m.unread).length)
  /** Short: the five newest. */
  const listed = $derived(isShort('mail') ? messages.slice(0, 5) : messages)

  const open = (m: MailMessage) => invoke('mail_open', { id: m.id }).catch((e) => toast(String(e), 6000))
</script>

<section class="panel" style="--src: var(--info)" class:collapsed={isCollapsed('mail')}>
  <h2>
    <span class="title"><Icon name="mail" />{t('Mail')}</span>
    {#if mail?.signed_in}<small>{t('{n} unread', { n: mail.unread })}{flagged ? ` · ${gmail ? t('{n} starred', { n: flagged }) : t('{n} flagged', { n: flagged })}` : ''}</small>{/if}
    <BlockControls id="mail" />
  </h2>
  {#if !enabled}
    <div class="empty">{t('Mail is not read.')} <button class="link" onclick={() => openSettings('mail')}>{t('Turn on Mail')}</button> {t('in Settings › Work › Mail, restart Prism, then sign in there (Microsoft 365, or Gmail with your Google account).')}</div>
  {:else if error}
    <div class="empty ko">{error}</div>
  {:else if !mail?.read_at}
    <div class="empty">{t('Reading the inbox…')}</div>
  {:else if !mail.signed_in}
    <div class="empty">{t('Not signed in to {account}.', { account })} <button class="link" onclick={() => openSettings(gmail ? 'mail-gmail' : 'mail-microsoft')}>{t('Sign in')}</button> {t('in Settings › Work › Mail › {provider}.', { provider: gmail ? 'Gmail' : 'Microsoft 365' })}</div>
  {:else}
    <div class="list">
      {#each listed as m (m.id)}
        <button class="msg" class:unread={m.unread} onclick={() => open(m)} title="{m.from} <{m.address}> · {t('open in {app}', { app: gmail ? 'Gmail' : 'Outlook' })}" disabled={!isDesktop()}>
          <span class="dot" aria-label={m.unread ? t('unread') : undefined}></span>
          <span class="what">
            <span class="from">{m.from}</span>
            <span class="subject">{m.subject || t('(no subject)')}</span>
          </span>
          <span class="meta">
            {#if m.flagged}<span class="flag" title={gmail ? t('Starred') : tc('tickets', 'Flagged')}><Icon name="flag" /></span>{/if}
            <span class="age">{ago(m.received)}</span>
          </span>
        </button>
      {:else}
        <div class="empty">{t('Nothing unread.')}</div>
      {/each}
      {#if listed.length < messages.length}<div class="more muted">{t('{n} more: expand the block.', { n: messages.length - listed.length })}</div>{/if}
    </div>
  {/if}
</section>

<style>
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
  .ko { color: var(--ko); }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 2px 0 6px; }
  .msg { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto; gap: 8px; align-items: center; width: 100%; padding: 5px 12px; border: none; border-top: 1px solid var(--border); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .msg:first-child { border-top: none; }
  .msg:hover:not(:disabled) { background: color-mix(in srgb, var(--src) 8%, transparent); }
  .msg:disabled { cursor: default; }
  .dot { width: 7px; height: 7px; border-radius: 50%; }
  .unread .dot { background: var(--src); }
  .what { display: flex; flex-direction: column; min-width: 0; }
  .from, .subject { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .from { font-size: var(--fs-body); color: var(--text-muted); }
  .unread .from { color: var(--text); font-weight: 600; }
  .subject { font-size: var(--fs-meta); color: var(--text-muted); }
  .meta { display: flex; gap: 6px; align-items: center; font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .flag { color: var(--warn); }
  .age { min-width: 26px; text-align: right; font-variant-numeric: tabular-nums; }
  .more { padding: 4px 12px; font-size: var(--fs-tag); }
  .muted { color: var(--text-muted); }
</style>
