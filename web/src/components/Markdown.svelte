<script lang="ts">
  import { forgeLink, onGithubWeb } from '../forge'
  import { projectOf, withEmoji } from '../emoji.svelte'
  import CustomEmoji from './CustomEmoji.svelte'
  import { inline, parse, slug, withMentions, type Block, type MdMarks, type MentionMarks, type Span } from '../markdown'
  import { canPreview, openTicket } from '../ticket-view.svelte'
  import { hasCore } from '../bridge'
  import { knownMr, withMrs, type MrOption, type MrPlace } from '../linkify'
  import { live } from '../live.svelte'
  import { openMrOptions } from '../mr-choice.svelte'
  import { mrKey } from '../mr-ref'
  import RemoteImage from './RemoteImage.svelte'
  import Icon from './Icon.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * ClickUp Markdown rendered as Svelte elements (see markdown.ts): no HTML from the
   * ticket is injected. With `plain`, only inline markup is read (comments).
   * `forge`: the project's web address, for a forge's text (MRs, comments); its attached
   * files (`/uploads/…`) are resolved against it, its images read through the API, and its
   * emoji shortcodes (`:thumbsup:`) drawn, by that forge's names (a GitLab group's custom ones too).
   * `parsed`: the blocks, already parsed from `text`. `marks`: the preview of a file in a diff,
   * its changed blocks marked in the margin, lit, and leading to their lines in the diff.
   * `onaction`: runs a `prism:` link (in-app Help); without it, such a link is plain text.
   * `anchors`: headings get an id (`slug`), for a table of contents.
   * `images`: the files shipped with the app, by relative path (`images/x.png` → its URL), for the
   * images of in-app Help; without it, or for a path it lacks, such an image stays its alt text.
   * With `onimage`, a click on such an image calls it with its URL.
   * `mrs`: an agent's words — the MRs they name (`!1957`) open their panel, as in its Terminal;
   * their images are links, never fetched. `scope`: the repositories that agent works in
   * (`mrScopeOf`), whose MRs its `!1957` are.
   * `mention`: a comment's mentions drawn as chips, mine lit (see `MentionMarks`).
   */
  let {
    text, plain = false, onimage, forge, parsed, marks, onaction, anchors = false, images, reading = false, mrs = false, scope = null, mention,
  }: {
    text: string
    plain?: boolean
    onimage?: (url: string, alt: string) => void
    forge?: string
    parsed?: Block[]
    marks?: MdMarks
    onaction?: (target: string) => void
    anchors?: boolean
    images?: Record<string, string>
    /** Long reading (Help, a page of an agent's folder): larger text, more room; otherwise the panels' size. */
    reading?: boolean
    mrs?: boolean
    scope?: MrPlace[] | null
    mention?: MentionMarks
  } = $props()

  /** Relative link (uploaded file): to the GitLab project, otherwise no link. */
  const resolve = (href: string) => (href.startsWith('/') ? (forge ? forgeLink(forge, href) : undefined) : href)

  const blocks = $derived(parsed ?? (plain ? [{ kind: 'paragraph' as const, spans: inline(text) }] : parse(text)))

  const CALLOUT = { tip: t('Tip'), note: t('Note'), warning: t('Careful') }

  /** Attributes of a top-level block changed by the diff: its mark, and what leads to its lines. */
  function at(top: boolean, i: number): Record<string, unknown> {
    const m = top && marks ? marks.of(i) : null
    if (!m || !marks) return {}
    const lit = marks.lit && 'block' in marks.lit && marks.lit.block === i
    return {
      'data-chg': m,
      'data-lit': lit ? '' : undefined,
      title: m === 'add' ? t('Added · click: its lines in the diff') : t('Changed · click: its lines in the diff'),
      onmouseenter: () => marks.onhover({ block: i }),
      onmouseleave: () => marks.onhover(null),
      onclick: (e: MouseEvent) => {
        if (!(e.target as HTMLElement).closest('a, button, summary')) marks.onpick({ block: i })
      },
    }
  }

  /** Under a link, its left edge: where its choice of MRs opens, by mouse or keyboard. */
  const below = (el: HTMLElement) => {
    const r = el.getBoundingClientRect()
    return { x: r.left, y: r.bottom }
  }

  /** One MR: its key and title, or where it opens; several: the choice to come. */
  function mrTitle(options: MrOption[], text: string): string {
    if (options.length > 1) return t('{mr}: in which repository?', { mr: text })
    const o = options[0]
    const known = knownMr(o, live.snapshot)
    if (known) return `${mrKey(o.ref)} · ${known.title}`
    return o.url ? t('Open {name} on its forge', { name: mrKey(o.ref) }) : t('Open {name}', { name: text })
  }
