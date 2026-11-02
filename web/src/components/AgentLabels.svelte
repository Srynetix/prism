<script lang="ts">
  import { armer } from '../armed.svelte'
  import Icon from './Icon.svelte'
  import { live } from '../live.svelte'
  import { agentsSettings, kindColor, relabelAgents, type AgentLabel } from '../agents.svelte'
  import { kindIcon } from '../agents-list'
  import { saveEdits } from '../settings.svelte'
  import type { AgentKind } from '../bindings/AgentKind'
  import type { Agent } from '../types'
  import { t, tn } from '../i18n.svelte'

  /**
   * Agents view › Manage: the kinds and suggested tags of `[agents]`, edited in place and saved to
   * config.toml at once (as Settings › Agents › Agent would). A rename carries the agents along; a
   * kind removed stays on the agents carrying it until they change. A tag only in use (not
   * suggested) can be kept as a suggestion, or taken off every agent.
   */
  let { onclose }: { onclose: () => void } = $props()

  const agents = $derived<Agent[]>(live.snapshot?.agents ?? [])
  const carriedBy = (kind: AgentLabel, name: string) => agents.filter((a) => (kind === 'category' ? a.category === name : (a.tags ?? []).includes(name))).length

  /** The lists as edited here, ahead of the snapshot that brings them back once saved. */
  // Once each, even if config.toml repeats a name: they key the rows.
  const once = (list: string[] | undefined) => [...new Set(list ?? [])]
  const onceKinds = (list: AgentKind[] | undefined) => (list ?? []).filter((k, i, all) => k.name && all.findIndex((x) => x.name === k.name) === i)
  const fromSettings = () => ({ kinds: onceKinds(agentsSettings().kinds), tags: once(agentsSettings().tags) })
  let lists = $state(fromSettings())
  let busy = $state(false)
  let error = $state('')
  // Taken again only when the saved lists change (another window, Settings): not back to the old ones before the snapshot catches up.
  // svelte-ignore state_referenced_locally
  let seen = JSON.stringify(lists)
  $effect(() => {
    const next = fromSettings()
    const key = JSON.stringify(next)
    if (key !== seen) {
      seen = key
      lists = next
    }
  })
  /** Tags carried by an agent but not suggested: shown after the list, to keep or to take off. */
  const usedOnly = $derived([...new Set(agents.flatMap((a) => a.tags ?? []))].filter((tag) => !lists.tags.includes(tag)).sort((a, b) => a.localeCompare(b)))

  async function save(edit: { kinds: AgentKind[] } | { tags: string[] }, relabel?: { kind: AgentLabel; from: string; to: string }) {
    busy = true
    error = ''
    const [key, value] = 'kinds' in edit ? (['kinds', (lists.kinds = edit.kinds)] as const) : (['tags', (lists.tags = edit.tags)] as const)
    try {
      if (relabel) await relabelAgents(relabel.kind, relabel.from, relabel.to)
      await saveEdits([{ path: ['agents', key], value }])
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  const moved = <T,>(list: T[], i: number, by: number) => {
    const out = [...list]
    const [x] = out.splice(i, 1)
    out.splice(i + by, 0, x)
    return out
  }

  function renameKind(i: number, input: HTMLInputElement) {
    const from = lists.kinds[i].name
    const to = input.value.trim()
    if (!to || to === from || lists.kinds.some((k) => k.name === to)) return void (input.value = from)
    void save({ kinds: lists.kinds.map((k, n) => (n === i ? { ...k, name: to } : k)) }, { kind: 'category', from, to })
  }
  function setWhen(i: number, input: HTMLInputElement) {
    const when = input.value.trim()
    if (when !== lists.kinds[i].when) void save({ kinds: lists.kinds.map((k, n) => (n === i ? { ...k, when } : k)) })
  }
  function renameTag(i: number, input: HTMLInputElement) {
    const from = lists.tags[i]
    const to = input.value.trim()
    if (!to || to === from || lists.tags.includes(to)) return void (input.value = from)
    void save({ tags: lists.tags.map((x, n) => (n === i ? to : x)) }, { kind: 'tag', from, to })
  }
  let adding = $state({ kind: '', tag: '' })
  function addKind() {
    const name = adding.kind.trim()
    adding.kind = ''
    if (name && !lists.kinds.some((k) => k.name === name)) void save({ kinds: [...lists.kinds, { name, when: '' }] })
  }
  function addTag() {
    const name = adding.tag.trim()
    adding.tag = ''
    if (name && !lists.tags.includes(name)) void save({ tags: [...lists.tags, name] })
  }
  /** Taking a tag off every agent: a first click arms the button, a second one within 3 s does it. */
  const arm = armer()
  function takeOff(tag: string) {
    if (!arm.confirm(tag)) return
    busy = true
    relabelAgents('tag', tag, '').catch((e) => (error = String(e))).finally(() => (busy = false))
  }
</script>

<div class="manage">
  <div class="hd">
    <b>{t('Kinds and tags')}</b>
    <span class="grow"></span>
    <button class="x" onclick={onclose} title={t('Back to the agents (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
  </div>
  {#if error}<p class="err">{error}</p>{/if}
  <section>
    <h4>{t('Kinds')}<small>{t('What an agent does, one each; groups and colours in this order. Guessed at launch from the command its prompt starts with.')}</small></h4>
    {#each lists.kinds as k, i (k.name)}
      {@const n = carriedBy('category', k.name)}
      <div class="row kind">
        <span class="sw" style:--k={kindColor(k.name)} title={t('Its colour, from its place in the list')}><Icon name={kindIcon(k.name)} size={11} /></span>
        <input value={k.name} spellcheck="false" disabled={busy} aria-label={t('Kind: {name}', { name: k.name })} onchange={(e) => renameKind(i, e.currentTarget)} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
        <span class="n" title={tn(n, '{n} agent carries it', '{n} agents carry it')}>{n || ''}</span>
        <button onclick={() => save({ kinds: moved(lists.kinds, i, -1) })} disabled={busy || i === 0} title={t('Move up')} aria-label={t('Move {name} up', { name: k.name })}><Icon name="up" /></button>
        <button onclick={() => save({ kinds: moved(lists.kinds, i, 1) })} disabled={busy || i === lists.kinds.length - 1} title={t('Move down')} aria-label={t('Move {name} down', { name: k.name })}><Icon name="down" /></button>
        <button class="rm" onclick={() => save({ kinds: lists.kinds.filter((x) => x.name !== k.name) })} disabled={busy} title={t('Remove from the list (the agents carrying it keep it until changed)')} aria-label={t('Remove {name}', { name: k.name })}><Icon name="close" /></button>
        <input class="when" value={k.when} spellcheck="false" disabled={busy} placeholder={t('its commands: refine, /review…')} title={t('The commands that make an agent of this kind when its prompt starts with one, with or without their plugin (refine matches /acme-flow:refine)')} aria-label={t('Commands of {name}', { name: k.name })} onchange={(e) => setWhen(i, e.currentTarget)} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
      </div>
    {/each}
    <form class="row add" onsubmit={(e) => { e.preventDefault(); addKind() }}>
      <input bind:value={adding.kind} placeholder={t('Add a kind…')} spellcheck="false" disabled={busy} />
      <button type="submit" disabled={busy || !adding.kind.trim()} title={t('Add it at the end')} aria-label={t('Add')}><Icon name="plus" /></button>
    </form>
  </section>
  <section>
    <h4>{t('Tags')}<small>{t('Free, several per agent; these are suggested, besides any typed one.')}</small></h4>
    {#each lists.tags as name, i (name)}
      {@const n = carriedBy('tag', name)}
      <div class="row">
        <input value={name} spellcheck="false" disabled={busy} aria-label={t('Tag: {name}', { name })} onchange={(e) => renameTag(i, e.currentTarget)} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
        <span class="n" title={tn(n, '{n} agent carries it', '{n} agents carry it')}>{n || ''}</span>
        <button onclick={() => save({ tags: moved(lists.tags, i, -1) })} disabled={busy || i === 0} title={t('Move up')} aria-label={t('Move {name} up', { name })}><Icon name="up" /></button>
        <button onclick={() => save({ tags: moved(lists.tags, i, 1) })} disabled={busy || i === lists.tags.length - 1} title={t('Move down')} aria-label={t('Move {name} down', { name })}><Icon name="down" /></button>
        <button class="rm" onclick={() => save({ tags: lists.tags.filter((x) => x !== name) })} disabled={busy} title={t('No longer suggested (the agents keep it)')} aria-label={t('Remove {name}', { name })}><Icon name="close" /></button>
      </div>
    {/each}
    {#each usedOnly as tag (tag)}
      {@const n = carriedBy('tag', tag)}
      <div class="row used">
        <span class="name" title={t('In use, not suggested')}>{tag}</span>
        <span class="n">{n}</span>
        <button onclick={() => save({ tags: [...lists.tags, tag] })} disabled={busy} title={t('Suggest it too')}>{t('Keep')}</button>
        <button class="rm" class:armed={arm.is(tag)} onclick={() => takeOff(tag)} disabled={busy} title={tn(n, 'Take it off the {n} agent carrying it (click twice)', 'Take it off the {n} agents carrying it (click twice)')}>{arm.is(tag) ? t('Take off?') : t('Take off')}</button>
      </div>
    {/each}
    <form class="row add" onsubmit={(e) => { e.preventDefault(); addTag() }}>
      <input bind:value={adding.tag} placeholder={t('Add a tag…')} spellcheck="false" disabled={busy} />
      <button type="submit" disabled={busy || !adding.tag.trim()} title={t('Add it at the end')} aria-label={t('Add')}><Icon name="plus" /></button>
    </form>
  </section>
  <p class="foot">{t('Saved to')} <code>[agents]</code> {t('in config.toml at once.')}</p>
</div>

<style>
  .manage { flex: 1; min-height: 0; overflow: auto; padding: 6px 10px 12px; }
  .hd { display: flex; align-items: center; gap: 6px; padding: 4px 0 6px; }
  .hd b { font-size: var(--fs-body); }
  .grow { flex: 1; }
  section { margin-top: 10px; }
  h4 { display: flex; flex-direction: column; gap: 1px; margin: 0 0 5px; font-size: var(--fs-meta); font-weight: 600; }
  h4 small { font-weight: 400; color: var(--text-muted); font-size: var(--fs-tag); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) 22px 22px 22px auto; gap: 3px; align-items: center; margin-bottom: 3px; }
  .row.kind { grid-template-columns: auto minmax(0, 1fr) 22px 22px 22px auto; }
  .sw { display: inline-flex; align-items: center; justify-content: center; width: 20px; height: 20px; border-radius: var(--r5); background: color-mix(in srgb, var(--k) 14%, transparent); color: var(--k); }
  .row.kind .when { grid-column: 1 / -1; height: 22px; font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); margin-bottom: 4px; }
  .row.used { grid-template-columns: minmax(0, 1fr) 22px auto auto; }
  .row.add { grid-template-columns: minmax(0, 1fr) 24px; }
  input { min-width: 0; height: 24px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); outline: none; }
  input:focus { border-color: var(--src-claude); }
  .name { padding: 0 7px; color: var(--text-muted); font-size: var(--fs-meta); font-style: italic; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .n { text-align: center; color: var(--text-muted); font-size: var(--fs-tag); }
  button { display: inline-flex; align-items: center; justify-content: center; height: 22px; min-width: 22px; padding: 0 6px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  button:hover:not(:disabled) { color: var(--text); border-color: var(--border); }
  button:disabled { opacity: 0.4; cursor: default; }
  .rm:hover:not(:disabled), .rm.armed { color: var(--ko); border-color: var(--ko); }
  .x { border: none; }
  .err { margin: 0 0 6px; padding: 5px 8px; border-radius: var(--r6); background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); }
  .foot { margin: 12px 0 0; color: var(--text-muted); font-size: var(--fs-tag); }
  code { font-family: var(--mono); }
</style>
