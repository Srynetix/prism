<script lang="ts">
  import { onDestroy } from 'svelte'
  import Icon from '../Icon.svelte'
  import DictateButton from '../chat/DictateButton.svelte'
  import DictationBar from '../chat/DictationBar.svelte'
  import { hasCore, isDesktop, localInvoke, localListen } from '../../bridge'
  import { t, tc } from '../../i18n.svelte'
  import { isMac } from '../../platform'
  import { armer } from '../../armed.svelte'
  import { setValue, valueOf } from '../../settings.svelte'

  /**
   * Settings › Agents › Dictation, beyond its keys: the engine `auto` picks on this computer, the
   * system's recognition (macOS) and its permission, whisper's models (downloaded here, checked,
   * removed), the microphones, what macOS does with Fn, and a test.
   */
  type Model = { name: string; bytes: number; installed: boolean }
  type Status = { engine: 'system' | 'whisper'; system: boolean; system_auth: string; system_language: string; models: Model[]; downloading: string[]; devices: string[]; fn_usage: number | null; demo: boolean }
  type Download = { model: string; done: number; total: number; finished: boolean; error: string | null }

  const NOTES: Record<string, string> = {
    tiny: t('very fast, many mistakes'),
    base: t('fast · about 1 s for 15 s of speech'),
    small: t('better on technical words · about 3 s'),
    medium: t('the most accurate, slow without a GPU'),
  }
  let status = $state<Status | null>(null)
  let error = $state('')
  let progress = $state<Record<string, number>>({})
  let test = $state('')
  let testField = $state<HTMLTextAreaElement>()
  const arm = armer()

  async function load() {
    if (!isDesktop()) return
    try {
      status = await localInvoke<Status>('dictation_status')
      for (const m of status.downloading) progress[m] ??= 0
    } catch (e) {
      error = String(e)
    }
  }
  $effect(() => void load())

  let unlisten: (() => void) | undefined
  if (isDesktop()) {
    void localListen<Download>('dictation-download', (e) => {
      const d = e.payload
      if (d.finished) {
        delete progress[d.model]
        if (d.error) error = t('{model}: {error}', { model: d.model, error: d.error })
        void load()
      } else progress[d.model] = d.total ? d.done / d.total : 0
    }).then((u) => (unlisten = u))
  }
  onDestroy(() => unlisten?.())

  async function download(model: string) {
    error = ''
    progress[model] = 0
    try {
      await localInvoke('dictation_download', { model })
    } catch (e) {
      delete progress[model]
      error = String(e)
    }
  }
  async function remove(model: string) {
    if (!arm.confirm(`rm:${model}`)) return
    try {
      await localInvoke('dictation_remove', { model })
    } catch (e) {
      error = String(e)
    }
    void load()
  }

  const size = (b: number) => (b >= 1e9 ? t('{n} GB', { n: (b / 1e9).toFixed(1) }) : t('{n} MB', { n: Math.round(b / 1e6) }))
  const chosenModel = $derived(valueOf(['dictation', 'whisper_model'], 'base'))
  const engine = $derived(valueOf<string>(['dictation', 'engine'], 'auto'))
  const device = $derived(valueOf(['dictation', 'input_device'], ''))
  const shortcut = $derived(valueOf(['dictation', 'shortcut'], isMac ? 'Fn' : 'Ctrl+Shift+Space'))
  const needsModel = $derived(status?.engine === 'whisper' && !status.models.find((m) => m.name === chosenModel)?.installed)
</script>

