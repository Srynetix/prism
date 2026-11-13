<script lang="ts">
  import Icon from '../Icon.svelte'
  import Markdown from '../Markdown.svelte'
  import { t } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import { toast } from '../../toast.svelte'
  import { conventions } from '../../conventions'
  import { openTicket } from '../../ticket-view.svelte'
  import { createTicket, defaultLane, openNewTicket, ticketBoards } from '../../new-ticket.svelte'
  import { launchNow, openAgents, openLauncher, type Launch } from '../../agents.svelte'
  import { takeImplementationOf } from '../../claude-actions'
  import { addNote } from '../../notes.svelte'
  import { openSettings } from '../../settings.svelte'
  import { repoAdd } from '../../clones.svelte'
  import { answerProposal, awaitRepo, factKindLabel, saveFact } from '../../assistant.svelte'
  import type { AssistantProposal } from '../../bindings/AssistantProposal'
  import type { ConversationTool } from '../../bindings/ConversationTool'

  /**
   * A proposal of Prism's assistant in its Chat (`assistant_propose_*`): while it waits, its card —
   * a ticket, an agent to launch, a note, a repository to add, a fact to remember — with what applies
   * it in a click, what changes it first, and Dismiss (a reason going back to the assistant); once
   * answered, a line saying what became of it.
   */
  let { call, proposal }: { call: ConversationTool; proposal: AssistantProposal | undefined } = $props()

  let busy = $state(false)
  let refusing = $state(false)
  let reason = $state('')
  let editing = $state(false)
  let fact = $state('')
  let promptOpen = $state(false)

  type Outcome = { status: string; key?: string; url?: string; agent?: string; repo?: string; title?: string; fact?: string; reason?: string }
  const outcome = $derived.by((): Outcome | null => {
    if (!call.output) return null
    try {
      const o = JSON.parse(call.output)
      return typeof o?.status === 'string' ? o : null
    } catch {
      return null
    }
  })

  const HEAD: Record<string, string> = { ticket: t('New ticket'), launch: t('Launch an agent'), note: t('New note'), repo: t('Add the repository'), memory: t('Remember?') }
  const boards = $derived(ticketBoards())
  const board = $derived(boards[0])
  const lane = $derived(defaultLane(board, conventions().pick_statuses ?? []))

  async function act(f: () => Promise<void>) {
    if (busy) return
    busy = true
    try {
      await f()
    } catch (e) {
      toast(String(e), 8000)
    } finally {
      busy = false
    }
  }

  const tell = (status: string, detail: Record<string, unknown> = {}) => (proposal ? answerProposal(proposal.id, status, detail) : Promise.resolve())

  // ---- Ticket ----
  const ticketText = (p: AssistantProposal) => (p.ticket_type ? `${p.text}\n\n${t('Type')}: ${p.ticket_type}` : p.text)
  const create = () =>
    act(async () => {
      const p = proposal!
      if (!board) throw new Error(t('The tracker offers no board to create a ticket on.'))
      const key = await createTicket({ board: board.id, status: lane, title: p.title, description: ticketText(p), assignMe: false })
      await tell('created', { key })
      toast(t('{key} created', { key }), 4000, { label: t('Open'), run: () => openTicket(key) })
    })
  const editTicket = () => {
    const p = proposal!
    openNewTicket({ title: p.title, description: ticketText(p) }, (key, d) => void tell('created', { key, title: d.title, note: 'the user changed it before creating it' }))
  }

  // ---- Launch ----
  const launchOf = (p: AssistantProposal): Launch => {
    const l = p.launch!
    return {
      title: l.title,
      ...(l.cwd ? { cwd: l.cwd } : {}),
      prompt: l.prompt ?? p.text,
      ...(l.worktree ? { worktree: { repo: l.worktree.repo ?? undefined, name: l.worktree.name ?? undefined, base: l.worktree.base ?? undefined } } : {}),
      ...(l.category ? { category: l.category } : {}),
      then: l.then,
      from: 'Assistant',
    }
  }
  const launch = () =>
    act(async () => {
      const p = proposal!
      const id = await launchNow(launchOf(p))
      if (!id) return
      await tell('launched', { agent: id, title: p.title })
      const take = p.ticket ? takeImplementationOf(p.ticket) : undefined
      if (take) await take().catch((e) => toast(String(e), 8000))
    })
  const inLauncher = () => {
    const p = proposal!
    const take = p.ticket ? takeImplementationOf(p.ticket) : undefined
    openLauncher(launchOf(p), null, async () => {
      await tell('launched', { title: p.title, note: 'launched from the launcher, maybe changed' })
      if (take) await take()
    }, t('Assistant'))
  }

  // ---- Note, repository, fact ----
  const write = () =>
    act(async () => {
      const p = proposal!
      addNote(p.title && !p.text.startsWith(p.title) ? `${p.title}\n${p.text}` : p.text, 'sticky', { repo: p.repo ?? null })
      await tell('written')
    })
  const addRepo = () => {
    const p = proposal!
    awaitRepo(p.id, p.title)
    repoAdd.publish = ''
    repoAdd.source = p.source ?? ''
    openSettings('repos')
  }
  const remember = (text: string, modified = false) =>
    act(async () => {
      const p = proposal!
      await saveFact({ kind: p.fact_kind ?? 'project', text, source: t('after a question') })
      await tell('remembered', { fact: text, ...(modified ? { note: 'the user changed it' } : {}) })
      editing = false
    })

  const dismiss = () =>
    act(async () => {
      await tell('dismissed', reason.trim() ? { reason: reason.trim() } : {})
      refusing = false
    })

  /** A done one, in a line. */
  const doneLine = $derived.by(() => {
    const o = outcome
    if (!o) return null
    switch (o.status) {
      case 'created': return { icon: 'kanban', text: t('Ticket created · {key}', { key: o.key ?? '' }) }
      case 'launched': return { icon: 'claude', text: t('Agent launched · {title}', { title: o.title ?? call.what ?? '' }) }
      case 'added': return { icon: 'folder', text: t('Repository added · {repo}', { repo: o.repo ?? call.what ?? '' }) }
      case 'written': return { icon: 'edit', text: t('Note written') }
      case 'remembered': return { icon: 'sparkles', text: t('Remembered: {fact}', { fact: o.fact ?? call.what ?? '' }) }
      case 'dismissed': return { icon: 'close', text: o.reason ? t('Dismissed: {reason}', { reason: o.reason }) : t('Dismissed') }
      case 'dropped': return { icon: 'close', text: t('No answer: its conversation ended') }
      default: return null
    }
  })
  const kindOf = (name?: string | null) => name ?? t('Auto')
