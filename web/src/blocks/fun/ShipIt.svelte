<script lang="ts">
  import { onMount } from 'svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import { isCollapsed } from '../../layout.svelte'
  import { alerts } from '../../rules'
  import type { BlockProps } from '../block'
  import { t } from '../../i18n.svelte'

  /**
   * "Should I deploy?": an answer from the clock (Friday afternoon, evenings, weekends), the
   * meeting about to start, and the CI (a red protected branch, a red alert). Changes by the minute.
   */
  let { snap }: BlockProps<never> = $props()
  let now = $state(new Date())
  onMount(() => {
    const timer = setInterval(() => (now = new Date()), 60_000)
    return () => clearInterval(timer)
  })

  type Verdict = { word: string; why: string; tone: 'ok' | 'warn' | 'ko' }
  const pick = (list: string[]) => list[(now.getDate() + now.getHours()) % list.length]
  const verdict = $derived.by((): Verdict => {
    const day = now.getDay()
    const h = now.getHours() + now.getMinutes() / 60
    const red = snap.pipelines.filter((p) => p.status === 'failed')
    const soon = (snap.agenda?.events ?? []).find((e) => !e.all_day && !e.canceled && +new Date(e.start) > +now && +new Date(e.start) - +now < 15 * 60_000)
    if (day === 6 || day === 0) return { word: t('No.'), why: t(pick(["It's the weekend. Go outside.", 'Nobody is on call who wants to be.', 'The servers are resting too.'])), tone: 'ko' }
    if (day === 5 && h >= 16) return { word: t('Absolutely not.'), why: t(pick(['Friday, after 4 pm. You know better.', 'Read-only Friday.', 'Your weekend says thanks in advance.'])), tone: 'ko' }
    if (h >= 19 || h < 7) return { word: t('No.'), why: t(pick(['Too late: who would roll it back?', 'Sleep on it.', 'Night deploys make morning incidents.'])), tone: 'ko' }
    if (red.length) return { word: t('Not now.'), why: t('{target} is red: fix it first.', { target: red[0].target }), tone: 'ko' }
    if (alerts(snap).some((a) => a.severity === 'ko')) return { word: t('Not now.'), why: t('Something is broken already (Alerts).'), tone: 'ko' }
    if (soon) return { word: t('Wait.'), why: t('“{title}” starts in a few minutes.', { title: soon.title }), tone: 'warn' }
    if (day === 5) return { word: t('If you must.'), why: t('Friday: small, and early.'), tone: 'warn' }
    if (day === 1 && h < 10) return { word: t('Coffee first.'), why: t('Monday morning: read the weekend’s alerts, then ship.'), tone: 'warn' }
    return { word: t('Yes, ship it.'), why: t(pick(['Everything is green.', 'The stars are aligned.', 'What could possibly go wrong?'])), tone: 'ok' }
  })
</script>

<section class="panel" style="--src: var(--{verdict.tone})" class:collapsed={isCollapsed('shipit')}>
  <h2>
    <span class="title">{t('Ship it?')}</span>
    <BlockControls id="shipit" />
  </h2>
  <div class="body">
    <p class="word {verdict.tone}">{verdict.word}</p>
    <p class="why">{verdict.why}</p>
  </div>
</section>

<style>
  .body { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 4px; padding: 14px 12px; text-align: center; }
  .word { margin: 0; font-size: 26px; font-weight: 700; letter-spacing: -0.5px; }
  .word.ok { color: var(--ok); }
  .word.warn { color: var(--warn); }
  .word.ko { color: var(--ko); }
  .why { margin: 0; color: var(--text-muted); font-size: var(--fs-body); }
</style>