</script>

{#snippet spans(list: Span[])}
  {@const shown = forge ? withEmoji(list, onGithubWeb(forge), projectOf(forge)) : mrs && hasCore() ? withMrs(list, scope) : list}
  {#each mention ? withMentions(shown, mention) : shown as s, i (i)}
    {#if s.mention}<span class="mention" class:me={s.mention === 'me'} class:b={s.bold} class:i={s.italic}>{s.text}</span>
    {:else if s.emoji}<CustomEmoji url={s.emoji} name={s.text} />
    {:else if s.local}{#if images?.[s.image!] && onimage}<button class="local zoom" onclick={() => onimage(images[s.image!], s.text)} title={t('{text} · click to enlarge', { text: s.text })} style:max-width={s.width ? `min(${s.width}px, 100%)` : undefined}><img src={images[s.image!]} alt={s.text} /></button>{:else if images?.[s.image!]}<img class="local" src={images[s.image!]} alt={s.text} title={s.text} style:max-width={s.width ? `min(${s.width}px, 100%)` : undefined} />{:else}<span><Icon name="image" /> {s.text}</span>{/if}
    {:else if s.image}<span class="inline-img"><RemoteImage url={s.image} alt={s.text} width={s.width} {forge} fetched={!mrs} onopen={onimage ? () => onimage(s.image!, s.text) : undefined} /></span>
    {:else if s.action && onaction}<button class="action" onclick={() => onaction(s.action!)}>{s.text}</button>
    {:else if s.mr !== undefined}<button class="ticket" onclick={(e) => openMrOptions(s.mr!, below(e.currentTarget))} title={mrTitle(s.mr, s.text)}>{s.text}</button>
    {:else if s.ticket && canPreview()}<button class="ticket" onclick={() => openTicket(s.ticket!)} title={t('Preview {ticket}', { ticket: s.ticket })}>{s.text}</button>
    {:else if s.href && resolve(s.href)}<a href={resolve(s.href)} target="_blank" rel="noreferrer" title={resolve(s.href)} class:b={s.bold} class:i={s.italic} class:s={s.strike} class:c={s.code}>{#if s.icon}<Icon name={s.icon} /> {/if}{s.text}</a>
    {:else if s.code}<code>{s.text}</code>
    {:else}<span class:b={s.bold} class:i={s.italic} class:s={s.strike}>{#if s.icon}<Icon name={s.icon} /> {/if}{s.text}</span>{/if}
  {/each}
{/snippet}

<!-- Lines removed by the diff before a block: a marker leading to them. -->
{#snippet gone(i: number)}
  {@const g = marks?.gone.get(i)}
  {#if marks && g}
    <button class="gone" data-lit={marks.lit && 'gone' in marks.lit && marks.lit.gone === i ? '' : undefined} title={g.text} onmouseenter={() => marks.onhover({ gone: i })} onmouseleave={() => marks.onhover(null)} onclick={() => marks.onpick({ gone: i })}>− {tn(g.count, '{n} line removed', '{n} lines removed')}</button>
  {/if}
{/snippet}

{#snippet render(list: Block[], top: boolean)}
  {#each list as b, i (i)}
    {#if top}{@render gone(i)}{/if}
    {#if b.kind === 'heading'}
      <p class="h h{Math.min(b.level, 3)}" id={anchors ? slug(b.spans) : undefined} {...at(top, i)}>{@render spans(b.spans)}</p>
    {:else if b.kind === 'paragraph'}
      <p {...at(top, i)}>{@render spans(b.spans)}</p>
    {:else if b.kind === 'quote'}
      <blockquote class:blocks={b.blocks} {...at(top, i)}>{#if b.blocks}{@render render(b.blocks, false)}{:else}{@render spans(b.spans)}{/if}</blockquote>
    {:else if b.kind === 'callout'}
      <div class="callout {b.tone}" {...at(top, i)}><b>{CALLOUT[b.tone]}</b><span class:blocks={b.blocks}>{#if b.blocks}{@render render(b.blocks, false)}{:else}{@render spans(b.spans)}{/if}</span></div>
    {:else if b.kind === 'code'}
      <pre {...at(top, i)}>{b.text}</pre>
    {:else if b.kind === 'rule'}
      <hr {...at(top, i)} />
    {:else if b.kind === 'list'}
      <ul class:ordered={b.ordered} {...at(top, i)}>
        {#each b.items as it, n (n)}
          <li style="--depth: {it.depth}" class:task={it.checked !== null} class:checked={it.checked}>
            {#if it.checked !== null}<span class="box"><Icon name={it.checked ? 'checked' : 'unchecked'} /></span>{:else}<span class="bullet" aria-hidden="true">{it.number !== undefined ? `${it.number}.` : '•'}</span>{/if}
            <span class="item">{@render spans(it.spans)}{#if it.blocks}{@render render(it.blocks, false)}{/if}</span>
          </li>
        {/each}
      </ul>
    {:else if b.kind === 'details'}
      <details open={b.open} {...at(top, i)}>
        <summary>{@render spans(b.summary)}</summary>
        <div class="inner">{@render render(b.blocks, false)}</div>
      </details>
    {:else if b.kind === 'table'}
      <div class="table" {...at(top, i)}>
        <table><tbody>
          {#each b.rows as row, r (r)}
            <tr>{#each row as cell, c (c)}{#if r === 0}<th>{@render spans(cell)}</th>{:else}<td>{@render spans(cell)}</td>{/if}{/each}</tr>
          {/each}
        </tbody></table>
      </div>
    {/if}
  {/each}
  {#if top}{@render gone(list.length)}{/if}
{/snippet}

<div class="md" class:doc={reading}>{@render render(blocks, true)}</div>

<style>
  /* Panels (tickets, MRs, comments): the interface's size, with air between blocks and lines. */
  .md { font-size: var(--fs-body); line-height: 1.7; overflow-wrap: anywhere; }
  .md > :first-child { margin-top: 0; }
  .md > :last-child { margin-bottom: 0; }
  p { margin: 0 0 12px; white-space: pre-wrap; }
  .h { font-weight: 600; line-height: 1.3; white-space: normal; }
  .h1 { font-size: 16px; margin: 24px 0 10px; padding-bottom: 6px; border-bottom: 1px solid var(--border); }
  .h2 { font-size: 14px; margin: 22px 0 8px; }
  .h3 { font-size: var(--fs-title); margin: 18px 0 6px; }
  .b { font-weight: 600; }
  .i { font-style: italic; }
  .s { text-decoration: line-through; }
  code, a.c { font-family: var(--mono); font-size: var(--fs-mono); }
  code { padding: 1px 5px; border-radius: var(--r4); background: var(--surface-muted); }
  a:not(.action) { text-decoration: underline; text-decoration-color: color-mix(in srgb, currentColor 35%, transparent); text-underline-offset: 2px; }
  pre { margin: 4px 0 14px; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface-muted); font: var(--fs-mono)/1.6 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  blockquote { margin: 4px 0 14px; padding: 6px 12px; border-left: 3px solid color-mix(in srgb, var(--accent) 50%, var(--border)); border-radius: 0 var(--r6) var(--r6) 0; background: color-mix(in srgb, var(--surface-muted) 60%, transparent); color: var(--text-muted); white-space: pre-wrap; }
  hr { border: none; border-top: 1px solid var(--border); margin: 22px 0; }
  ul { list-style: none; margin: 0 0 12px; padding: 0; }
  li { display: flex; gap: 8px; margin: 0 0 5px; padding-left: calc(var(--depth) * 18px + 6px); }
  .bullet { flex-shrink: 0; min-width: 10px; color: var(--accent); font-weight: 700; }
  .ordered .bullet { min-width: 16px; color: var(--text-muted); font-weight: 500; font-variant-numeric: tabular-nums; }
  .box { flex-shrink: 0; color: var(--text-muted); }
  li.checked > span:last-child { color: var(--text-muted); text-decoration: line-through; }
  /* An item's text, then what it holds under it (a code block): the column's width at most. */
  .item { min-width: 0; }
  .item > pre { margin: 6px 0 8px; }
  .item > p { margin: 6px 0 0; }
  .inline-img { display: block; margin: 6px 0; }
  /* An image shipped with the app (Help): the column's width at most, framed like a screenshot. */
  .local { display: block; max-width: 100%; height: auto; margin: 6px 0 10px; border: 1px solid var(--border); border-radius: var(--r8); }
  .zoom { padding: 0; overflow: hidden; background: none; cursor: zoom-in; }
  .zoom img { display: block; max-width: 100%; height: auto; }
  .ticket { border: none; background: none; padding: 0; font: inherit; color: var(--info); cursor: pointer; }
  .ticket:hover { text-decoration: underline; }
  /* Someone mentioned: a chip; me, lit. */
  .mention { padding: 0 4px; border-radius: var(--r4); background: color-mix(in srgb, var(--info) 12%, transparent); color: var(--info); font-weight: 500; white-space: nowrap; }
  .mention.me { background: color-mix(in srgb, var(--warn) 22%, transparent); color: var(--text); }
  .table { overflow-x: auto; margin: 4px 0 14px; }
  table { border-collapse: collapse; font-size: var(--fs-meta); }
  /* `break-word`, not the `anywhere` around it: a cell's narrowest is its longest word, so no column is squeezed to a letter (the table scrolls instead). */
  th, td { border-bottom: 1px solid var(--border); padding: 6px 12px; text-align: left; vertical-align: top; overflow-wrap: break-word; }
  th { border-bottom-width: 2px; color: var(--text-muted); font-weight: 600; }
  tr:nth-child(even) td { background: color-mix(in srgb, var(--surface-muted) 45%, transparent); }
  /* Collapsible block (GitLab `<details>`). */
  details { margin: 0 0 14px; border: 1px solid var(--border); border-radius: var(--r6); }
  summary { padding: 5px 10px; cursor: pointer; font-weight: 500; }
  summary:hover { background: var(--surface-muted); }
  details[open] > summary { border-bottom: 1px solid var(--border); }
  .inner { padding: 10px 12px 2px; }
  /*
   * Preview in a diff: a bar in the margin for a changed block (green: added, orange:
   * changed), outside the text (negative margin), so even a scrolling table keeps it.
   */
  [data-chg] { margin-left: -10px; padding-left: 7px; border-left: 3px solid var(--warn); border-radius: var(--r2); cursor: pointer; }
  [data-chg='add'] { border-left-color: var(--ok); }
  [data-lit] { outline: 2px solid color-mix(in srgb, var(--accent) 45%, transparent); outline-offset: 2px; }
  /* In-app Help: callouts, and `prism:` links as buttons that act in the app. */
  .callout { display: flex; gap: 10px; margin: 4px 0 14px; padding: 8px 12px; border-radius: var(--r8); white-space: pre-wrap; }
  .callout b { flex-shrink: 0; }
  /* A quote or callout holding a code block: its blocks, the text in them wrapping as theirs, the first and last flush. */
  .blocks { min-width: 0; white-space: normal; }
  .blocks > :first-child { margin-top: 0; }
  .blocks > :last-child { margin-bottom: 0; }
  .callout span { color: var(--text); }
  .callout.tip { background: var(--info-bg); color: var(--info); }
  .callout.note { background: var(--surface-muted); color: var(--text-muted); }
  .callout.warning { background: var(--warn-bg); color: var(--warn); }
  .action { display: inline-flex; align-items: center; margin: 1px 2px; padding: 1px 9px; border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border)); border-radius: var(--r6); background: var(--surface); color: var(--accent); font: inherit; font-weight: 500; cursor: pointer; }
  .action:hover { background: color-mix(in srgb, var(--accent) 10%, var(--surface)); }
  .gone { display: block; width: calc(100% + 10px); margin: 0 0 8px -10px; padding: 1px 8px; border: none; border-left: 3px solid var(--ko); border-radius: 0 var(--r4) var(--r4) 0; background: var(--ko-bg); color: var(--ko); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  /* Long reading (`reading`: Help, an agent's pages): larger text, larger headings, more room. */
  .md.doc { font-size: 13.5px; line-height: 1.75; }
  .doc p { margin-bottom: 14px; }
  .doc .h { font-weight: 650; letter-spacing: -0.005em; }
  .doc .h1 { font-size: 20px; margin: 30px 0 12px; padding-bottom: 0; border-bottom: none; }
  .doc .h2 { font-size: 16px; margin: 28px 0 10px; padding-bottom: 5px; border-bottom: 1px solid var(--border); }
  .doc .h3 { font-size: 14px; margin: 22px 0 8px; }
  .doc code, .doc a.c { font-size: 12.5px; }
  .doc pre { margin: 6px 0 18px; padding: 12px 16px; border: none; font-size: 12.5px; }
  .doc blockquote { margin: 6px 0 18px; padding: 8px 16px; border-left-color: var(--accent); border-radius: 0 var(--r8) var(--r8) 0; background: color-mix(in srgb, var(--accent) 7%, transparent); color: var(--text); }
  .doc hr { margin: 28px 0; }
  .doc ul { margin-bottom: 14px; }
  .doc li { margin-bottom: 6px; }
  .doc .table { margin: 6px 0 18px; }
  .doc table { font-size: 12.5px; }
  .doc th, .doc td { padding: 7px 14px; }
  .doc .callout, .doc details { margin-bottom: 18px; }
</style>
