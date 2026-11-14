<script lang="ts">
  import RepoChip from './RepoChip.svelte'
  import { t, tn, tc } from '../i18n.svelte'
  import { hasCore, invoke } from '../bridge'
  import BlockControls from './BlockControls.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { claudeClick, cveAction } from '../claude-actions'
  import { ago } from '../time'
  import type { ScannedImage, VulnFinding, VulnProject, Vulnerabilities } from '../types'
  import { keys } from '../platform'
  import { copy } from '../toast.svelte'
  import { imageState } from '../vuln-images'

  /**
   * Vulnerabilities in the dependencies of the main branch (or of `branches`: release and main, each
   * project's line then named after its branch) and of the images a block names (`images`), from a
   * local Trivy scan (HIGH and CRITICAL with a fix). By project then by image, what needs fixing and what the `.trivyignore`
   * accepts; a project expands onto its own CVEs (remembered under `vulns.expanded`). Then
   * all the CVEs to fix, with Fix CVE (cve skill), and the accepted ones, collapsed (remembered under
   * `vulns.accepted`) or hidden everywhere ("Hide ignored").
   */
  let { vulns, error }: { vulns: Vulnerabilities | null | undefined; error?: string } = $props()

  let showAccepted = $state(pref<boolean>('vulns.accepted', false))
  function toggleAccepted() {
    showAccepted = !showAccepted
    setPref('vulns.accepted', showAccepted)
  }
  /**
   * Hide the CVEs ignored by a `.trivyignore` (accepted): their list, their share in the
   * projects and their counters; a project left with only those becomes clean (`vulns.hideIgnored`).
   */
  let hideIgnored = $state(pref<boolean>('vulns.hideIgnored', false))
  function toggleIgnored() {
    hideIgnored = !hideIgnored
    setPref('vulns.hideIgnored', hideIgnored)
  }
  /** Hide projects with nothing to report (remembered under `vulns.hideClean`). */
  let hideClean = $state(pref<boolean>('vulns.hideClean', false))
  function toggleClean() {
    hideClean = !hideClean
    setPref('vulns.hideClean', hideClean)
  }
  /**
   * Displayed groups: dependencies (repo files), deployed images and base images a provider names,
   * built in other repos (remembered under `vulns.show`).
   */
  type Kind = 'deps' | 'images' | 'base'
  let show = $state<Record<Kind, boolean>>({ deps: true, images: true, base: true, ...pref<Partial<Record<Kind, boolean>>>('vulns.show', {}) })
  function toggleShow(key: Kind) {
    show = { ...show, [key]: !show[key] }
    setPref('vulns.show', show)
  }
  const kindOf = (p: VulnProject): Kind => (p.base ? 'base' : p.image ? 'images' : 'deps')
  const targets = $derived(new Map((vulns?.projects ?? []).map((p) => [p.project, p])))
  /** A CVE shows if it affects a project of a displayed group (the indicators stay global). */
  const shown = (f: VulnFinding) => f.projects.some((p) => show[kindOf(targets.get(p) ?? { project: p } as VulnProject)])
  /**
   * Fix CVE for a library: in its repo if all the affected targets are base images of one
   * repository, otherwise in the primary repository.
   */
  function fixOf(g: PkgGroup, project?: string) {
    const scope = project ? [project] : g.projects
    const repos = new Set(scope.map((p) => (targets.get(p)?.base ? targets.get(p)?.repo : null) ?? null))
    const repo = repos.size === 1 ? [...repos][0] : null
    return repo ? cveAction(g.package, { repo, images: scope, ids: g.findings.map((f) => f.id) }) : cveAction(g.package)
  }
  let expanded = $state<string[]>(pref<string[]>('vulns.expanded', []))
  function toggleProject(project: string) {
    expanded = expanded.includes(project) ? expanded.filter((p) => p !== project) : [...expanded, project]
    setPref('vulns.expanded', expanded)
  }
  /** A project's CVEs: to fix first, then accepted (same order as the general list). */
  const findingsOf = (project: string) => {
    const mine = vulns?.findings.filter((f) => f.projects.includes(project)) ?? []
    return [...mine.filter((f) => !f.accepted), ...(hideIgnored ? [] : mine.filter((f) => f.accepted))]
  }
  let asked = $state(false)
  async function rescan() {
    asked = true
    try {
      await invoke('rescan_vulnerabilities')
    } finally {
      // The scan itself is shown through `scanning` in the snapshot.
      setTimeout(() => (asked = false), 1500)
    }
  }

  /** « Check the registry » or one image rescanned: shown through `checking` in the snapshot. */
  let checkAsked = $state(false)
  const checking = $derived(checkAsked || !!vulns?.checking)
  async function ask(command: string, args?: Record<string, unknown>) {
    checkAsked = true
    try {
      await invoke(command, args)
    } finally {
      setTimeout(() => (checkAsked = false), 1500)
    }
  }
  const hasImages = $derived(!!vulns?.projects.some((p) => p.scanned))

  const open = $derived(vulns?.findings.filter((f) => !f.accepted) ?? [])
  const accepted = $derived(vulns?.findings.filter((f) => f.accepted) ?? [])
  const listOpen = $derived(open.filter(shown))
  const listAccepted = $derived(accepted.filter(shown))

  /**
   * One row per library: its CVEs (expandable), the highest severity, the affected
   * projects, the installed versions and the highest fixed version. Critical first, then
   * the most affected.
   */
  interface PkgGroup {
    package: string
    findings: VulnFinding[]
    severity: 'critical' | 'high'
    installed: string[]
    fixed: string
    projects: string[]
    fresh: number
    accepted: boolean
  }
  /** Rough version comparison, number by number (`1.10.2` > `1.9.9`). */
  function cmpVersion(a: string, b: string) {
    const na = a.split(/[^0-9]+/).filter(Boolean).map(Number)
    const nb = b.split(/[^0-9]+/).filter(Boolean).map(Number)
    for (let i = 0; i < Math.max(na.length, nb.length); i++) if ((na[i] ?? 0) !== (nb[i] ?? 0)) return (na[i] ?? 0) - (nb[i] ?? 0)
    return 0
  }
  function byPackage(findings: VulnFinding[]): PkgGroup[] {
    const groups = new Map<string, VulnFinding[]>()
    for (const f of findings) groups.set(f.package, [...(groups.get(f.package) ?? []), f])
    return [...groups.entries()]
      .map(([pkg, fs]) => {
        // Trivy may give several fixed versions ("1.2.3, 2.0.1"): the first of each.
        const fixes = fs.map((f) => f.fixed.split(',')[0].trim()).filter(Boolean)
        return {
          package: pkg,
          findings: [...fs].sort((a, b) => Number(b.severity === 'critical') - Number(a.severity === 'critical') || a.id.localeCompare(b.id)),
          severity: fs.some((f) => f.severity === 'critical') ? 'critical' : 'high',
          installed: [...new Set(fs.map((f) => f.installed))],
          fixed: fixes.sort(cmpVersion).at(-1) ?? '',
          projects: [...new Set(fs.flatMap((f) => f.projects))],
          fresh: fs.filter((f) => f.new).length,
          accepted: fs.every((f) => f.accepted),
        } satisfies PkgGroup
      })
      .sort((a, b) => Number(b.severity === 'critical') - Number(a.severity === 'critical') || b.findings.length - a.findings.length || a.package.localeCompare(b.package))
  }
  const groupsOpen = $derived(byPackage(listOpen))
  const groupsAccepted = $derived(byPackage(listAccepted))
  /** Libraries expanded onto their CVEs (by `<list>:<package>`), for the session. */
  let unfolded = $state<string[]>([])
  const toggleUnfold = (key: string) => (unfolded = unfolded.includes(key) ? unfolded.filter((k) => k !== key) : [...unfolded, key])
  const critical = $derived(open.filter((f) => f.severity === 'critical').length)
  const fresh = $derived(open.filter((f) => f.new).length)
  /** All scanned targets: the most exposed first (errors included), the clean ones after. */
  const exposed = (kind: Kind) =>
    [...(vulns?.projects ?? [])]
      .filter((p) => kindOf(p) === kind)
      .sort((a, b) => b.critical - a.critical || b.high - a.high || Number(!!b.error) - Number(!!a.error) || b.accepted - a.accepted || a.project.localeCompare(b.project))
  const isClean = (p: { critical: number; high: number; accepted: number; error?: string | null }) => !p.error && p.critical + p.high + (hideIgnored ? 0 : p.accepted) === 0
  const GROUPS: { key: Kind; label: string; check: string; title: string }[] = [
    { key: 'deps', label: t('Dependencies'), check: t('Files'), title: t('Dependencies of the repository files (lockfiles, manifests)') },
    { key: 'images', label: t('Images'), check: t('Images'), title: t('Images a block of your team names ([vulnerabilities] images)') },
    { key: 'base', label: t('Base images'), check: t('Base'), title: t('Base images a block of your team names, fixed in their repository') },
  ]
  const groups = $derived(GROUPS.map((g) => ({ ...g, items: exposed(g.key) })).map((g) => ({ ...g, total: g.items.length })))
  const clean = $derived((vulns?.projects ?? []).filter((p) => p.critical + p.high === 0 && !p.error).length)
  const failed = $derived((vulns?.projects ?? []).filter((p) => p.error).length)
  const busy = $derived(asked || !!vulns?.scanning)
