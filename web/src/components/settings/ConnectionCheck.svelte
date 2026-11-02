<script lang="ts">
  import { invoke, isDesktop } from '../../bridge'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import type { CheckRequest } from '../../settings-checks.svelte'

  /**
   * A login or a token, asked of the service itself (checks.rs: "who am I"): signed in as whom,
   * or what to do. Asked when shown and again with **Test**; `nonce` changing asks again (a token
   * just saved). Never cached: a token fixed a second ago reads fixed.
   */
  let { request, nonce = 0 }: { request: CheckRequest | null; nonce?: number } = $props()

  let phase = $state<'idle' | 'pending' | 'ok' | 'ko'>('idle')
  let message = $state('')
  let asked = 0

  async function test() {
    if (!request || !isDesktop()) return
    const n = ++asked
    phase = 'pending'
    message = ''
    const out = await invoke<{ ok: boolean; message: string }>('check_setting', { check: request }).catch((e) => ({ ok: false, message: String(e) }))
    if (n !== asked) return
    phase = out.ok ? 'ok' : 'ko'
    message = out.message
  }
  // A pause after the last keystroke (Jira's site and account are typed in the form).
  $effect(() => {
    void nonce
    void JSON.stringify(request)
    const timer = setTimeout(() => void test(), 600)
    return () => clearTimeout(timer)
  })
</script>

{#if request && isDesktop()}
  <span class="conn {phase}">
    <Icon name={phase === 'ok' ? 'check' : phase === 'ko' ? 'warning' : 'pending'} />
    <span class="msg">{phase === 'pending' ? t('Checking…') : message || t('Not checked yet')}</span>
    <button class="test" onclick={test} disabled={phase === 'pending'} title={t('Ask the service again who this login is')}>{t('Test')}</button>
  </span>
{/if}

<style>
  .conn { display: flex; align-items: center; gap: 6px; font-size: var(--fs-meta); color: var(--text-muted); }
  .conn.ok { color: var(--ok); }
  .conn.ko { color: var(--ko); }
  .msg { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .test { flex-shrink: 0; padding: 1px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .test:hover:not(:disabled) { border-color: var(--accent); }
  .test:disabled { opacity: 0.5; cursor: default; }
</style>
