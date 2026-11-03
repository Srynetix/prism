<script lang="ts">
  import { clock as ticker } from '../clock.svelte'
  import { t, tc } from '../i18n.svelte'
  import { anchored, dismissable, exclusive } from '../popup'
  import { pref, setPref } from '../layout.svelte'
  import { showIn } from '../youtube'
  import Icon from './Icon.svelte'
  import { audio, clearAudio, clock, externalCommand, externalPosition, loadGroups, next, play, previous, providerOf, seek, setVolume, showsExternal, toggle, type External } from '../audio.svelte'

  /**
   * "Audio" block of the status bar: provider pill, equalizer while playing,
   * title, ⏮ ⏯ ⏭ and a thin progress line (except live streams). A click on the title opens the
   * menu: current track, volume, and all sources (radios, podcast episodes).
   * When macOS "Now Playing" played last (Spotify in the browser…), the block
   * shows it instead, with the app's name, and its buttons control it.
   */
  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  let failures = $state<string[]>([])
  let loading = $state(false)

  const track = $derived(audio.track)
  const provider = $derived(track ? providerOf(track.provider) : null)
  const pct = $derived(audio.duration ? (100 * audio.position) / audio.duration : 0)

  const ext = $derived(showsExternal() ? audio.external : null)
  /** Clock for the external position: it advances without a new event. */
  const now = $derived(ext?.playing ? ticker(1000).now : Date.now())
  const extPos = $derived(ext ? externalPosition(ext, now) : null)
  const extPct = $derived(ext?.duration && extPos !== null ? (100 * extPos) / ext.duration : null)
  const EXT_COLOR = 'var(--info)'
  const appLabel = (x: External) => x.app_name ?? t('this computer')

  /** YouTube: the current video takes precedence over the source name (live, playlist). */
  const title = $derived(track ? (audio.detail?.title ?? track.title) : '')
  const isLive = $derived(!!track && (track.live || audio.ytLive))
  const subtitle = $derived(track ? (audio.detail ? track.title : isLive ? t('Live stream') : track.source) : '')

  /** Groups collapsed in the menu (by key: `radio`, `youtube`, `feed:0`…), remembered. */
  let collapsed = $state<string[]>(pref<string[]>('audioCollapsed', []))
  function fold(key: string) {
    collapsed = collapsed.includes(key) ? collapsed.filter((k) => k !== key) : [...collapsed, key]
    setPref('audioCollapsed', collapsed)
  }

  async function toggleMenu(e: MouseEvent) {
    e.stopPropagation()
    open = !open
    if (open && !audio.groups.length) {
      loading = true
      failures = await loadGroups()
      loading = false
    }
  }
</script>

