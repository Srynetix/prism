<script lang="ts">
  import { t } from '../i18n.svelte'
  import { forgeLink, onGithubWeb } from '../forge'
  import { hasCore, invoke } from '../bridge'
  import Icon from './Icon.svelte'

  /**
   * Image from a ClickUp ticket, or from a forge's text (`forge`: the project's web address), loaded
   * by the Rust core (the window's CSP allows no external image) and kept in
   * memory for the session. `fetched = false`: never read, a link (an agent's words: any address
   * it writes would be fetched unasked). On the forge's side, only its attachments are read (uploaded to the
   * GitLab project, attached on GitHub); an image from elsewhere stays a link. Outside the app, or
   * on failure, a link.
   */
  let {
    url, alt = t('image'), onopen, forge, width, fetched = true,
  }: { url: string; alt?: string; onopen?: () => void; forge?: string; width?: number; fetched?: boolean } = $props()

  const UPLOAD = /\/uploads\/[0-9a-f]{32}\/[^/?#]+/
  const ATTACHMENT = /^https:\/\/(github\.com\/user-attachments\/|([a-z0-9-]+\.)?githubusercontent\.com\/)/
  /** What the fallback link opens: a relative path is opened on the forge. */
  const href = $derived(url.startsWith('/') && forge ? forgeLink(forge, url) : url)
  /** On the forge's side, only its attachments are read (not any URL). */
  const loadable = $derived(fetched && (!forge || (onGithubWeb(forge) ? ATTACHMENT : UPLOAD).test(url)))

  let src = $state<string | null>(null)
  let error = $state<string | null>(null)

  $effect(() => {
    const u = url
    src = null
    error = null
    if (!hasCore() || !loadable) return
    load(u, forge ? 'forge_upload' : 'ticket_image', forge)
      .then((s) => u === url && (src = s))
      .catch((e) => u === url && (error = String(e)))
  })
</script>

<script module lang="ts">
  const cache = new Map<string, Promise<string>>()
  /** `forge`: the project's web address; its path (`group/project`) says where to read the file. */
  function load(url: string, command: 'ticket_image' | 'forge_upload', forge?: string): Promise<string> {
    const key = `${forge ?? ''}${url}`
    let p = cache.get(key)
    if (!p) {
      const project = forge ? new URL(forge).pathname.replace(/^\/|\/$/g, '') : null
      p = invoke<string>(command, command === 'forge_upload' ? { path: url, project } : { url })
      p.catch(() => cache.delete(key))
      cache.set(key, p)
    }
    return p
  }
</script>

{#if src}
  {#if onopen}
    <button class="img" onclick={onopen} title={t('{alt} · click to enlarge', { alt })} style:max-width={width ? `min(${width}px, 100%)` : undefined}><img {src} {alt} /></button>
  {:else}
    <img class="plain" {src} {alt} style:max-width={width ? `min(${width}px, 100%)` : undefined} />
  {/if}
{:else if error || !hasCore() || !loadable}
  <a class="fallback" {href} target="_blank" rel="noreferrer" title={error ?? href}><Icon name="image" /> {alt}</a>
{:else}
  <span class="loading" aria-label={t('Loading {alt}', { alt })}><Icon name="image" /> {alt}…</span>
{/if}

<style>
  .img { display: block; max-width: 100%; padding: 0; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); overflow: hidden; cursor: zoom-in; }
  .img img, .plain { display: block; max-width: 100%; max-height: 420px; object-fit: contain; }
  .fallback, .loading { font-size: var(--fs-meta); }
  .loading { color: var(--text-muted); }
</style>
