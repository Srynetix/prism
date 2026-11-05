<script lang="ts">
  import { hasCore } from '../../bridge'
  import Icon from '../../components/Icon.svelte'
  import { showBlock } from '../../layout.svelte'
  import { call, stateOf, type StatusProps } from '../block'
  import { micProblem, recorders, type SoundSide, type SoundState } from './types'
  import { t } from '../../i18n.svelte'

  /**
   * The status bar's item. With `[sound] status_volume` (on by default): always there, the output
   * volume and the microphone's beside it (coloured while too low); the wheel sets the output volume, a click opens
   * a panel — output and microphone, each with its mute, volume and device, "Raise" while too low.
   * Off: nothing, unless the microphone is too low; a click shows the block.
   */
  let { snap }: StatusProps = $props()

  type Side = 'output' | 'input'
  const snd = $derived(stateOf<SoundState>({ id: 'sound' }, snap))
  const problem = $derived(micProblem(snd))
  const always = $derived(!!snd?.updated_at && snd.limits.status_volume !== false)
  const rec = $derived(!!snd?.recording.length)
  const app = hasCore()

  let open = $state(false)
  let at = $state({ left: 0, bottom: 0 })
  let failed = $state<string | null>(null)
  /** The slider being dragged, or the wheel's value: shown until the next read. */
  let pending = $state<{ side: Side; value: number } | null>(null)
  const LABEL: Record<Side, string> = { output: t('Output'), input: t('Microphone') }
  const shown = (side: Side, s: SoundSide) => (pending?.side === side ? pending.value : (s.volume ?? 0))

  async function run(command: string, args: Record<string, unknown>) {
    failed = null
    try {
      await call('sound', command, args)
    } catch (e) {
      failed = String(e)
    }
  }
  const setVolume = async (side: Side, value: number) => {
    await run('volume', { side, value })
    pending = null
  }

  function toggle(e: MouseEvent) {
    e.stopPropagation()
    if (!always) return showBlock('sound')
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    // Under its item, kept inside the window.
    at = { left: Math.max(8, Math.min(r.right - 290, innerWidth - 298)), bottom: innerHeight - r.top + 4 }
    open = !open
  }

  /** The wheel on the item: ±5 % of output volume a notch, a trackpad's flow taken a step at a time. */
  let lastWheel = 0
  let chip = $state<HTMLButtonElement>()
  // Not passive, so it does not scroll the status bar too.
  $effect(() => {
    if (!chip) return
    const el = chip
    el.addEventListener('wheel', wheel, { passive: false })
    return () => el.removeEventListener('wheel', wheel)
  })
  function wheel(e: WheelEvent) {
    const out = snd?.output
    if (!always || !app || !out || out.volume === null || !out.volume_settable) return
    e.preventDefault()
    if (e.timeStamp - lastWheel < 60 || e.deltaY === 0) return
    lastWheel = e.timeStamp
    const value = Math.max(0, Math.min(100, shown('output', out) + (e.deltaY < 0 ? 5 : -5)))
    pending = { side: 'output', value }
    void setVolume('output', value)
  }

  const level = (side: Side, s: SoundSide) => (s.muted ? t('muted') : s.volume === null ? '—' : `${shown(side, s)} %`)
  const icon = (side: Side, s: SoundSide | null) => (side === 'input' ? (s?.muted ? 'micOff' : 'mic') : s?.muted ? 'speakerOff' : 'speaker')
  const title = $derived.by(() => {
    if (!snd?.input || !problem) return ''
    const vars = { name: snd.input.name, volume: problem.volume, min: snd.limits.min_input_gain, who: rec ? recorders(snd) : '' }
    return rec ? t('{name}: input volume {volume} %, under {min} % while {who} records · open Sound', vars) : t('{name}: input volume {volume} %, under {min} % · open Sound', vars)
  })
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} onclick={() => (open = false)} />