<span class="audio" class:paused={ext ? !ext.playing : !audio.playing} class:open style:--prov={ext ? EXT_COLOR : (provider?.color ?? 'var(--text-muted)')}>
  <button bind:this={button} class="main" onclick={toggleMenu} aria-expanded={open} title={ext ? `${ext.title} · ${appLabel(ext)}` : track ? `${title} · ${subtitle}` : t('Choose something to listen to')}>
    {#if ext}
      <span class="pv" aria-hidden="true"><Icon name="artwork" /></span>
      <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="trk"><b>{ext.title}</b>{#if ext.artist}<span class="sep"> — </span>{ext.artist}{/if}</span>
      <span class="app">{appLabel(ext)}</span>
    {:else if track && provider}
      <span class="pv" aria-hidden="true"><Icon name={provider.icon} /></span>
      <span class="eq" class:loading={audio.loading} aria-hidden="true"><i></i><i></i><i></i></span>
      <span class="trk"><b>{title}</b>{#if subtitle && subtitle !== t('Live stream')}<span class="sep"> — </span>{subtitle}{/if}</span>
      {#if audio.error}<span class="err">{audio.error}</span>{:else if isLive}<span class="livebadge">{t('LIVE')}</span>{/if}
    {:else}
      <span class="g"><Icon name="music" /></span><span class="trk">Audio</span>
    {/if}
  </button>
  {#if ext}
    <span class="ctl">
      <button onclick={() => externalCommand('previous')} title={t('Previous')} aria-label={t('Previous')}><Icon name="prev" /></button>
      <button class="pp" onclick={() => externalCommand('toggle')} title={ext.playing ? t('Pause') : tc('infra', 'Play')} aria-label={ext.playing ? t('Pause') : tc('infra', 'Play')}><Icon name={ext.playing ? 'pause' : 'play'} /></button>
      <button onclick={() => externalCommand('next')} title={t('Next')} aria-label={t('Next')}><Icon name="next" /></button>
    </span>
    {#if extPct !== null}<span class="prog" aria-hidden="true"><i style:width="{extPct}%"></i></span>{/if}
  {:else if track}
    <span class="ctl">
      <button onclick={previous} title={t('Previous')} aria-label={t('Previous')}><Icon name="prev" /></button>
      <button class="pp" onclick={toggle} title={audio.playing ? t('Pause') : tc('infra', 'Play')} aria-label={audio.playing ? t('Pause') : tc('infra', 'Play')}><Icon name={audio.playing ? 'pause' : 'play'} /></button>
      <button onclick={next} title={t('Next')} aria-label={t('Next')}><Icon name="next" /></button>
    </span>
    {#if !track.live && audio.duration}<span class="prog" aria-hidden="true"><i style:width="{pct}%"></i></span>{/if}
  {/if}
</span>

{#if open}
  <div class="menu" {@attach anchored(() => button, { align: 'end', gap: 6 })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button })}>
    {#if ext}
      <div class="np" style:--prov={EXT_COLOR}>
        <div class="art" aria-hidden="true"><Icon name="artwork" /></div>
        <div class="meta">
          <b title={ext.title}>{ext.title}</b>
          <small>{[ext.artist, ext.album].filter(Boolean).join(' · ') || t('Now playing')} · {appLabel(ext)}</small>
          {#if ext.duration && extPos !== null}
            <span class="scrub"><span>{clock(extPos)}</span><span class="bar"><i style:width="{extPct}%"></i></span><span>{clock(ext.duration)}</span></span>
          {/if}
        </div>
      </div>
      <div class="bigctl">
        <button onclick={() => externalCommand('previous')} aria-label={t('Previous')}><Icon name="prev" /></button>
        <button class="pp" onclick={() => externalCommand('toggle')} aria-label={ext.playing ? t('Pause') : tc('infra', 'Play')}><Icon name={ext.playing ? 'pause' : 'play'} /></button>
        <button onclick={() => externalCommand('next')} aria-label={t('Next')}><Icon name="next" /></button>
      </div>
    {:else if track && provider}
      {#if track.provider === 'youtube'}
        <!-- The YouTube player, hidden the rest of the time, comes to sit on this frame. -->
        <div class="video" {@attach showIn}></div>
      {/if}
      <div class="np" style:--prov={provider.color}>
        <div class="art" aria-hidden="true"><Icon name={provider.icon} /></div>
        <div class="meta">
          <b title={title}>{title}</b>
          <small>{subtitle}{#if audio.error}{' '}· <span class="err">{audio.error}</span>{/if}</small>
          {#if !isLive && !(track.provider === 'youtube' && !audio.duration)}
            <label class="scrub">
              <span>{clock(audio.position)}</span>
              <input type="range" min="0" max={audio.duration || 1} step="1" value={audio.position} style:--p="{audio.duration ? (audio.position / audio.duration) * 100 : 0}%" oninput={(e) => seek(Number(e.currentTarget.value))} disabled={!audio.duration} aria-label={t('Position')} />
              <span>{audio.duration ? clock(audio.duration) : '–'}</span>
            </label>
          {/if}
        </div>
      </div>
      <div class="bigctl">
        <button onclick={previous} aria-label={t('Previous')}><Icon name="prev" /></button>
        <button class="pp" onclick={toggle} aria-label={audio.playing ? t('Pause') : tc('infra', 'Play')}><Icon name={audio.playing ? 'pause' : 'play'} /></button>
        <button onclick={next} aria-label={t('Next')}><Icon name="next" /></button>
        <label class="vol" title={t('Volume')}><Icon name="volume" /><input type="range" min="0" max="1" step="0.05" value={audio.volume} style:--p="{audio.volume * 100}%" oninput={(e) => setVolume(Number(e.currentTarget.value))} aria-label={t('Volume')} /></label>
        <button class="clear" onclick={clearAudio} title={t('Stop and deselect: nothing playing in Prism any more')} aria-label={t('Stop and deselect')}><Icon name="close" /></button>
      </div>
    {/if}
    <div class="list">
      {#if audio.external && !ext}
        {@const x = audio.external}
        <p class="group">{t('On this computer')}<small>{appLabel(x)}</small></p>
        <button class="it" style:--prov={EXT_COLOR} onclick={() => externalCommand('toggle')} title="{x.title} · {appLabel(x)}">
          <span class="pv" aria-hidden="true"><Icon name="artwork" /></span>
          <span class="name">{x.title}{#if x.artist}{' '}— {x.artist}{/if}</span>
          <small>{#if x.playing}<Icon name="running" /> {t('playing')}{:else}{t('paused')}{/if}</small>
        </button>
      {/if}
      {#if loading}<p class="hint">{t('Loading sources…')}</p>{/if}
      {#each failures as f, i (i)}<p class="hint err">{f}</p>{/each}
      {#each audio.groups as g (g.key)}
        {@const p = g.tracks[0] ? providerOf(g.tracks[0].provider) : null}
        {@const folded = collapsed.includes(g.key)}
        <button class="group" class:folded onclick={() => fold(g.key)} aria-expanded={!folded} title={folded ? t('Expand') : t('Collapse')}>
          <span class="chev" aria-hidden="true"><Icon name="chevronDown" /></span>{g.label}<small>{g.tracks.length}</small>
        </button>
        <!-- Collapsed, a group keeps the current track. -->
        {#each folded ? g.tracks.filter((tr) => tr.url === track?.url) : g.tracks as tr (tr.url)}
          <button class="it" class:on={tr.url === track?.url} style:--prov={p?.color} onclick={() => play(tr)} title={tr.title}>
            <span class="pv" aria-hidden="true">{#if p}<Icon name={p.icon} />{/if}</span>
            <span class="name">{tr.title}</span>
            {#if tr.url === track?.url}<small>{#if audio.playing}<Icon name="running" /> {t('playing')}{:else}{t('paused')}{/if}</small>{/if}
          </button>
        {/each}
      {:else}
        {#if !loading && !failures.length}<p class="hint">{t('No source: add')} <code>[[audio.stations]]</code>, <code>[[audio.youtube]]</code> {t('or')} <code>[[audio.feeds]]</code> {t('to config.toml.')}</p>{/if}
      {/each}
    </div>
  </div>
{/if}

<style>
  .audio { position: relative; display: flex; align-items: center; gap: 2px; height: 20px; min-width: 0; max-width: 360px; padding-right: 2px; border-radius: var(--r5); }
  .audio:hover, .audio.open { background: var(--surface-muted); }
  button { font: inherit; color: inherit; border: none; background: none; cursor: pointer; padding: 0; }
  .main { display: flex; align-items: center; gap: 6px; min-width: 0; height: 20px; padding: 0 4px 0 7px; }
  .pv { width: 14px; height: 14px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border-radius: var(--r4); background: var(--prov); color: #fff; font-size: 9px; font-weight: 700; }
  .trk { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .trk b { font-weight: 600; color: var(--text); }
  .sep { opacity: 0.5; }
  .paused .trk, .paused .pv { opacity: 0.6; }
  .err { color: var(--warn); white-space: nowrap; }
  .app { flex-shrink: 0; font-size: var(--fs-tag); color: var(--text-muted); }
  .livebadge { flex-shrink: 0; padding: 0 4px; border-radius: var(--r3); font-size: 9px; font-weight: 700; letter-spacing: 0.4px; background: color-mix(in srgb, var(--ko) 18%, transparent); color: var(--ko); }
  .eq { display: flex; align-items: flex-end; gap: 1.5px; height: 10px; flex-shrink: 0; }
  /* Bars scaled (transform, the GPU's), not resized: a height animated repaints at 60 Hz. */
  .eq i { width: 2px; height: 10px; border-radius: var(--r1); background: var(--prov); transform-origin: bottom; transform: scaleY(0.3); animation: eq 1s ease-in-out infinite; }
  .eq i:nth-child(2) { animation-delay: -0.4s; }
  .eq i:nth-child(3) { animation-delay: -0.7s; }
  .paused .eq i, .eq.loading i { animation: none; opacity: 0.5; }
  .eq.loading i { animation: pulse 0.9s ease-in-out infinite; }
  @keyframes eq { 0%, 100% { transform: scaleY(0.3); } 50% { transform: scaleY(1); } }
  @keyframes pulse { 0%, 100% { opacity: 0.25; } 50% { opacity: 0.8; } }
  :global(html[data-reduce-motion]) .eq i { animation: none; transform: scaleY(0.6); }
  .ctl { display: flex; flex-shrink: 0; }
  .ctl button { width: 18px; height: 18px; display: flex; align-items: center; justify-content: center; border-radius: var(--r4); font-size: 10px; color: var(--text-muted); }
  .ctl button:hover { background: var(--bg); color: var(--text); }
  .ctl .pp { color: var(--text); }
  .prog { position: absolute; left: 7px; right: 7px; bottom: 0; height: 2px; border-radius: var(--r1); background: color-mix(in srgb, var(--prov) 20%, transparent); pointer-events: none; }
  .prog i { display: block; height: 100%; border-radius: var(--r1); background: var(--prov); }

  .menu { position: fixed; z-index: 45; width: 320px; max-height: 70vh; display: flex; flex-direction: column; overflow: hidden; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); font-size: var(--fs-body); color: var(--text); }
  .np { display: flex; gap: 10px; padding: 10px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--prov) 9%, var(--surface)); }
  .art { width: 48px; height: 48px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; border-radius: var(--r8); font-size: 22px; color: #fff; background: linear-gradient(135deg, var(--prov), color-mix(in srgb, var(--prov) 45%, #000)); }
  .meta { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .meta b { font-size: var(--fs-title); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta small { font-size: var(--fs-meta); color: var(--text-muted); }
  .scrub { display: flex; align-items: center; gap: 6px; margin-top: 3px; font-size: var(--fs-tag); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .scrub .bar { flex: 1; height: 4px; border-radius: var(--r2); overflow: hidden; background: var(--surface-muted); }
  .scrub .bar i { display: block; height: 100%; background: var(--prov); }
  .scrub input { flex: 1; min-width: 0; --tick: var(--prov); height: 12px; }
  .bigctl { display: flex; align-items: center; justify-content: center; gap: 14px; padding: 6px 10px 8px; border-bottom: 1px solid var(--border); font-size: 14px; color: var(--text-muted); }
  .bigctl > button:hover { color: var(--text); }
  .bigctl .pp { width: 30px; height: 30px; display: flex; align-items: center; justify-content: center; border-radius: 50%; background: var(--text); color: var(--surface); font-size: 12px; }
  .bigctl .clear { font-size: 12px; }
  .vol { display: flex; align-items: center; gap: 6px; font-size: 13px; }
  .vol input { width: 70px; --tick: var(--text-muted); height: 12px; }
  .list { overflow: auto; padding: 0 4px 6px; }
  .group { display: flex; align-items: center; gap: 4px; width: calc(100% - 4px); margin: 6px 2px 2px; padding: 2px 4px; border-radius: var(--r5); font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .group:hover { background: var(--surface-muted); color: var(--text); }
  .group small { margin-left: auto; }
  .chev { display: inline-flex; width: 10px; transition: transform 0.12s; }
  .group.folded .chev { transform: rotate(-90deg); }
  .video { height: 166px; margin: 8px 10px 0; border-radius: var(--r8); background: #000; }
  .hint { margin: 8px 6px; font-size: var(--fs-meta); color: var(--text-muted); }
  .it { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 6px; border-radius: var(--r6); text-align: left; }
  .it:hover { background: var(--surface-muted); }
  .it.on { background: color-mix(in srgb, var(--prov) 14%, transparent); }
  .it .pv { width: 16px; height: 16px; }
  .it .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .it small { flex-shrink: 0; font-size: var(--fs-tag); color: var(--text-muted); }
</style>
