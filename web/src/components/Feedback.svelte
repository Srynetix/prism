<script lang="ts">
  import Icon from './Icon.svelte'
  import { openLink } from '../links'
  import { hasCore } from '../bridge'
  import { agentsEnabled } from '../agents.svelte'
  import { t, tc } from '../i18n.svelte'
  import Modal from './ui/Modal.svelte'
  import State from './ui/State.svelte'
  import { KINDS, attached, capture, closeFeedback, contextLines, draftWithAgent, dropCapture, feedback, preview, send, setDraft, stopWaiting, target, uploadsCapture, vote } from '../feedback.svelte'

  /**
   * The feedback composer (feedback.svelte.ts): to which repository, what kind, a title and a text;
   * the issues that look like it (a 👍 rather than a duplicate), and exactly what goes with it, each
   * line off-able. Escape or a click outside closes, the draft stays. `phone`: the remote's, without
   * the window's capture.
   */
  let { phone = false }: { phone?: boolean } = $props()

  const dest = $derived(target())
  const lines = $derived(contextLines())
  let body = $state('')
  let showPreview = $state(false)
  $effect(() => {
    // Read again whenever the text or the ticked lines change.
    void feedback.draft.text, attached()
    if (showPreview) preview().then((b) => (body = b), () => (body = ''))
  })
  const cli = $derived(dest?.forge === 'github' ? 'gh' : 'glab')

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      void send()
    }
  }
  function opened() {
    const url = feedback.sent?.url
    if (url) openLink(url)
  }
</script>