</script>

{#snippet pkgRow(g: PkgGroup, scope: string, project?: string)}
  {@const a = fixOf(g, project)}
  {@const key = `${scope}:${g.package}`}
  {@const many = g.findings.length > 1}
  {@const isOpen = many && unfolded.includes(key)}
  <div class="cve" class:accepted={g.accepted}>
    <span class="sev {g.severity}">{g.severity}</span>
    <span class="pkg">
      <span class="l1">
        <b>{g.package}</b>
        {#if many}
          <button class="count" class:open={isOpen} onclick={() => toggleUnfold(key)} aria-expanded={isOpen} title={g.findings.map((f) => f.id).join(', ')}><span class="caret" class:open={isOpen}><Icon name="chevronRight" /></span>{t('{n} CVEs', { n: g.findings.length })}</button>
        {:else}
          {@const f = g.findings[0]}
          {#if f.url}<a class="id" href={f.url} target="_blank" rel="noreferrer" title={f.title || f.id}>{f.id}</a>{:else}<span class="id" title={f.title}>{f.id}</span>{/if}
        {/if}
        {#if g.fresh}<span class="new">{many ? t('{n} new', { n: g.fresh }) : t('new')}</span>{/if}
      </span>
      {#if project}
        {@const others = g.projects.filter((p) => p !== project)}
        {#if others.length || g.accepted}<small title={others.join(', ')}>{g.accepted ? t('accepted in .trivyignore') : ''}{g.accepted && others.length ? ' · ' : ''}{others.length ? t('also in {list}', { list: others.join(', ') }) : ''}</small>{/if}
      {:else}
        <small title={g.projects.join(', ')}>{g.projects.join(', ')}</small>
      {/if}
    </span>
    <span class="fix" title={t('Installed {installed} · fixed in {fixed}', { installed: g.installed.join(', '), fixed: g.fixed })}><span class="ver">{g.installed[0]}{#if g.installed.length > 1}<span class="more">+{g.installed.length - 1}</span>{/if}</span>→<span class="ver ok">{g.fixed}</span></span>
    {#if a.href}<a class="act" href={a.href} onclick={claudeClick(a)} title={`${a.prompt}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{a.label}</a>{:else}<span class="act off" title={a.why}><Logo name="agent" size={10} />{a.label}</span>{/if}
  </div>
  {#if isOpen}
    <div class="cves">
      {#each g.findings as f (f.id + f.installed)}
        <div class="cv">
          <span class="dot {f.severity}" title={f.severity}></span>
          {#if f.url}<a class="id" href={f.url} target="_blank" rel="noreferrer">{f.id}</a>{:else}<span class="id">{f.id}</span>{/if}
          <span class="ttl" title={f.title}>{f.title}</span>
          {#if f.new}<span class="new">{t('new')}</span>{/if}
          <span class="ver">{f.installed}</span>→<span class="ver ok">{f.fixed}</span>
        </div>
      {/each}
    </div>
  {/if}
{/snippet}

<!-- An image as scanned: its digest (copied in a click), its platform, what the registry says now. -->
{#snippet imageLine(p: VulnProject, img: ScannedImage)}
  {@const st = imageState(p)}
  {@const digest = img.digest ?? img.pulled}
  <div class="img">
    <button class="digest" onclick={() => digest && copy(digest, t('Digest copied'))} title={[img.reference, digest ?? t('digest unknown'), img.platform, img.created ? t('built {ago}', { ago: ago(img.created) }) : '', t('scanned {ago}', { ago: ago(img.scanned_at) }), img.gcp ? t('read from GCP') : ''].filter(Boolean).join('\n')}>{digest ? digest.replace(/^sha256:/, '').slice(0, 12) : t('no digest')}</button>
    {#if img.platform !== 'linux/amd64'}<span class="tag">{img.platform.replace(/^linux\//, '')}</span>{/if}
    <span class="muted" title={img.reference}>{img.reference.replace(/:[^:/]+$/, '')}</span>
    {#if st.kind === 'changed'}<span class="warn-pill" title={t('The registry’s {tag} now points to {digest}', { tag: p.image ?? '', digest: p.remote?.digest ?? '' })}>{t('changed since the scan')}</span>{/if}
    {#if p.remote?.newer}<span class="warn-pill" title={t('Scanned: {tag}', { tag: p.image ?? '' })}>{t('{tag} published', { tag: p.remote.newer.split(':').pop() ?? p.remote.newer })}</span>{/if}
    {#if st.kind === 'same' && !p.remote?.newer}<span class="ok-pill" title={t('Checked {ago}', { ago: ago(p.remote?.at) })}>{t('up to date')}</span>{/if}
    {#if st.kind === 'unknown'}<span class="muted" title={p.remote?.error ?? ''}>{t('registry unreadable')}</span>{/if}
    <span class="grow"></span>
    {#if hasCore()}<button class="again" class:lit={st.kind === 'changed' || !!p.remote?.newer} disabled={busy || checking} onclick={() => ask('vulnerabilities_rescan_image', { project: p.project })} title={t('Scan this image again, not the others')}><Icon name="refresh" />{t('Rescan')}</button>{/if}
  </div>
{/snippet}

<section class="panel" style="--src: var(--src-alert)" class:collapsed={isCollapsed('vulns')}>
  <h2>
    <span class="title"><Logo name="alert" />{t('Vulnerabilities')}<RepoChip id="vulnerabilities" /></span>
    <small title={t('{trivy} · HIGH and CRITICAL with a fix, as in CI; each component’s .trivyignore applies', { trivy: vulns?.trivy ?? 'Trivy' })}>
      {vulns ? `Trivy · ${[[vulns.branch, vulns.sha], ...(vulns.other_branches ?? [])].map(([b, sha]) => `${b} @ ${sha}`).join(' + ')} · ${vulns.scanning ? t('scanning…') : t('scanned {ago}', { ago: ago(vulns.scanned_at) })}` : t('local Trivy scan')}
    </small>
    {#if fresh}<span class="badge">+{fresh} new</span>{/if}
    {#if hasCore() && hasImages}<button class="check" disabled={busy || checking} onclick={() => ask('vulnerabilities_check')} title={t('Ask the registry whether each scanned image changed since, or a newer tag is out: nothing is pulled nor scanned')}>{checking ? t('Checking…') : t('Check the registry')}</button>{/if}
    {#if hasCore()}<button class="refresh" class:busy disabled={busy} onclick={rescan} title={busy ? t('Scanning…') : t('Scan again now')} aria-label={t('Scan again')}><Icon name="refresh" spin={busy} /></button>{/if}
    <BlockControls id="vulns" />
  </h2>
  {#if !vulns}
    <div class="empty">{error ? t('Unavailable: {error}', { error }) : t('First Trivy scan…')}</div>
  {:else if isShort('vulns')}
    <div class="short-body">
      <div class="srow">
        <span class="t">{open.length ? `${t('{n} to fix', { n: open.length })}${critical ? ` · ${t('{n} critical', { n: critical })}` : ''}` : t('Nothing to fix')}</span>
        {#if accepted.length}<span class="pill">{accepted.length} accepted</span>{/if}
        <span class="muted">{ago(vulns.scanned_at)}</span>
      </div>
    </div>
  {:else}
    {#if error}<div class="warn-line" title={error}>{t('Last scan failed: {error}', { error })}</div>{/if}
    <div class="kpis">
      <div class="kpi"><span>{t('To fix')}</span><b class:ko={critical > 0} class:okc={!open.length}>{open.length}</b><small>{t('{critical} critical · {high} high', { critical, high: open.length - critical })}</small></div>
      <div class="kpi"><span>{t('Accepted')}</span><b>{accepted.length}</b><small>{t('in .trivyignore')}</small></div>
      <div class="kpi"><span>{t('Clean')}</span><b>{clean}/{vulns.projects.length}</b><small>{failed ? tn(failed, '{n} image unreadable', '{n} images unreadable') : t('components and images')}</small></div>
    </div>
    <div class="tb">
      <span class="lbl">{t('Show')}</span>
      {#each groups.filter((g) => g.total || g.key !== 'base') as g (g.key)}
        <label class="chk" title={g.title}>
          <input type="checkbox" checked={show[g.key]} onchange={() => toggleShow(g.key)} />{g.check}<small>{g.total}</small>
        </label>
      {/each}
      <span class="grow"></span>
      <button class="sw" class:on={hideIgnored} role="switch" aria-checked={hideIgnored} onclick={toggleIgnored} title={t('Hide the CVEs ignored in a .trivyignore (accepted)')}><i></i>{t('Hide ignored')}</button>
      <button class="sw" class:on={hideClean} role="switch" aria-checked={hideClean} onclick={toggleClean} title={t('Hide the components and images with nothing to report')}><i></i>{t('Hide clean')}</button>
    </div>
    <div class="scroll">
      {#each groups as g (g.key)}
        {#if g.total && show[g.key]}
          {@const cleanCount = g.items.filter(isClean).length}
          <h5>
            {g.label} <span class="hint">{g.items.some((p) => p.critical + p.high > 0 || p.error) ? t('{clean}/{total} clean', { clean: g.total - g.items.filter((p) => p.critical + p.high > 0 || p.error).length, total: g.total }) : t('all {n} clean', { n: g.total })}{hideClean && cleanCount ? ` · ${t('{n} hidden', { n: cleanCount })}` : ''}</span>
          </h5>
          {#each hideClean ? g.items.filter((p) => !isClean(p)) : g.items as p (p.project)}
            {@const open = expanded.includes(p.project)}
            {@const own = p.error ? [] : findingsOf(p.project)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="proj" class:openable={own.length > 0} class:open class:clean={isClean(p)} onclick={() => own.length && toggleProject(p.project)}>
              <b class="name" title={p.image ?? p.project}>{#if own.length}<span class="caret" class:open aria-hidden="true"><Icon name="chevronRight" /></span>{/if}{p.project}{#if p.image}<span class="tag">{p.image.split(':').pop()}</span>{/if}{#if p.note}<span class="note" title={p.note}>{p.note}</span>{/if}</b>
              {#if p.error}
                <span class="err" title={p.error}>{p.error}</span>
              {:else if isClean(p)}
                <span class="ok-mark" title={t('No HIGH or CRITICAL vulnerability with a fix')}><Icon name="check" /> {tc('mrs', 'clean')}</span>
              {:else}
                <span class="counts">
                  <i class="c" class:z={!p.critical} title={t('Critical to fix')}>{p.critical}</i><i class="h" class:z={!p.high} title={t('High to fix')}>{p.high}</i>
                </span>
                <span class="muted">{p.accepted && !hideIgnored ? t('{n} accepted', { n: p.accepted }) : ''}</span>
                {#if p.new}<span class="new">+{p.new} new</span>{:else}<span></span>{/if}
              {/if}
            </div>
            {#if p.scanned}{@render imageLine(p, p.scanned)}{/if}
            {#if open && own.length}
              <div class="detail" role="group" aria-label={t('Vulnerabilities of {project}', { project: p.project })}>
                {#each byPackage(own) as g (g.package)}{@render pkgRow(g, `p:${p.project}`, p.project)}{/each}
              </div>
            {/if}
          {/each}
        {/if}
      {/each}
      <h5>{t('To fix')} {#if listOpen.length}<b>{listOpen.length}</b> {tn(groupsOpen.length, 'CVEs in {n} library', 'CVEs in {n} libraries')}{/if}{#if listOpen.length < open.length}<span class="hint">{t('{n} in hidden groups', { n: open.length - listOpen.length })}</span>{/if}</h5>
      {#each groupsOpen as g (g.package)}{@render pkgRow(g, 'open')}{:else}
        <p class="none ok"><span class="fun-glint"><Icon name="secure" size="15px" /></span> {t('Nothing to fix: every HIGH or CRITICAL vulnerability with a fix is patched or accepted.')}</p>
      {/each}
      {#if listAccepted.length && !hideIgnored}
        <button class="fold" aria-expanded={showAccepted} onclick={toggleAccepted}>
          <span class="caret" class:open={showAccepted}><Icon name="chevronRight" /></span>{t('Accepted in .trivyignore')} <b>{listAccepted.length}</b>
          <span class="hint">{t('a fix exists: worth revisiting')}</span>
        </button>
        {#if showAccepted}{#each groupsAccepted as g (g.package)}{@render pkgRow(g, 'accepted')}{/each}{/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  section { container-type: inline-size; }
  .badge { padding: 0 6px; border-radius: var(--r9); background: var(--ko-bg); color: var(--ko); font-size: var(--fs-tag); font-weight: 500; white-space: nowrap; }
  .refresh { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 20px; height: 20px; margin-left: auto; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 14px; line-height: 1; cursor: pointer; }
  .refresh:hover { background: var(--surface-muted); color: var(--text); }
  .refresh.busy { cursor: progress; }
  .check + .refresh { margin-left: 4px; }
  .warn-line { padding: 4px 12px; background: var(--warn-bg); color: var(--warn); font-size: var(--fs-tag); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kpis { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 1px; background: var(--border); border-bottom: 1px solid var(--border); }
  .kpi { display: flex; flex-direction: column; gap: 1px; min-width: 0; padding: 6px 12px; background: var(--surface); }
  .kpi span { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.3px; }
  .kpi b { font-size: 16px; font-weight: 600; }
  .kpi b.ko { color: var(--ko); }
  .kpi b.okc { color: var(--ok); }
  .kpi small { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  h5 { display: flex; align-items: baseline; gap: 6px; margin: 0; padding: 5px 12px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); background: var(--bg); border-bottom: 1px solid var(--border); }
  h5 b { color: var(--ko); }
  .proj { display: grid; grid-template-columns: minmax(0, 1fr) auto 90px 56px; align-items: center; gap: 10px; padding: 3px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .proj.openable { cursor: pointer; }
  .proj.clean .name { font-weight: 400; color: var(--text-muted); }
  .ok-mark { grid-column: span 3; display: inline-flex; align-items: center; gap: 3px; color: var(--ok); font-size: var(--fs-tag); }
  .proj.openable:hover { background: color-mix(in srgb, var(--src-alert) 6%, var(--surface)); }
  .proj.open { background: color-mix(in srgb, var(--src-alert) 9%, var(--surface)); }
  .name .caret { width: 12px; margin-right: 2px; color: var(--text-muted); }
  .detail { border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--src-alert) 4%, var(--surface)); }
  .detail .cve { padding-left: 26px; }
  .detail .cve:last-child { border-bottom: none; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .tag { margin-left: 6px; padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); font-weight: 400; }
  .note { margin-left: 6px; color: var(--text-muted); font-size: var(--fs-tag); font-weight: 400; }
  .err { grid-column: span 3; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--warn); font-size: var(--fs-tag); }
  .img { display: flex; align-items: center; gap: 6px; padding: 1px 12px 3px 26px; border-bottom: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; }
  .img .muted { overflow: hidden; text-overflow: ellipsis; }
  .img .grow { flex: 1; }
  .digest { padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); background: var(--surface); color: var(--text); font-family: var(--mono); font-size: var(--fs-tag); cursor: copy; }
  .warn-pill, .ok-pill { padding: 0 6px; border-radius: var(--r6); font-weight: 600; }
  .warn-pill { background: color-mix(in srgb, var(--warn) 16%, var(--surface)); color: var(--warn); }
  .ok-pill { background: color-mix(in srgb, var(--ok) 14%, var(--surface)); color: var(--ok); }
  .again { display: inline-flex; align-items: center; gap: 3px; padding: 0 5px; border: 1px solid transparent; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .again:hover:not(:disabled), .again.lit { border-color: var(--border); color: var(--text); }
  .check { margin-left: auto; padding: 1px 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  .check:hover:not(:disabled) { color: var(--text); }
  .tb { display: flex; align-items: center; gap: 10px; padding: 4px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .tb .lbl { text-transform: uppercase; letter-spacing: 0.4px; }
  .tb .grow { flex: 1; }
  /* On the phone: the switches go to the next line rather than off the edge. */
  :global(html.remote) .tb { flex-wrap: wrap; row-gap: 6px; }
  .chk { display: inline-flex; align-items: center; gap: 4px; color: var(--text); cursor: pointer; }
  .chk input { margin: 0; --tick: var(--src-alert); }
  .chk small { color: var(--text-muted); }
  .sw { display: inline-flex; align-items: center; gap: 4px; padding: 0; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); font-weight: 400; text-transform: none; letter-spacing: 0; white-space: nowrap; cursor: pointer; }
  .sw:hover { color: var(--text); }
  .sw i { position: relative; width: 20px; height: 11px; border-radius: var(--r6); background: var(--surface-muted); border: 1px solid var(--border); }
  .sw i::after { content: ''; position: absolute; top: 1px; left: 1px; width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); transition: left 0.12s; }
  .sw.on i { background: color-mix(in srgb, var(--violet) 35%, var(--surface)); border-color: var(--violet); }
  .sw.on i::after { left: 10px; background: var(--violet); }
  h5 .hint { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; }
  .counts { display: inline-flex; gap: 3px; }
  .counts i { min-width: 22px; padding: 0 4px; border-radius: var(--r4); font-style: normal; font-family: var(--mono); font-size: var(--fs-tag); text-align: center; }
  .counts .c { background: var(--ko); color: var(--surface); }
  .counts .h { background: var(--ko-bg); color: var(--ko); }
  .counts .z { opacity: 0.35; }
  .muted { color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .new { padding: 0 6px; border-radius: var(--r9); background: var(--ko-bg); color: var(--ko); font-size: var(--fs-tag); white-space: nowrap; }
  .cve { display: grid; grid-template-columns: 62px minmax(0, 1fr) auto auto; align-items: center; gap: 10px; padding: 4px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .cve.accepted { opacity: 0.8; }
  .sev { justify-self: start; padding: 0 5px; border-radius: var(--r4); font-size: var(--fs-tag); text-align: center; min-width: 56px; }
  .sev.critical { background: var(--ko); color: var(--surface); }
  .sev.high { background: var(--ko-bg); color: var(--ko); }
  .pkg { display: flex; flex-direction: column; min-width: 0; }
  .l1 { display: flex; align-items: baseline; gap: 6px; min-width: 0; white-space: nowrap; }
  .id { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; }
  a.id:hover { color: var(--info); }
  .count { display: inline-flex; align-items: center; gap: 4px; padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r9); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .count:hover, .count.open { color: var(--text); border-color: var(--src-alert); }
  .count .caret { font-size: 9px; }
  .ver .more { margin-left: 3px; color: var(--text-muted); }
  /* CVEs of an expanded library: one row each, under the library. */
  .cves { padding: 2px 12px 6px 84px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--src-alert) 4%, var(--surface)); }
  .cv { display: grid; grid-template-columns: 8px auto minmax(0, 1fr) auto auto auto auto; align-items: center; gap: 6px; padding: 2px 0; font-size: var(--fs-tag); color: var(--text-muted); }
  .cv .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ko-bg); box-shadow: inset 0 0 0 1px var(--ko); }
  .cv .dot.critical { background: var(--ko); }
  .cv .ttl { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pkg small { font-size: var(--fs-tag); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fix { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); white-space: nowrap; }
  .ver { padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text); font-family: var(--mono); font-size: var(--fs-tag); }
  .ver.ok { background: var(--ok-bg); color: var(--ok); }
  .act { display: inline-flex; flex-shrink: 0; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--src-claude) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; }
  .act:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); text-decoration: none; }
  .act.off { opacity: 0.5; cursor: not-allowed; }
  .fold { display: flex; align-items: baseline; gap: 6px; width: 100%; padding: 5px 12px; border: none; border-bottom: 1px solid var(--border); background: var(--bg); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; text-align: left; cursor: pointer; }
  .fold b { color: var(--text); }
  .fold .hint { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; }
  .caret { display: inline-flex; font-size: 11px; transition: transform 0.12s; }
  .caret.open { transform: rotate(90deg); }
  .none { margin: 6px 12px; color: var(--text-muted); font-size: var(--fs-meta); }
  .none.ok { display: flex; align-items: center; gap: 6px; color: var(--ok); }
  @container (max-width: 520px) {
    .kpis { grid-template-columns: repeat(3, minmax(0, 1fr)); }
    .cve { grid-template-columns: 62px minmax(0, 1fr) auto; }
    .fix { display: none; }
    .fold .hint { display: none; }
  }
</style>
