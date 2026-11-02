<script lang="ts">
  import { onMount } from 'svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import type { BlockProps } from '../block'
  import { isCollapsed } from '../../layout.svelte'
  import { copy } from '../../toast.svelte'
  import { t } from '../../i18n.svelte'

  /** A quote about programming, and who said it: a new one each hour, or on demand. */
  // The grid's props: none read, the snapshot has nothing for it.
  let {}: BlockProps<never> = $props()
  const QUOTES: [string, string][] = [
    ['Programs must be written for people to read, and only incidentally for machines to execute.', 'Harold Abelson'],
    ['There are only two hard things in Computer Science: cache invalidation and naming things.', 'Phil Karlton'],
    ['Premature optimization is the root of all evil.', 'Donald Knuth'],
    ['Simplicity is prerequisite for reliability.', 'Edsger W. Dijkstra'],
    ['Make it work, make it right, make it fast.', 'Kent Beck'],
    ['Any fool can write code that a computer can understand. Good programmers write code that humans can understand.', 'Martin Fowler'],
    ['Debugging is twice as hard as writing the code in the first place.', 'Brian Kernighan'],
    ['The best code is no code at all.', 'Jeff Atwood'],
    ['Talk is cheap. Show me the code.', 'Linus Torvalds'],
    ['Walking on water and developing software from a specification are easy if both are frozen.', 'Edward V. Berard'],
    ['If debugging is the process of removing bugs, then programming must be the process of putting them in.', 'Edsger W. Dijkstra'],
    ['Measuring programming progress by lines of code is like measuring aircraft building progress by weight.', 'Bill Gates'],
    ['The most dangerous phrase in the language is: we’ve always done it this way.', 'Grace Hopper'],
    ['First, solve the problem. Then, write the code.', 'John Johnson'],
    ['Code is like humor. When you have to explain it, it’s bad.', 'Cory House'],
    ['Deleted code is debugged code.', 'Jeff Sickel'],
    ['Weeks of coding can save you hours of planning.', t('Unknown')],
    ['It’s not a bug — it’s an undocumented feature.', t('Anonymous')],
    ['The function of good software is to make the complex appear to be simple.', 'Grady Booch'],
    // Written in French: the original, in French.
    [t('Perfection is achieved not when there is nothing more to add, but when there is nothing left to take away.'), 'Antoine de Saint-Exupéry'],
    ['Simple things should be simple, complex things should be possible.', 'Alan Kay'],
    ['Without requirements or design, programming is the art of adding bugs to an empty text file.', 'Louis Srygley'],
    ['Testing shows the presence, not the absence of bugs.', 'Edsger W. Dijkstra'],
    ['Good judgment comes from experience, and experience comes from bad judgment.', 'Fred Brooks'],
    ['Adding manpower to a late software project makes it later.', 'Fred Brooks'],
  ]
  const hourly = () => Math.floor(Date.now() / 3_600_000) % QUOTES.length
  let i = $state(hourly())
  onMount(() => {
    const timer = setInterval(() => (i = hourly()), 60_000)
    return () => clearInterval(timer)
  })
  function another() {
    let n = i
    while (n === i) n = Math.floor(Math.random() * QUOTES.length)
    i = n
  }
  const [text, author] = $derived(QUOTES[i])
</script>

<section class="panel" style="--src: var(--violet)" class:collapsed={isCollapsed('quotes')}>
  <h2>
    <span class="title">{t('Quotes')}</span>
    <BlockControls id="quotes" />
  </h2>
  <div class="body">
    <blockquote>“{text}”</blockquote>
    <p class="who">— {author}</p>
    <div class="acts">
      <button onclick={another}>{t('Another')}</button>
      <button onclick={() => copy(`“${text}” — ${author}`, t('Quote copied'))}>{t('Copy')}</button>
    </div>
  </div>
</section>

<style>
  .body { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 6px; padding: 14px 16px; text-align: center; overflow: auto; }
  blockquote { margin: 0; font-size: 15px; font-style: italic; line-height: 1.4; }
  .who { margin: 0 0 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  .acts { display: flex; gap: 6px; }
  .acts button { padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
</style>