{#if always && snd}
  {@const out = snd.output}
  {@const inp = snd.input}
  <button class="sbi snd" class:open bind:this={chip} onclick={toggle} title={`${out ? `${out.name}: ${level('output', out)}` : t('No output device.')}${title ? ` · ${title.replace(/ · .*$/, '')}` : inp ? ` · ${inp.name}: ${level('input', inp)}` : ''} · ${t('a click: output and microphone, the wheel: volume')}`}>
    <Icon name={icon('output', out)} />
    {#if out?.muted}{t('muted')}{:else if out && out.volume !== null}{shown('output', out)} %{/if}
    {#if inp}<span class="mic" class:low={!!problem} class:ko={!!problem && rec}><Icon name={icon('input', inp)} />{#if !inp.muted && inp.volume !== null}{shown('input', inp)} %{/if}</span>{/if}
  </button>
{:else if snd?.input && problem}
  <button class="sbi mic" class:ko={rec} onclick={toggle} {title}>
    <Icon name="mic" />
    {problem.volume} %
  </button>
{/if}

{#snippet line(side: Side, s: SoundSide | null)}
  <div class="sec">
    <h4>{LABEL[side]}{#if side === 'input' && rec && snd}<span class="rec" title={t('Recording from a microphone now')}>● {recorders(snd)}</span>{/if}</h4>
    {#if !s}
      <div class="none">{side === 'input' ? t('No input device.') : t('No output device.')}</div>
    {:else}
      <div class="row">
        {#if s.muted !== null}
          <button class="ib" class:on={s.muted} disabled={!app || !s.mute_settable} onclick={() => run('mute', { side, muted: !s.muted })} title={s.muted ? t('Unmute') : t('Mute')} aria-label={s.muted ? t('Unmute {name}', { name: s.name }) : t('Mute {name}', { name: s.name })}><Icon name={icon(side, s)} /></button>
        {:else}
          <span class="ic"><Icon name={icon(side, s)} /></span>
        {/if}
        {#if s.volume !== null}
          <input type="range" min="0" max="100" value={shown(side, s)} style:--p="{shown(side, s)}%" disabled={!app || !s.volume_settable} aria-label={t('{side} volume', { side: LABEL[side] })} oninput={(e) => (pending = { side, value: Number(e.currentTarget.value) })} onchange={(e) => setVolume(side, Number(e.currentTarget.value))} />
          <span class="n" class:low={side === 'input' && !!problem}>{shown(side, s)} %</span>
        {:else}
          <span class="none grow">{t('No volume on this device: set it on the device itself.')}</span>
        {/if}
      </div>
      <select value={s.device} disabled={!app} onchange={(e) => run('device', { side, id: Number(e.currentTarget.value) })} aria-label={t('{side} device', { side: LABEL[side] })}>
        {#each (snd?.devices ?? []).filter((d) => d[side]) as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
      </select>
    {/if}
  </div>
{/snippet}

{#if open && always && snd}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="pop" role="dialog" aria-label={t('Sound')} tabindex="-1" style:left="{at.left}px" style:bottom="{at.bottom}px" onclick={(e) => e.stopPropagation()}>
    {@render line('output', snd.output)}
    {#if problem && snd.input}
      {@const raise = snd.limits.restore_gain}
      <div class="alert" class:ko={rec}>
        <Icon name="mic" />
        <span class="msg">{t('Microphone at {volume} %, under {min} %', { volume: problem.volume, min: snd.limits.min_input_gain })}</span>
        {#if app}<button class="act" onclick={() => run('volume', { side: 'input', value: raise })}>{t('Raise to {n} %', { n: raise })}</button>{/if}
      </div>
    {/if}
    {@render line('input', snd.input)}
    {#if failed}<p class="err" title={failed}>{failed}</p>{/if}
    <div class="ft">
      <span>{t('the wheel on the item: ±5 %')}</span>
      <button class="link" onclick={() => { open = false; showBlock('sound') }}>{t('Open Sound')} ›</button>
    </div>
  </div>
{/if}

<style>
  .mic { display: inline-flex; align-items: center; gap: 3px; color: var(--warn); }
  .mic.ko { color: var(--ko); }
  .snd .mic { margin-left: 3px; color: inherit; }
  .snd .mic.low { color: var(--warn); }
  .snd .mic.ko { color: var(--ko); }
  .pop { position: fixed; z-index: 60; width: 290px; padding: 2px 0; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-drawer); font-size: var(--fs-meta); }
  .sec { padding: 7px 12px 8px; border-top: 1px solid var(--border); }
  .sec:first-child { border-top: none; }
  h4 { display: flex; gap: 6px; margin: 0 0 5px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .rec { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; color: var(--warn); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row { display: flex; align-items: center; gap: 8px; }
  .row input { flex: 1; min-width: 0; --tick: var(--accent); }
  .ib { display: inline-flex; align-items: center; justify-content: center; flex: none; width: 24px; height: 22px; padding: 0; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .ib:hover:not(:disabled) { color: var(--text); }
  .ib.on { border-color: var(--ko); color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .ic { display: inline-flex; justify-content: center; flex: none; width: 24px; color: var(--text-muted); }
  .ib:disabled, select:disabled, input:disabled { opacity: 0.6; cursor: default; }
  .n { width: 40px; text-align: right; font: var(--fs-tag) var(--mono); color: var(--text-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .n.low { color: var(--warn); font-weight: 600; }
  select { width: 100%; margin-top: 6px; padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); color: var(--text); font: inherit; }
  .none { color: var(--text-muted); }
  .grow { flex: 1; }
  .alert { display: flex; align-items: center; gap: 8px; margin: 0 8px 4px; padding: 5px 8px; border-radius: var(--r6); background: color-mix(in srgb, var(--warn) 14%, transparent); color: var(--warn); }
  .alert.ko { background: color-mix(in srgb, var(--ko) 14%, transparent); color: var(--ko); }
  .alert .msg { flex: 1; min-width: 0; }
  .act { padding: 1px 8px; border: 1px solid currentColor; border-radius: var(--r6); background: none; color: inherit; font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  .err { margin: 0; padding: 4px 12px; color: var(--ko); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ft { display: flex; align-items: center; justify-content: space-between; padding: 6px 12px 4px; border-top: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); }
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; }
</style>