</script>

{#if proposal}
  {@const p = proposal}
  <div class="card" class:mem={p.action === 'memory'}>
    <div class="ch">
      <Icon name="sparkles" size={12} />
      <span>{HEAD[p.action]}</span>
      {#if p.repo}<span class="pill">{p.repo}</span>{/if}
      <span class="r">{t('nothing happens without your click')}</span>
    </div>
    <div class="cb">
      {#if p.why}<p class="why">{p.why}</p>{/if}
      {#if p.action === 'ticket'}
        <div class="ttl">{p.title}</div>
        <dl class="kv">
          {#if board}<dt>{t('Board')}</dt><dd>{board.name}{lane ? ` › ${lane}` : ''}</dd>{/if}
          {#if p.ticket_type}<dt>{t('Type')}</dt><dd>{p.ticket_type}</dd>{/if}
        </dl>
        <div class="desc"><Markdown text={p.text} /></div>
      {:else if p.action === 'launch' && p.launch}
        {@const l = p.launch}
        <div class="ttl">{l.title}</div>
        <dl class="kv">
          <dt>{t('Kind')}</dt><dd>{kindOf(l.category)}{#if p.ticket}<small> · {t('{ticket} taken on launch', { ticket: p.ticket })}</small>{/if}</dd>
          {#if l.worktree}<dt>{t('Worktree')}</dt><dd class="mono">{l.worktree.repo}/{l.worktree.name ?? l.title}{#if l.worktree.base}<small> {t('from {base}', { base: l.worktree.base })}</small>{/if}</dd>
          {:else if l.cwd}<dt>{t('Folder')}</dt><dd class="mono">{l.cwd}</dd>{/if}
          {#if l.then.length}<dt>{t('Then')}</dt><dd>{l.then.map((s) => (s.kind === 'mr' ? t('Draft MR') : s.kind === 'agent' ? t('Review') : s.kind)).join(' → ')}</dd>{/if}
          <dt>{t('Prompt')}</dt>
          <dd><button class="prompt" class:open={promptOpen} onclick={() => (promptOpen = !promptOpen)} title={promptOpen ? t('Fold') : t('Unfold')}>{l.prompt ?? p.text}</button></dd>
        </dl>
      {:else if p.action === 'note'}
        <div class="desc"><Markdown text={p.text} /></div>
      {:else if p.action === 'repo'}
        <div class="ttl">{p.title}</div>
        <dl class="kv">
          <dt>{t('Source')}</dt><dd class="mono">{p.source}</dd>
          {#if p.text}<dt>{t('What it is')}</dt><dd>{p.text}</dd>{/if}
        </dl>
      {:else if p.action === 'memory'}
        {#if editing}
          <!-- svelte-ignore a11y_autofocus -->
          <textarea bind:value={fact} rows="3" autofocus aria-label={t('Fact')}></textarea>
        {:else}
          <p class="fact"><span class="kind">{factKindLabel(p.fact_kind ?? 'project')}</span>{p.text}</p>
        {/if}
      {/if}
    </div>
    {#if refusing}
      <div class="ca">
        <!-- svelte-ignore a11y_autofocus -->
        <input bind:value={reason} placeholder={t('Why? (optional, the assistant reads it)')} autofocus onkeydown={(e) => { if (e.key === 'Enter') void dismiss(); if (e.key === 'Escape') { e.stopPropagation(); refusing = false } }} />
        <button class="btn" onclick={dismiss} disabled={busy}>{t('Dismiss')}</button>
        <button class="btn ghost" onclick={() => (refusing = false)}>{t('Cancel')}</button>
      </div>
    {:else}
      <div class="ca">
        {#if p.action === 'ticket'}
          <button class="btn pri" onclick={create} disabled={busy || !board}>{t('Create the ticket')}</button>
          <button class="btn" onclick={editTicket} disabled={busy || !board}>{t('Edit…')}</button>
        {:else if p.action === 'launch'}
          <button class="btn pri" onclick={launch} disabled={busy}>{t('Launch')}</button>
          <button class="btn" onclick={inLauncher} disabled={busy}>{t('Open in the launcher…')}</button>
        {:else if p.action === 'note'}
          <button class="btn pri" onclick={write} disabled={busy}>{t('Write the note')}</button>
        {:else if p.action === 'repo'}
          <button class="btn pri" onclick={addRepo} disabled={busy}>{t('Add…')}</button>
        {:else if p.action === 'memory'}
          {#if editing}
            <button class="btn pri" onclick={() => remember(fact.trim(), true)} disabled={busy || !fact.trim()}>{t('Remember')}</button>
            <button class="btn" onclick={() => (editing = false)}>{t('Cancel')}</button>
          {:else}
            <button class="btn pri" onclick={() => remember(p.text)} disabled={busy}>{t('Remember')}</button>
            <button class="btn" onclick={() => { fact = p.text; editing = true }}>{t('Edit…')}</button>
          {/if}
        {/if}
        <button class="btn ghost" onclick={() => (refusing = true)} disabled={busy}>{p.action === 'memory' ? t('No') : t('Dismiss')}</button>
        {#if p.action === 'repo'}<span class="r">{t('Settings › Repositories, prefilled: nothing is cloned without your click')}</span>{/if}
        {#if !board && p.action === 'ticket'}<span class="r ko">{t('The tracker offers no board to create a ticket on.')}</span>{/if}
      </div>
    {/if}
  </div>
{:else if doneLine && outcome}
  {@const o = outcome}
  <div class="done" class:ko={o.status === 'dismissed' || o.status === 'dropped'}>
    <Icon name={doneLine.icon as 'kanban'} size={12} />
    <span>{doneLine.text}</span>
    {#if o.status === 'created' && o.key}<button class="link" onclick={() => openTicket(o.key!)}>{t('Open')} →</button>{/if}
    {#if o.status === 'launched' && o.agent && live.snapshot?.agents.some((a) => a.id === o.agent)}<button class="link" onclick={() => openAgents(o.agent)}>{t('See the agent')} →</button>{/if}
  </div>
{:else if call.state === 'running'}
  <div class="done"><Icon name="sparkles" size={12} /><span>{t('Proposal')}: {call.what ?? ''}</span></div>
{:else if call.state === 'failed'}
  <div class="done ko"><Icon name="close" size={12} /><span>{t('Proposal refused by Prism')}{call.output ? `: ${call.output}` : ''}</span></div>
{/if}

<style>
  .card { --as: var(--info); max-width: 640px; border: 1px solid color-mix(in srgb, var(--as) 60%, var(--border)); border-radius: var(--r10, 10px); background: var(--surface); overflow: hidden; }
  .card.mem { border-style: dashed; }
  .ch { display: flex; align-items: center; gap: 7px; padding: 6px 11px; background: color-mix(in srgb, var(--as) 12%, var(--surface)); color: var(--as); font-weight: 700; font-size: var(--fs-meta); }
  .ch .r { margin-left: auto; font-weight: 400; color: var(--text-muted); }
  .pill { padding: 0 7px; border-radius: 9px; background: var(--surface); color: var(--text-muted); font-weight: 600; font-size: var(--fs-tag); }
  .cb { display: grid; gap: 7px; padding: 9px 12px; }
  .why { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .ttl { font-size: 13px; font-weight: 600; }
  .kv { display: grid; grid-template-columns: 86px minmax(0, 1fr); gap: 3px 10px; margin: 0; font-size: var(--fs-meta); }
  .kv dt { color: var(--text-muted); }
  .kv dd { margin: 0; min-width: 0; }
  .kv small { color: var(--text-muted); }
  .mono { font-family: var(--mono); font-size: var(--fs-tag); }
  .desc { max-height: 220px; overflow: auto; padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); font-size: var(--fs-meta); }
  .prompt { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; padding: 0; border: none; background: none; color: inherit; font: inherit; text-align: left; white-space: pre-wrap; cursor: pointer; }
  .prompt.open { display: block; }
  .fact { margin: 0; }
  .kind { margin-right: 6px; padding: 0 5px; border-radius: 4px; background: var(--surface-muted); color: var(--text-muted); font-size: 9.5px; text-transform: uppercase; letter-spacing: 0.03em; }
  textarea { width: 100%; box-sizing: border-box; font: inherit; }
  .ca { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 8px 12px; border-top: 1px solid var(--border); }
  .ca input { flex: 1; min-width: 160px; font: inherit; }
  .ca .r { margin-left: auto; color: var(--text-muted); font-size: var(--fs-tag); }
  .ca .r.ko { color: var(--ko); }
  .btn { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn.pri { border-color: var(--as); background: var(--as); color: var(--surface); }
  .btn.ghost { border-color: transparent; background: none; color: var(--text-muted); }
  .btn:disabled { opacity: 0.55; cursor: default; }
  .done { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-meta); }
  .done :global(svg) { color: var(--info); }
  .done.ko :global(svg) { color: var(--text-muted); }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
</style>