<!-- While the window is captured, the dialog is out of the picture (`.capturing`). -->
<Modal open={feedback.open && hasCore()} key="feedback" label={t('Send feedback')} title={t('Send feedback')} width={620} class={feedback.capturing ? 'fb capturing' : 'fb'} onclose={() => !feedback.busy && closeFeedback()}>
    {#snippet head()}{#if dest}<span class="pill">{cli}</span>{/if}{/snippet}
    <!-- svelte-ignore a11y_no_static_element_interactions : ⌘↵ sends, the button does the same -->
    <div class="wrap" {onkeydown}>
      {#if !feedback.info}
        <State kind="loading" inline title={t('Reading the repositories…')} />
      {:else if !feedback.info.targets.length}
        <p class="empty">{t('No repository to send to: Prism’s is')} <code>[updates] source</code> ({t('or')} <code>[feedback] repo</code>), {t('a pack’s is the one it was imported from. Settings › General › Updates.')}</p>
      {:else if feedback.sent}
        <div class="done">
          <Icon name="check" />
          <div>
            <b>{t('Opened #{number}', { number: feedback.sent.number })}</b> {t('on {name}: it is in Settings › General › Feedback, followed with the updates.', { name: feedback.sent.name })}
            {#if feedback.captured && (!uploadsCapture() || feedback.sent.paste_capture)}<p>{t('The capture of the window is on the clipboard: paste it into the issue.')}</p>{/if}
          </div>
        </div>
        <footer class="foot">
          <span></span>
          <button class="btn ghost" onclick={() => (feedback.sent = null)}>{t('Another one')}</button>
          <button class="btn primary" onclick={opened}><Icon name="external" />{t('Open the issue')}</button>
        </footer>
      {:else}
        <div class="body">
          <span class="lab">{t('To')}</span>
          <div class="targets" role="radiogroup" aria-label={t('Repository')}>
            {#each feedback.info.targets as x (x.id)}
              <button class="opt" class:on={x.id === dest?.id} role="radio" aria-checked={x.id === dest?.id} onclick={() => setDraft({ target: x.id })}>
                <b>{x.name}</b><code>{x.repo}</code><span class="why">{x.why}</span>
              </button>
            {/each}
          </div>

          <span class="lab">{t('Kind')}</span>
          <div class="kind">
            <div class="seg" role="radiogroup" aria-label={t('Kind')}>
              {#each KINDS as k (k.kind)}
                <button class:on={feedback.draft.kind === k.kind} role="radio" aria-checked={feedback.draft.kind === k.kind} onclick={() => setDraft({ kind: k.kind })}>{t(k.label)}</button>
              {/each}
            </div>
            {#if dest?.labels[feedback.draft.kind]}<span class="hint">{t('label')} <code>{dest.labels[feedback.draft.kind]}</code></span>{/if}
          </div>

          <label class="lab" for="fb-title">{t('Title')}</label>
          <!-- svelte-ignore a11y_autofocus -->
          <input id="fb-title" autofocus value={feedback.draft.title} oninput={(e) => setDraft({ title: e.currentTarget.value })} placeholder={t('What it would change, in a line')} />

          <label class="lab" for="fb-text">{t('Text')}</label>
          <textarea id="fb-text" value={feedback.draft.text} oninput={(e) => setDraft({ text: e.currentTarget.value })} placeholder={t('Why, and what you would expect. Markdown.')}></textarea>

          {#if feedback.similar.length}
            <span class="lab">{t('Already there')}</span>
            <div class="dupes">
              {#each feedback.similar as i (i.number)}
                <div class="d">
                  <a href={i.url} onclick={(e) => (e.preventDefault(), openLink(i.url))}><code>#{i.number}</code> {i.title}</a>
                  <span class="pill" class:info={i.state === 'open'}>{tc('issue', i.state === 'open' ? 'open' : 'closed')}</span>
                  <button class="vote" class:on={feedback.voted.includes(i.number)} disabled={feedback.voted.includes(i.number)} onclick={() => vote(i)} title={t('A 👍 rather than a duplicate')}>👍 {i.votes + (feedback.voted.includes(i.number) ? 1 : 0)}</button>
                </div>
              {/each}
            </div>
          {/if}

          <span class="lab">{t('Attached')}</span>
          <div class="ctx">
            {#each lines as l (l.key)}
              <label class="c"><input type="checkbox" checked={!!feedback.on[l.key]} onchange={(e) => (feedback.on[l.key] = e.currentTarget.checked)} /> {t(l.label)}<span class="v" title={l.value}>{l.value}</span></label>
            {/each}
            {#if !phone && feedback.info.capture}
              <div class="c">
                <input type="checkbox" checked={feedback.captured} onchange={(e) => (e.currentTarget.checked ? capture() : dropCapture())} aria-label={t('Capture of the window')} /> {t('Capture of the window')}
                <span class="v">{feedback.captured ? (uploadsCapture() ? t('uploaded with the issue') : t('on the clipboard: paste it into the issue')) : uploadsCapture() ? t('off · uploaded with the issue') : t('off · put on the clipboard, to paste')}</span>
              </div>
              {#if feedback.shot}
                <div class="c shot">
                  <img src={feedback.shot} alt={t('Capture of the window')} />
                  <button class="link" onclick={() => capture()}>{t('Capture again')}</button>
                </div>
              {/if}
            {/if}
            <div class="c never"><Icon name="lock" /> {t('Never: config values, tokens, tickets, mail')}</div>
          </div>

          {#if showPreview}
            <span class="lab">{t('Preview')}</span>
            <pre class="preview"><b>{feedback.draft.title || t('No title')}</b>{'\n\n'}{body}</pre>
          {/if}
        </div>

        {#if feedback.error}
          <div class="banner ko">
            <span>{feedback.error}</span>
            {#if feedback.fallback}<button class="link" onclick={() => openLink(feedback.fallback)}>{t('Open it on the forge instead →')}</button>{/if}
          </div>
        {/if}
        {#if feedback.drafting}<div class="banner info"><span>{t('An agent drafts the issue: its draft comes here when written.')}</span><button class="link" onclick={stopWaiting}>{t('Stop waiting')}</button></div>{/if}

        <footer class="foot">
          <button class="btn ghost" onclick={() => (showPreview = !showPreview)} aria-pressed={showPreview}>{showPreview ? t('Hide') : t('Preview')}</button>
          {#if agentsEnabled()}<button class="btn ghost" onclick={draftWithAgent} disabled={feedback.drafting} title={t('An agent reads the code and writes the draft; you send it')}>{t('Draft with an agent')}</button>{/if}
          <span class="cmd">{cli} → {dest?.project}</span>
          <button class="btn primary" class:busy={feedback.busy} onclick={send} disabled={feedback.busy || !feedback.draft.title.trim()} title="⌘↵">{#if feedback.busy}<span class="spinner"></span>{t('Opening…')}{:else}{t('Open the issue')}{/if}</button>
        </footer>
      {/if}
    </div>
</Modal>

<style>
  :global(.veil:has(> .modal.fb.capturing)) { visibility: hidden; }
  .wrap { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .wrap > .empty { margin: 0; padding: var(--sp-6); }
  .body { min-height: 0; display: grid; grid-template-columns: 84px minmax(0, 1fr); gap: 9px 12px; align-items: start; padding: 12px 14px; overflow: auto; }
  :global(html.remote) .body { grid-template-columns: minmax(0, 1fr); }
  .body > * { min-width: 0; }
  .lab { padding-top: 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  input:not([type]), textarea { width: 100%; padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; }
  textarea { min-height: 96px; resize: vertical; font-family: var(--mono); font-size: var(--fs-meta); line-height: 1.5; }
  .targets { display: flex; flex-direction: column; gap: 4px; }
  .opt { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .opt.on { border-color: var(--accent); }
  .opt b { flex-shrink: 0; white-space: nowrap; }
  .opt code { min-width: 0; overflow: hidden; color: var(--text-muted); font-size: var(--fs-tag); text-overflow: ellipsis; white-space: nowrap; }
  .opt .why { flex: 1 1 0; min-width: 0; text-align: right; margin-left: auto; color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kind { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
  .hint { margin-left: 6px; color: var(--text-muted); font-size: var(--fs-tag); }
  .dupes { display: flex; flex-direction: column; gap: 2px; padding: 5px 8px; border: 1px dashed var(--border); border-radius: var(--r6); }
  .d { display: flex; align-items: center; gap: 8px; }
  .d a { flex: 1; min-width: 0; overflow: hidden; color: var(--text); text-decoration: none; text-overflow: ellipsis; white-space: nowrap; }
  .d a:hover { color: var(--accent); }
  .vote { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .vote.on { border-color: var(--accent); color: var(--accent); }
  .ctx { border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .c { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); }
  .c:last-child { border-bottom: 0; }
  .c .v { margin-left: auto; min-width: 0; max-width: 62%; overflow: hidden; color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); text-overflow: ellipsis; white-space: nowrap; }
  .c.shot { align-items: flex-end; }
  .c.shot img { max-width: 220px; max-height: 140px; border: 1px solid var(--border); border-radius: var(--r6); }
  .c.shot .link { color: var(--text-muted); font-size: var(--fs-tag); }
  .c.never { background: var(--surface-muted); color: var(--text-muted); }
  .preview { margin: 0; max-height: 220px; overflow: auto; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); font-family: var(--mono); font-size: var(--fs-tag); line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
  .banner { display: flex; align-items: baseline; gap: 10px; margin: 0 14px 10px; padding: 7px 10px; border-radius: var(--r6); font-size: var(--fs-meta); }
  .banner span { flex: 1; }
  .banner.ko { background: var(--ko-bg, var(--surface-muted)); color: var(--ko); }
  .banner.info { background: var(--info-bg); color: var(--info); }
  .link { flex-shrink: 0; padding: 0; border: none; background: none; color: inherit; font: inherit; font-weight: 600; cursor: pointer; }
  .done { display: flex; gap: 10px; padding: 16px 14px; color: var(--ok); }
  .done div { color: var(--text); }
  .done p { margin: 6px 0 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .foot { display: flex; align-items: center; gap: 8px; padding: var(--sp-4) var(--sp-6); border-top: 1px solid var(--border); }
  .foot > span, .cmd { flex: 1; min-width: 0; overflow: hidden; color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); text-overflow: ellipsis; white-space: nowrap; }
</style>