{#if !hasCore()}
  <div class="card"><p class="hint">{t('Not in the browser preview.')}</p></div>
{:else if !isDesktop()}
  <div class="card"><p class="hint">{t('On the computer only: a phone has its keyboard’s dictation.')}</p></div>
{:else}
  <div class="card">
    <div class="head"><b>{t('On this computer')}</b><span class="grow"></span><button class="btn ghost" onclick={load} title={t('Read again')}><Icon name="refresh" /></button></div>
    {#if status}
      <p class="line"><Icon name="mic" />
        {#if status.engine === 'system'}{t('Dictation goes to macOS’s speech recognition ({language}), on this Mac only.', { language: status.system_language })}
        {:else}{t('Dictation goes to whisper, model {model}, on this computer.', { model: chosenModel })}{/if}
        {#if engine === 'auto'}<span class="hint">{t('(auto)')}</span>{/if}
      </p>
      {#if needsModel}<p class="warn"><Icon name="warning" /> {t('The {model} model is not downloaded yet: download it below.', { model: chosenModel })}</p>{/if}
      {#if isMac}
        {#if status.system_auth === 'denied' || status.system_auth === 'restricted'}
          <p class="warn"><Icon name="warning" /> {t('macOS’s speech recognition is refused to Prism: System Settings › Privacy & Security › Speech Recognition.')}{#if engine !== 'whisper'} {t('Meanwhile, auto uses whisper.')}{/if}</p>
        {:else if !status.system && engine !== 'whisper'}
          <p class="hint">{t('macOS has no on-device recognition for {language}: System Settings › Keyboard › Dictation installs it. Meanwhile, auto uses whisper.', { language: status.system_language })}</p>
        {:else if status.system_auth === 'not_determined' && status.engine === 'system'}
          <p class="hint">{t('macOS asks once, at the first dictation, to allow its speech recognition.')}</p>
        {/if}
        {#if shortcut.trim().toLowerCase() === 'fn' && status.fn_usage}
          <p class="warn"><Icon name="warning" /> {status.fn_usage === 3 ? t('macOS starts its own dictation with Fn too.') : status.fn_usage === 2 ? t('macOS shows the emoji with Fn too.') : t('macOS changes the input source with Fn too.')} {t('System Settings › Keyboard › Press 🌐 key to: Do Nothing, or choose another shortcut.')}</p>
        {/if}
      {/if}
      {#if status.demo}<p class="hint">{t('Demo profile: nothing is recorded, a sample sentence comes back.')}</p>{/if}
    {:else if error}
      <p class="warn">{error}</p>
    {:else}
      <p class="hint">{t('Reading…')}</p>
    {/if}
  </div>

  <div class="card">
    <div class="head"><b>{t('whisper’s models')}</b><span class="hint">{t('downloaded here when needed, checked, kept for every profile')}</span></div>
    {#each status?.models ?? [] as m (m.name)}
      {@const p = progress[m.name]}
      <div class="row" class:on={m.name === chosenModel}>
        <button class="radio" class:on={m.name === chosenModel} onclick={() => setValue(['dictation', 'whisper_model'], m.name, 'base')} aria-label={t('Use {model}', { model: m.name })} aria-pressed={m.name === chosenModel}></button>
        <b class="name">{m.name}</b><span class="hint sz">{size(m.bytes)}</span><span class="hint grow">{NOTES[m.name] ?? ''}</span>
        {#if p !== undefined}
          <span class="prog"><i style:width="{Math.round(p * 100)}%"></i></span><span class="hint">{Math.round(p * 100)} %</span>
          <button class="btn ghost" onclick={() => localInvoke('dictation_cancel_download', { model: m.name })}>{t('Cancel')}</button>
        {:else if m.installed}
          <span class="chip ok">{t('downloaded')}</span>
          <button class="btn ghost" onclick={() => remove(m.name)}>{arm.is(`rm:${m.name}`) ? t('Remove?') : t('Remove')}</button>
        {:else}
          <button class="btn" disabled={status?.demo} onclick={() => download(m.name)}>{t('Download')}</button>
        {/if}
      </div>
    {/each}
    {#if error && status}<p class="warn">{error}</p>{/if}
  </div>

  <div class="card">
    <div class="head"><b>{t('Microphone')}</b></div>
    <div class="row"><button class="radio" class:on={!device} onclick={() => setValue(['dictation', 'input_device'], '', '')} aria-pressed={!device} aria-label={tc('microphone', 'The system’s')}></button><span>{tc('microphone', 'The system’s')}</span>{#if status?.devices[0]}<span class="hint">· {status.devices[0]}</span>{/if}</div>
    {#each status?.devices ?? [] as d (d)}
      <div class="row"><button class="radio" class:on={device === d} onclick={() => setValue(['dictation', 'input_device'], d, '')} aria-pressed={device === d} aria-label={d}></button><span>{d}</span></div>
    {/each}
    {#if device && status && !status.devices.includes(device)}<p class="warn"><Icon name="warning" /> {t('{device} is not connected: the system’s is used.', { device })}</p>{/if}
  </div>

  <div class="card">
    <div class="head"><b>{t('Try it')}</b><span class="hint">{t('hold the microphone and say something (saved first: it uses the settings saved)')}</span></div>
    <div class="test">
      <textarea bind:this={testField} bind:value={test} rows="2" placeholder={t('The text comes here…')}></textarea>
      <DictateButton key="settings-test" field={() => testField} text={() => test} write={(v) => (test = v)} />
    </div>
    <DictationBar key="settings-test" />
  </div>
{/if}

<style>
  .card { border: 1px solid var(--border); border-radius: var(--r8, 8px); padding: 10px 12px; margin: 0 0 12px; display: flex; flex-direction: column; gap: 4px; }
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .grow { flex: 1; }
  .hint { color: var(--text-muted); font-size: var(--fs-meta); margin: 0; }
  .line { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin: 4px 0; }
  .warn { display: flex; align-items: center; gap: 6px; color: var(--warn); font-size: var(--fs-meta); margin: 2px 0; }
  .row { display: flex; align-items: center; gap: 8px; padding: 5px 0; border-top: 1px solid var(--border); font-size: var(--fs-meta); }
  .name { min-width: 52px; }
  .sz { min-width: 56px; }
  .radio { flex: none; width: 13px; height: 13px; padding: 0; border-radius: 50%; border: 1.5px solid var(--text-muted); background: none; cursor: pointer; }
  .radio.on { border: 4px solid var(--accent); }
  .chip { padding: 0 8px; border-radius: var(--r9, 9px); background: var(--surface-muted); font-size: var(--fs-tag); line-height: 18px; }
  .chip.ok { color: var(--ok); background: var(--ok-bg); }
  .prog { width: 110px; height: 5px; border-radius: 3px; background: var(--surface-muted); overflow: hidden; }
  .prog i { display: block; height: 100%; background: var(--info, var(--accent)); }
  .btn { display: inline-flex; align-items: center; font: inherit; font-size: var(--fs-meta); padding: 3px 10px; border-radius: var(--r6, 6px); border: 1px solid var(--border); background: var(--surface); color: var(--text); cursor: pointer; }
  .btn:hover:not(:disabled) { background: var(--surface-muted); }
  .btn:disabled { opacity: 0.5; }
  .btn.ghost { border-color: transparent; }
  .test { display: flex; align-items: flex-start; gap: 6px; }
  .test textarea { flex: 1; min-width: 0; resize: vertical; border: 1px solid var(--border); border-radius: 7px; padding: 5px 8px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
</style>
