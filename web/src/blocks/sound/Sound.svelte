<script lang="ts">
  import { hasCore } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import { call, type BlockProps } from '../block'
  import { micProblem, recorders, type SoundSide, type SoundState } from './types'
  import { t } from '../../i18n.svelte'

  /**
   * Sound: the output and the microphone, each with its device (a list to change it), its volume
   * and mute; who records now; and, above the microphone, what is wrong with it (too low, muted)
   * with its fix. Short: a line each.
   */
  let { state: snd, error }: BlockProps<SoundState> = $props()

  type Side = 'output' | 'input'
  let failed = $state<string | null>(null)
  /** The slider being dragged: its value until the next read. */
  let dragging = $state<{ side: Side; value: number } | null>(null)

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
    dragging = null
  }

  const problem = $derived(micProblem(snd))
  const recording = $derived(snd?.recording.length ? recorders(snd) : '')
  const short = $derived(isShort('sound'))
  const app = hasCore()
  const shown = (side: Side, s: SoundSide) => (dragging?.side === side ? dragging.value : (s.volume ?? 0))
  const LABEL: Record<Side, string> = { output: t('Output'), input: t('Microphone') }
</script>

{#snippet line(side: Side, s: SoundSide | null)}
  <div class="sec">
    <h3>{LABEL[side]}{#if side === 'input' && recording}<span class="rec" title={t('Recording from a microphone now')}>● {recording}</span>{/if}</h3>
    {#if !s}
      <div class="muted">{side === 'input' ? t('No input device.') : t('No output device.')}</div>
    {:else}
      <div class="dev">
        <select value={s.device} disabled={!app} onchange={(e) => run('device', { side, id: Number(e.currentTarget.value) })} aria-label={t('{side} device', { side: LABEL[side] })}>
          {#each (snd?.devices ?? []).filter((d) => d[side]) as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
        </select>
        {#if s.muted !== null}
          <button class="icon" class:on={s.muted} disabled={!app || !s.mute_settable} onclick={() => run('mute', { side, muted: !s.muted })} title={s.muted ? t('Unmute') : t('Mute')} aria-label={s.muted ? t('Unmute {name}', { name: s.name }) : t('Mute {name}', { name: s.name })}>
            <Icon name={side === 'input' ? (s.muted ? 'micOff' : 'mic') : s.muted ? 'speakerOff' : 'speaker'} />
          </button>
        {/if}
      </div>
      {#if s.volume !== null}
        <div class="vol">
          <span class="k">{side === 'input' ? t('Input') : t('Volume')}</span>
          <input type="range" min="0" max="100" value={shown(side, s)} style:--p="{shown(side, s)}%" disabled={!app || !s.volume_settable} aria-label={t('{side} volume', { side: LABEL[side] })} oninput={(e) => (dragging = { side, value: Number(e.currentTarget.value) })} onchange={(e) => setVolume(side, Number(e.currentTarget.value))} />
          <span class="n" class:low={side === 'input' && !!problem}>{shown(side, s)} %</span>
        </div>
      {:else}
        <div class="muted">{t('No volume on this device: set it on the device itself.')}</div>
      {/if}
    {/if}
  </div>
{/snippet}

<section class="panel" style="--src: var(--accent)" class:collapsed={isCollapsed('sound')}>
  <h2>
    <span class="title"><Icon name="speaker" />{t('Sound')}</span>
    {#if snd?.input && problem}<span class="pill {recording ? 'ko' : 'warn'}">{t('microphone too low')}</span>{/if}
    <BlockControls id="sound" />
  </h2>
  {#if !snd}
    <div class="empty">{t('Sound is not read. Turn on Settings › Infra › Sound, then restart Prism.')}</div>
  {:else if !snd.updated_at}
    <div class="empty">{error ?? t('Reading the audio devices…')}</div>
  {:else if short}
    <div class="short">
      {#each [['output', snd.output], ['input', snd.input]] as const as [side, s] (side)}
        {@const icon = side === 'input' ? (s?.muted ? 'micOff' : 'mic') : s?.muted ? 'speakerOff' : 'speaker'}
        <div class="srow">
          {#if s && s.muted !== null}
            <button class="icon bare" class:on={s.muted} disabled={!app || !s.mute_settable} onclick={() => run('mute', { side, muted: !s.muted })} title={s.muted ? t('Unmute') : t('Mute')} aria-label={s.muted ? t('Unmute {name}', { name: s.name }) : t('Mute {name}', { name: s.name })}><Icon name={icon} /></button>
          {:else}
            <span class="ic"><Icon name={icon} /></span>
          {/if}
          <span class="nm" title={s?.name}>{s?.name ?? '—'}</span>
          {#if s && s.volume !== null}
            <input type="range" min="0" max="100" value={shown(side, s)} style:--p="{shown(side, s)}%" disabled={!app || !s.volume_settable} aria-label={t('{side} volume', { side: LABEL[side] })} oninput={(e) => (dragging = { side, value: Number(e.currentTarget.value) })} onchange={(e) => setVolume(side, Number(e.currentTarget.value))} />
            <span class="n" class:low={side === 'input' && !!problem}>{shown(side, s)} %</span>
          {/if}
        </div>
      {/each}
      {#if failed}<p class="err" title={failed}>{failed}</p>{/if}
    </div>
  {:else}
    <div class="scroll">
      {@render line('output', snd.output)}
      {#if problem && snd.input}
        {@const raise = snd.limits.restore_gain}
        <div class="alert" class:ko={!!recording}>
          <Icon name="mic" />
          <span class="msg">
            <b>{t('Microphone too low')}</b>{recording ? t(': input volume {volume} %, under {min} % while {who} records.', { volume: problem.volume, min: snd.limits.min_input_gain, who: recording }) : t(': input volume {volume} %, under {min} %.', { volume: problem.volume, min: snd.limits.min_input_gain })}
          </span>
          {#if app}
            <button class="act" onclick={() => run('volume', { side: 'input', value: raise })}>{t('Raise to {n} %', { n: raise })}</button>
          {/if}
        </div>
      {/if}
      {@render line('input', snd.input)}
      {#if failed}<p class="err" title={failed}>{failed}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .sec { padding: 8px 12px 10px; border-bottom: 1px solid var(--border); }
  .sec:last-child { border-bottom: none; }
  h3 { display: flex; gap: 6px; margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .rec { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; color: var(--warn); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dev { display: flex; align-items: center; gap: 6px; margin-bottom: 6px; }
  .dev select { flex: 1; min-width: 0; padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 22px; padding: 0; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { color: var(--text); }
  .icon.on { border-color: var(--ko); color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .icon:disabled, select:disabled, input:disabled { opacity: 0.6; cursor: default; }
  .vol { display: flex; align-items: center; gap: 8px; font-size: var(--fs-meta); }
  .vol .k { width: 48px; color: var(--text-muted); }
  .vol input { flex: 1; min-width: 0; --tick: var(--src); }
  .n { width: 44px; text-align: right; font: var(--fs-tag) var(--mono); color: var(--text-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .n.low { color: var(--warn); font-weight: 600; }
  .alert { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--warn) 14%, transparent); color: var(--warn); font-size: var(--fs-meta); }
  .alert.ko { background: color-mix(in srgb, var(--ko) 14%, transparent); color: var(--ko); }
  .alert .msg { flex: 1; min-width: 0; }
  .alert b { color: inherit; }
  .act { padding: 1px 8px; border: 1px solid currentColor; border-radius: var(--r6); background: none; color: inherit; font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  .err { margin: 0; padding: 4px 12px; color: var(--ko); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .short { display: flex; flex-direction: column; }
  .srow { display: flex; align-items: center; gap: 8px; padding: 5px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); white-space: nowrap; }
  .srow:first-child { border-top: none; }
  .srow .nm { flex: 0 1 180px; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .srow .nm:last-child { flex: 1; max-width: none; }
  .srow input { flex: 1; min-width: 40px; --tick: var(--src); }
  .icon.bare { width: 22px; height: 20px; border-color: transparent; }
  .icon.bare.on { border-color: var(--ko); }
  .ic { display: inline-flex; justify-content: center; width: 22px; flex: none; }
</style>
