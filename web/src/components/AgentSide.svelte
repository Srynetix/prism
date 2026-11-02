<script lang="ts">
  import { suggest } from '../suggest'
  import { tick } from 'svelte'
  import Icon from './Icon.svelte'
  import MrDiffView from './MrDiffView.svelte'
  import AgentWebPicker from './AgentWebPicker.svelte'
  import Markdown from './Markdown.svelte'
  import AgentFiles from './AgentFiles.svelte'
  import Shells from './Shells.svelte'
  import AgentTasks from './AgentTasks.svelte'
  import AgentActivityPanel from './AgentActivity.svelte'
  import AgentLaunch from './AgentLaunch.svelte'
  import AgentGit from './AgentGit.svelte'
  import AgentNotes from './AgentNotes.svelte'
  import AgentPlan from './AgentPlan.svelte'
  import { addNote, cancelQueue, composePrompt, drafts, editNote, fingerprint, isQueued, notesOf, removeNote, resolveNote, sendError, sendReview, sentDiffOf, syncWithDiff, type ReviewHooks } from '../agent-review.svelte'
  import { ready } from '../agent-inbox.svelte'
  import { keys } from '../platform'
  import { agentArtifacts, agentShellOwner, agentDiff, agentDiffImage, agentFileText, agentFileUrl, agentPageText, agentPages, SCRATCHPAD, type AgentArtifact, type AgentPage, type SideTab } from '../agents.svelte'
  import { nameOf } from '../agent-cli'
  import { invoke, isDesktop } from '../bridge'
  import { ago } from '../time'
  import { pref, setPref } from '../layout.svelte'
  import type { MrDiff } from '../mr-review.svelte'
  import type { Agent } from '../types'
  import { pollsShown } from '../polls.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Right panel of the Agents view, on the folder the agent works in (its worktree, or
   * its folder): its files, its diff (uncommitted, or the whole branch), a Web view
   * that shows a page from the folder (served by `prismfile:`), a Claude artifact (a page
   * from its session's scratchpad) or a local server, its tasks, and a shell there.
   */
  let { agent, tab = $bindable() }: { agent: Agent; tab: SideTab } = $props()
  /**
   * On a phone (remote.html): its diff read only — review notes go to the agent as a prompt, which a
   * phone types only through its answer box.
   */
  const phone = !isDesktop()

  /* ---------- Diff ---------- */

  let branch = $state(pref<boolean>('agents.side.branch', false))
  const setBranch = (on: boolean) => {
    setPref('agents.side.branch', (branch = on))
    void loadDiff()
  }
  let diff = $state<(MrDiff & { against: string; head: string }) | null>(null)
  let diffError = $state('')
  let diffBusy = $state(false)
  async function loadDiff() {
    diffBusy = true
    diffError = ''
    try {
      diff = await agentDiff(agent.id, branch)
      changed = syncWithDiff(agent.id, diff)
    } catch (e) {
      diffError = String(e)
    } finally {
      diffBusy = false
    }
  }
  /** What a diff reads: the same value, the same view. */
  const diffKey = (d: NonNullable<typeof diff>) => `${d.against} ${d.head} ${d.truncated}\n${d.files.map((f) => `${f.old_path}>${f.new_path}:${fingerprint(f.diff)}`).join('\n')}`
  let diffBox = $state<HTMLDivElement>()
  /**
   * The diff read again in the background while its tab shows: the agent writes its files along
   * the way. The view is redrawn only when the diff changed, at the same scroll, and never under
   * a note being written (the redraw would drop it).
   */
  async function refreshDiff() {
    if (diffBusy || !diff || diffBox?.contains(document.activeElement) && document.activeElement?.matches('textarea, input')) return
    const was = branch
    try {
      const next = await agentDiff(agent.id, branch)
      if (was !== branch || diffBusy || !diff || diffKey(next) === diffKey(diff) || diffBox?.contains(document.activeElement) && document.activeElement?.matches('textarea, input')) return
      const top = diffBox?.scrollTop ?? 0
      diff = next
      changed = syncWithDiff(agent.id, next)
      diffError = ''
      await tick()
      if (diffBox) diffBox.scrollTop = top
    } catch {
      // The next read, or ↻, says it.
    }
  }

  /* ---------- Review notes to the agent ---------- */

  /** Sent notes whose lines read differently in the last diff read. */
  let changed = $state(new Set<string>())
  const review = $derived<ReviewHooks>({
    notes: notesOf(agent.id).map((n) => ({ ...n, changed: changed.has(n.id) })),
    add: (n) => addNote(agent.id, { ...n, head: diff?.head ?? '' }),
    edit: (id, body) => editNote(agent.id, id, body),
    remove: (id) => removeNote(agent.id, id),
    resolve: (id) => resolveNote(agent.id, id),
  })
  const pending = $derived(drafts(agent.id))
  const queued = $derived(isQueued(agent.id))
  const busy = $derived(agent.state === 'running' && (agent.status === 'running' || agent.status === 'waiting_tool'))
  /** A menu of its on its screen (a permission, its question): Send waits for its answer (`agent-inbox`). */
  const menu = $derived(agent.state === 'running' && !ready(agent.id))
  /** "Since my notes": only the files whose diff moved since the notes were sent. */
  let since = $state(false)
  const sentDiff = $derived(sentDiffOf(agent.id))
  const shownDiff = $derived(diff && since && sentDiff ? { ...diff, files: diff.files.filter((f) => sentDiff[f.new_path || f.old_path] !== fingerprint(f.diff)) } : diff)
  /**
   * An image of the diff, kept while what it is read from stays (the view is drawn again when any
   * file of the diff changes): its former version by the revision compared to, its new one by the
   * file's stamp on disk (none found: read again).
   */
  async function diffImage(path: string, old: boolean): Promise<string> {
    const d = diff
    const version = old ? (d ? `${d.against}@${d.head}` : null) : await invoke<(string | null)[]>('agent_files_stamps', { id: agent.id, paths: [path] }).then(([s]) => s, () => null)
    return agentDiffImage(agent.id, path, branch, old, version)
  }
  let showPrompt = $state(false)
  let sending = $state(false)
  let reviewError = $state('')
  async function send(how: 'send' | 'insert' | 'interrupt') {
    if (!diff || sending) return
    sending = true
    reviewError = ''
    try {
      await sendReview(agent.id, how, diff)
      showPrompt = false
    } catch (e) {
      reviewError = String(e)
    } finally {
      sending = false
    }
  }
  /** ⌘↵ in the Diff tab sends the drafts (in a note being written, it adds the note). */
  function onDiffKey(e: KeyboardEvent) {
    if (e.key !== 'Enter' || !(e.metaKey || e.ctrlKey) || (e.target as HTMLElement | null)?.closest?.('textarea, input')) return
    e.preventDefault()
    void send('send')
  }
  // Once the agent is back at its prompt after a review, the diff is read again: its notes say what moved.
  let wasBusy = false
  $effect(() => {
    const now = busy
    if (wasBusy && !now && tab === 'diff' && notesOf(agent.id).some((n) => n.status === 'sent')) void loadDiff()
    wasBusy = now
  })

  /* ---------- Web ---------- */

  // The panel is recreated for each agent ({#key}): the initial value is enough.
  // svelte-ignore state_referenced_locally
  let address = $state(pref<Record<string, string>>('agents.side.web', {})[agent.id] ?? '')
  let shown = $state('')
  let nonce = $state(0)
  const isLocal = (u: string) => /^http:\/\/(localhost|127\.0\.0\.1)(:\d+)?(\/|$)/i.test(u)
  /** A local URL as is; otherwise a path in the agent's folder. */
  const path = $derived(shown.replace(/^\.?\//, ''))
  /**
   * Markdown and plain text are shown by Prism, in its theme: in the frame, a text file follows
   * the dark appearance (light text) over the frame's white, and Markdown would stay raw.
   */
  const TEXT = /\.(md|markdown|txt|log)$/i
  const isText = $derived(!!shown && !isLocal(shown) && TEXT.test(path))
  const src = $derived(!shown || isText ? '' : isLocal(shown) ? shown : agentFileUrl(agent.id, path))
  let page = $state<{ text: string; error: string } | null>(null)
  $effect(() => {
    void nonce
    if (!isText) return void (page = null)
    let gone = false
    agentPageText(agent.id, path).then(
      (text) => !gone && (page = { text, error: '' }),
      (e) => !gone && (page = { text: '', error: String(e) }),
    )
    return () => (gone = true)
  })
  /** The picker of pages (Open…): the folder's, or the session's artifacts; both read again as it opens. */
  let picking = $state(false)
  function togglePicker() {
    picking = !picking
    if (picking) {
      void loadPages()
      void loadArtifacts()
    }
  }
  /**
   * The files of the folder the Web view can show, for its picker (reread when it opens); with
   * `ignored` (remembered), those `.gitignore` leaves out too.
   */
  let ignored = $state(pref<boolean>('agents.picker.ignored', false))
  function setIgnored(on: boolean) {
    setPref('agents.picker.ignored', (ignored = on))
    pages = null
    void loadPages()
  }
  let pages = $state<AgentPage[] | null>(null)
  async function loadPages() {
    try {
      const next = await agentPages(agent.id, ignored)
      if (JSON.stringify(next) !== JSON.stringify(pages)) pages = next
    } catch {
      pages = []
    }
  }
  function go(target = address) {
    address = target.trim()
    shown = address
    nonce++
    setPref('agents.side.web', { ...pref<Record<string, string>>('agents.side.web', {}), [agent.id]: address })
  }
  /**
   * Artifacts: the pages Claude writes to the session scratchpad before publishing them
   * (claude.ai refuses to be shown in a frame: we show their local source).
   */
  let artifacts = $state<AgentArtifact[] | null>(null)
  async function loadArtifacts() {
    let next: AgentArtifact[]
    try {
      next = await agentArtifacts(agent.id)
    } catch {
      next = []
    }
    // Unchanged: left as is, so that a rereading does not close the open selector.
    if (JSON.stringify(next) !== JSON.stringify(artifacts)) artifacts = next
  }
  const artifactAddress = (path: string) => `${SCRATCHPAD}/${path}`
  /**
   * The page over the whole window, Prism's chrome hidden (not macOS full screen): ⤢ in the bar, Esc
   * or ⤡ to come back. Only its frame changes place, so the page is not reloaded.
   */
  let overlay = $state(false)
  $effect(() => {
    if (tab !== 'web') overlay = false
  })
  function onOverlayKey(e: KeyboardEvent) {
    if (!overlay || e.key !== 'Escape' || picking || (e.target as HTMLElement | null)?.closest?.('input, textarea, select')) return
    e.preventDefault()
    e.stopImmediatePropagation()
    overlay = false
  }
  const webError = $derived(/^https?:\/\//i.test(shown) && !isLocal(shown) ? t('Only a local server (http://localhost:…) or a file of the agent folder.') : '')

  // Each tab loads on first opening; ↻ reloads it.
  $effect(() => {
    if (tab === 'diff' && !diff && !diffError && !diffBusy) void loadDiff()
    if (tab === 'web' && !shown && address) go()
  })
  const polling = pollsShown()
  /** On the Diff tab, the diff is reread every 3 s (`refreshDiff`). */
  $effect(() => {
    if (tab !== 'diff' || !polling()) return
    const timer = setInterval(() => void refreshDiff(), 3000)
    return () => clearInterval(timer)
  })
  /** On the Web tab, the artifacts are reread on opening then every 5 s: Claude writes them along the way. */
  $effect(() => {
    if (tab !== 'web' || !polling()) return
    void loadArtifacts()
    if (!pages) void loadPages()
    const timer = setInterval(() => void loadArtifacts(), 5000)
    return () => clearInterval(timer)
  })
</script>

<aside class="side">
  {#if tab === 'activity'}
    <AgentActivityPanel {agent} />
  {:else if tab === 'launch'}
    <AgentLaunch {agent} />
  {:else if tab === 'notes'}
    <AgentNotes {agent} />
  {:else if tab === 'plan'}
    <AgentPlan {agent} />
  {:else if tab === 'shell'}
    <Shells owner={agentShellOwner(agent)} />
  {:else if tab === 'tasks'}
    <AgentTasks {agent} onrun={() => (tab = 'shell')} />
  {:else if tab === 'files'}
    <AgentFiles {agent} onweb={(path) => { tab = 'web'; go(path) }} ondiff={() => (tab = 'diff')} />
  {:else if tab === 'diff'}
    <AgentGit {agent} onchange={loadDiff} />
    <div class="bar">
      <div class="seg">
        <button class:on={!branch} onclick={() => setBranch(false)} title={t('Working tree against HEAD')}>{t('Uncommitted')}</button>
        <button class:on={branch} onclick={() => setBranch(true)} title={t('Working tree against the merge-base of its base branch: what the MR will carry')}>{t('Branch')}</button>
      </div>
      {#if diff}<small class="muted">{t('vs {ref}', { ref: diff.against })}</small>{/if}
      <span class="grow"></span>
      {#if !phone && sentDiff && notesOf(agent.id).some((n) => n.status === 'sent')}<button class:on={since} onclick={() => (since = !since)} title={t('Only the files that changed since the notes were sent')}>{t('Since my notes')}</button>{/if}
      <button onclick={loadDiff} disabled={diffBusy} title={t('Compute the diff again')}><Icon name="refresh" spin={diffBusy} /></button>
    </div>
    {#if diffError}<p class="err">{diffError}</p>{/if}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="diff" bind:this={diffBox} onkeydown={onDiffKey}>
      {#if shownDiff}
        {#if shownDiff.files.length}
          {#key shownDiff}<MrDiffView iid="agent:{agent.id}" diff={shownDiff} discussions={[]} me={null} source={(path) => agentFileText(agent.id, path)} image={diffImage} review={phone ? undefined : review} {phone} />{/key}
          {#if shownDiff.truncated}<p class="muted empty">{t('Diff cut: too large.')}</p>{/if}
        {:else}
          <p class="muted empty">{since ? t('Nothing changed since the notes were sent.') : branch ? t('No change on this branch.') : t('No change since the last commit.')}</p>
        {/if}
      {/if}
    </div>
    {#if !phone && (pending.length || queued)}
      <div class="tray">
        <div class="trow">
          <b>{tn(pending.length, '{n} note', '{n} notes')}</b>
          <span class="muted">{queued ? t('to send once {agent}’s menu is answered', { agent: nameOf(agent) }) : t('not sent')}</span>
          <span class="grow"></span>
          {#if queued}
            <button onclick={() => cancelQueue(agent.id)} title={t('Keep them as drafts')}>{t('Cancel')}</button>
            <button onclick={() => send('interrupt')} disabled={sending} title={t('Stop {agent}’s turn (Esc), then send', { agent: nameOf(agent) })}>{t('Interrupt and send')}</button>
          {:else}
            <button class:on={showPrompt} onclick={() => (showPrompt = !showPrompt)} title={t('What {agent} will read', { agent: nameOf(agent) })}>{t('Preview')}</button>
            <button onclick={() => send('insert')} disabled={sending || !diff} title={t('Type it in {agent}’s prompt, without Enter', { agent: nameOf(agent) })}>{t('Insert')}</button>
            <button class="primary" onclick={() => send('send')} disabled={sending || !diff} title={menu ? t('{agent} shows a menu: sent once it is answered', { agent: nameOf(agent) }) : busy ? t('Sent at once: {agent} reads it once its turn is over', { agent: nameOf(agent) }) : t('Type it in {agent}’s terminal and submit it', { agent: nameOf(agent) })}>{menu ? t('Send after its menu') : t('Send')} <kbd>{keys('⌘↵')}</kbd></button>
          {/if}
        </div>
        {#if busy && !menu && !queued}<p class="muted note">{t('{agent} is working: it reads them once its turn is over.', { agent: nameOf(agent) })} <button class="link" onclick={() => send('interrupt')} disabled={sending}>{t('Interrupt and send')}</button></p>{/if}
        {#if reviewError || sendError(agent.id)}<p class="err">{reviewError || sendError(agent.id)}</p>{/if}
        {#if showPrompt && diff}<pre class="prompt">{composePrompt(pending, diff.files.map((f) => f.new_path || f.old_path))}</pre>{/if}
      </div>
    {/if}
  {:else}
    <div class="web" class:overlay role={overlay ? 'dialog' : undefined} aria-label={overlay ? t('Web view of {agent}, over the window', { agent: agent.title }) : undefined}>
    <form class="bar" onsubmit={(e) => { e.preventDefault(); go() }}>
      <input bind:value={address} placeholder={t('docs/page.html or http://localhost:5173')} spellcheck="false" {@attach suggest(() => (pages ?? []).map((p) => p.path))} />
      <button type="button" class:on={picking} data-picker-toggle onclick={togglePicker} aria-expanded={picking} title={t('Pick a page of the folder, or a {agent} artifact', { agent: nameOf(agent) })}><Icon name="files" />{t('Open…')}{#if artifacts?.length}<span class="n">{artifacts.length}</span>{/if}</button>
      <button type="submit" title={t('Load (reload)')}><Icon name="refresh" /></button>
      <button type="button" class:on={overlay} onclick={() => (overlay = !overlay)} aria-pressed={overlay} title={overlay ? t('Back to the panel (Esc)') : t('The page over the whole window')} aria-label={overlay ? t('Back to the panel') : t('Over the whole window')}><Icon name={overlay ? 'shrink' : 'expand'} /></button>
    </form>
    {#if picking}
      <AgentWebPicker {pages} {artifacts} {ignored} onignored={setIgnored} cli={nameOf(agent)} onpick={(p, scratchpad) => go(scratchpad ? artifactAddress(p) : p)} onclose={() => (picking = false)} />
    {/if}
    {#if webError}
      <p class="err">{webError}</p>
    {:else if isText}
      {#if page?.error}<p class="err">{page.error}</p>
      {:else if page && /\.(md|markdown)$/i.test(path)}<div class="page md"><Markdown text={page.text} reading /></div>
      {:else if page}<pre class="page">{page.text}</pre>{/if}
    {:else if src}
      <!-- No allow-same-origin for a file: the page has neither storage nor access to Prism. A local server has its own origin. -->
      {#key nonce}
        <!-- Named by aria-label, not title: a title would be a tooltip that stays over the page (the pointer inside it no longer reaches Prism). -->
        <!-- svelte-ignore a11y_missing_attribute -->
        <iframe aria-label={t('Web view of {agent}', { agent: agent.title })} {src} sandbox={isLocal(shown) ? 'allow-scripts allow-forms allow-popups allow-modals allow-same-origin' : 'allow-scripts allow-forms allow-popups allow-modals'}></iframe>
      {/key}
    {:else}
      <p class="muted empty">{t('A page of the agent folder (a mockup, a report, a Markdown file: pick it with Open… or type its path), a {agent} artifact, or a local dev server. In Files, an HTML file opens here.', { agent: nameOf(agent) })}</p>
      {#if artifacts?.length}
        <div class="list">
          <div class="grp">{t('Artifacts of this session')}</div>
          {#each artifacts as x (x.path)}
            <button class="file art" onclick={() => go(artifactAddress(x.path))} title={t('{path}: in the scratchpad of this {agent} session', { path: x.path, agent: nameOf(agent) })}><Icon name="web" /><span class="p">{x.path}</span><span class="tm">{ago(x.modified)}</span></button>
          {/each}
        </div>
      {/if}
    {/if}
    </div>
  {/if}
</aside>

<svelte:window onkeydowncapture={onOverlayKey} />

<style>
  .side { position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; border-left: 1px solid var(--border); background: var(--surface); }
  .bar .n { padding: 0 5px; border-radius: var(--r8); background: color-mix(in srgb, var(--src-claude) 18%, transparent); color: var(--src-claude); font-size: var(--fs-tag); }
  .bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .bar input { flex: 1; min-width: 0; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: var(--fs-meta) var(--mono); outline: none; }
  .bar input:focus { border-color: var(--src-claude); }
  .bar button { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .bar button.on { border-color: var(--src-claude); color: var(--src-claude); }
  .grow { flex: 1; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { border: none; border-radius: 0; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: var(--src-claude); color: #fff; }
  .err { margin: 0; padding: 6px 10px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .empty { padding: 12px; font-size: var(--fs-meta); }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 4px; display: flex; flex-direction: column; }
  .file { display: grid; grid-template-columns: 14px minmax(0, 1fr); gap: 6px; padding: 1px 6px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: var(--fs-meta) var(--mono); text-align: left; cursor: pointer; }
  .file:hover:not(:disabled) { background: var(--surface-muted); }
  .file:disabled { cursor: default; }
  .file .p { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grp { margin: 4px 6px; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .file.art { grid-template-columns: 14px minmax(0, 1fr) auto; align-items: center; }
  .file.art :global(svg) { color: var(--src-claude); }
  .file .tm { color: var(--text-muted); }
  .diff { flex: 1; min-height: 0; overflow: auto; padding: 6px; container-type: inline-size; --diff-pad-y: 6px; --diff-pad-x: 6px; }
  /* The notes not sent yet, and how they go to the agent. */
  .tray { flex-shrink: 0; max-height: 45%; display: flex; flex-direction: column; border-top: 1px solid var(--border); background: color-mix(in srgb, var(--src-claude) 6%, var(--surface)); }
  .trow { display: flex; align-items: center; gap: 6px; padding: 6px 8px; font-size: var(--fs-meta); }
  .trow b { color: var(--src-claude); }
  .trow button { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .trow button.on { border-color: var(--src-claude); color: var(--src-claude); }
  .trow button.primary { background: var(--src-claude); border-color: var(--src-claude); color: #fff; background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--src-claude); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .trow button:disabled { opacity: 0.6; cursor: default; }
  .trow kbd { font: 10px var(--mono); opacity: 0.8; }
  .tray .note { margin: 0; padding: 0 8px 6px; font-size: var(--fs-tag); }
  .link { padding: 0; border: none; background: none; color: var(--src-claude); font: inherit; cursor: pointer; text-decoration: underline; }
  .prompt { flex: 1; min-height: 0; margin: 0 8px 8px; padding: 8px 10px; overflow: auto; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: var(--fs-tag) / 1.5 var(--mono); white-space: pre-wrap; }
  .page { flex: 1; min-height: 0; margin: 0; overflow: auto; padding: 10px 14px; color: var(--text); }
  pre.page { font: var(--fs-mono) var(--mono); white-space: pre-wrap; }
  .page.md { padding: 16px 22px 24px; }
  iframe { flex: 1; min-height: 0; width: 100%; border: none; background: #fff; }
  .web { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
  /* Over the whole window: above the views and drawers, below the boot screen and tooltips. */
  .web.overlay { position: fixed; inset: 0; z-index: 90; background: var(--surface); box-shadow: inset 0 2px var(--src-claude); }
</style>
