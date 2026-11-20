<script lang="ts">
  import Icon from './Icon.svelte'
  import { live } from '../live.svelte'
  import { isDesktop } from '../bridge'
  import { sendPrompt } from '../agent-inbox.svelte'
  import { atLabel, cancelResume, compactThenResume, errorInfo, failureOf, limitResetOf, resumeAgent } from '../agent-errors'
  import { toast } from '../toast.svelte'
  import { t } from '../i18n.svelte'
  import type { Agent } from '../types'

  /**
   * An agent stopped by an API error (`failed`): what it is, what to do, and its resume — now,
   * at its usage limit's reset, or the automatic one (`[agents] retry_on_error`) counting down,
   * cancellable. The same in the Agents view and on the phone; /login (free typing in its
   * terminal) only on the computer.
   */
  let { a }: { a: Agent } = $props()

  const failure = $derived(failureOf(a))
  const info = $derived(failure ? errorInfo(failure.kind) : null)
  const reset = $derived(failure?.kind === 'rate_limit' ? limitResetOf(live.snapshot, a) : null)
  let busy = $state(false)
  let now = $state(Date.now())
  $effect(() => {
    if (!failure?.resume_at) return
    const timer = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(timer)
  })
  /** `4:05`: what is left before the resume planned. */
  const left = $derived.by(() => {
    if (!failure?.resume_at) return ''
    const s = Math.max(0, Math.round((new Date(failure.resume_at).getTime() - now) / 1000))
    return s >= 3600 ? atLabel(failure.resume_at) : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
  })
  async function run(f: () => Promise<unknown>) {
    busy = true
    try {
      await f()
    } finally {
      busy = false
    }
  }
  /** A command of its CLI typed in its terminal (/login, /compact), then the resume when it asks for it. */
  async function type(command: string, then: string) {
    await sendPrompt(a.id, command)
    toast(then, 5000, null, 'info')
  }
</script>

{#if failure && info}
  <div class="banner" role="alert">
    <Icon name="warning" />
    <div class="what">
      <b>{info.short}</b>
      <small>{info.message}</small>
      {#if failure.text}<code>{failure.text}</code>{/if}
      {#if failure.resume_at}
        <small class="plan">
          {#if failure.auto}{t('Resuming on its own in {left} (try {n} of 3)', { left, n: failure.attempts + 1 })}{:else}{t('Resumes at {at}', { at: atLabel(failure.resume_at) })}{/if}
        </small>
      {:else if failure.attempts >= 3}
        <small class="plan">{t('Resumed 3 times on its own: it is up to you now.')}</small>
      {/if}
    </div>
    <div class="acts">
      {#if failure.resume_at}
        <button class="btn sm ghost" disabled={busy} onclick={() => run(() => cancelResume(a.id))}>{t('Cancel')}</button>
        <button class="btn sm primary" disabled={busy} onclick={() => run(() => resumeAgent(a.id))}>{t('Resume now')}</button>
      {:else if info.action === 'resume-at'}
        {#if reset}<button class="btn sm primary" disabled={busy} onclick={() => run(() => resumeAgent(a.id, reset))} title={t('Its resume prompt sent once the plan resets')}>{t('Resume at {at}', { at: atLabel(reset) })}</button>{/if}
        <button class="btn sm" class:primary={!reset} disabled={busy} onclick={() => run(() => resumeAgent(a.id))}>{t('Resume now')}</button>
      {:else if info.action === 'login'}
        {#if isDesktop()}<button class="btn sm primary" disabled={busy} onclick={() => run(() => type('/login', t('/login typed in its terminal: log in there, then Resume.')))}>{t('Log in again')}</button>{/if}
        <button class="btn sm" disabled={busy} onclick={() => run(() => resumeAgent(a.id))}>{t('Resume')}</button>
      {:else if info.action === 'compact'}
        <button class="btn sm primary" disabled={busy} onclick={() => run(() => compactThenResume(a.id))} title={t('/compact, then its resume prompt once compacted')}>{t('Compact then resume')}</button>
        <button class="btn sm" disabled={busy} onclick={() => run(() => resumeAgent(a.id))}>{t('Resume')}</button>
      {:else if info.action === 'model'}
        <button class="btn sm" disabled={busy} onclick={() => run(() => resumeAgent(a.id))} title={t('After picking another model')}>{t('Resume')}</button>
      {:else if info.action === 'resume'}
        <button class="btn sm primary" disabled={busy} onclick={() => run(() => resumeAgent(a.id))} title={t('Its resume prompt: the error has passed, it goes on where it was')}>{t('Resume')}</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .banner { display: flex; align-items: flex-start; gap: 10px; padding: 8px 12px; border-bottom: 1px solid color-mix(in srgb, var(--ko) 35%, var(--border)); background: color-mix(in srgb, var(--ko) 10%, var(--surface)); color: var(--ko); }
  .banner > :global(svg) { margin-top: 3px; flex-shrink: 0; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; font-size: var(--fs-meta); color: var(--text); }
  .what b { color: var(--ko); }
  .what small { color: var(--text-muted); }
  .what code { color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); overflow-wrap: anywhere; }
  .what .plan { color: var(--text); font-variant-numeric: tabular-nums; }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; justify-content: flex-end; }
</style>
