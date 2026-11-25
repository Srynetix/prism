/**
 * A container's files (`ContainerFiles.svelte`): a pod's, from its logs pane, or a container's of
 * the Docker block. Its folders listed, a text file shown, a file or a folder (`.tar.gz`)
 * downloaded, files sent into the open folder (`[docker] files`, `[clusters] files` = "write").
 * On a phone: listing, showing and downloading only.
 */
import { invoke, isDesktop } from './bridge'
import { t } from './i18n.svelte'
import { live } from './live.svelte'
import type { AgentDownload } from './bindings/AgentDownload'
import type { ContainerFileText } from './bindings/ContainerFileText'
import type { ContainerListing } from './bindings/ContainerListing'
import type { ContainerRef } from './bindings/ContainerRef'

export type { ContainerRef }

/** A file dropped on the window is sent as bytes (base64): beyond this, the Send button's dialog. */
export const DROP_MAX = 64 << 20

interface Shown {
  path: string
  text: ContainerFileText | null
  loading: boolean
  error: string | null
}

/** Files waiting for a second look before they are sent: into prod, or replacing some. */
interface Pending {
  /** Paths of the computer (the Send dialog), or files dropped. */
  paths: string[]
  files: File[]
  replaced: string[]
}

export const containerFiles = $state({
  open: false,
  target: null as ContainerRef | null,
  /** What the drawer's head names: the container, or its pod and container. */
  label: '',
  /** Files may be sent into it (`write`, and the computer's window). */
  write: false,
  /** A production environment's container: sending always asks again. */
  prod: false,
  /** The folder asked for (empty: the container's working folder), then the one listed. */
  path: '',
  listing: null as ContainerListing | null,
  loading: false,
  error: null as string | null,
  file: null as Shown | null,
  /** What runs: a download or a send, by its path. */
  busy: null as string | null,
  /** The last thing done (`2 files sent`), or why it failed. */
  note: null as { ok: boolean; text: string } | null,
  pending: null as Pending | null,
})

/** `name` in the folder `dir`. */
export const joinPath = (dir: string, name: string) => `${dir.replace(/\/+$/, '')}/${name}`

/** The folder above `path` (`/` stays `/`). */
export function parentPath(path: string): string {
  const p = path.replace(/\/+$/, '')
  const i = p.lastIndexOf('/')
  return i <= 0 ? '/' : p.slice(0, i)
}

/** The segments of an absolute path, each with the path it leads to: `/`, `/var`, `/var/log`. */
export function crumbs(path: string): { name: string; path: string }[] {
  const out = [{ name: '/', path: '/' }]
  let at = ''
  for (const part of path.split('/').filter(Boolean)) {
    at += `/${part}`
    out.push({ name: part, path: at })
  }
  return out
}

/** The name a download is saved under: the file's, or the folder's as a `.tar.gz`. */
export function downloadName(path: string, dir: boolean): string {
  const base = path.replace(/\/+$/, '').split('/').pop() || 'root'
  return dir ? `${base}.tar.gz` : base
}

/** The names of `names` the folder already has: those a send replaces. */
export function replacedBy(names: string[], listing: ContainerListing | null): string[] {
  const there = new Set((listing?.entries ?? []).filter((e) => !e.dir).map((e) => e.name))
  return names.filter((n) => there.has(n))
}

/** A pod of a production environment (`[[environments]] env = "prod"` on its context). */
export function isProdContext(context: string): boolean {
  return (live.snapshot?.environments?.envs ?? []).some((e) => e.context === context && /^prod/i.test(e.env))
}

export function openContainerFiles(target: ContainerRef, label: string, opts: { write: boolean; prod?: boolean }) {
  Object.assign(containerFiles, { open: true, target, label, write: opts.write && isDesktop(), prod: !!opts.prod, path: '', listing: null, error: null, file: null, busy: null, note: null, pending: null })
  void go('')
}

export function closeContainerFiles() {
  Object.assign(containerFiles, { open: false, target: null, listing: null, file: null, pending: null, note: null, error: null })
}

/** Incremented on each listing: an older one answering late is ignored. */
let seq = 0

/** Lists the folder `path` (empty: the container's working folder). */
export async function go(path: string) {
  const target = containerFiles.target
  if (!target) return
  const mine = ++seq
  Object.assign(containerFiles, { path, loading: true, error: null, file: null, pending: null })
  try {
    const listing = await invoke<ContainerListing>('container_files', { target, path })
    if (mine !== seq) return
    Object.assign(containerFiles, { listing, path: listing.path, loading: false })
  } catch (e) {
    if (mine !== seq) return
    Object.assign(containerFiles, { listing: null, loading: false, error: String(e) })
  }
}

export const reloadFolder = () => go(containerFiles.listing?.path ?? containerFiles.path)

/** Shows the file `path` (its first MB). */
export async function show(path: string) {
  const target = containerFiles.target
  if (!target) return
  containerFiles.file = { path, text: null, loading: true, error: null }
  try {
    const text = await invoke<ContainerFileText>('container_file_text', { target, path })
    if (containerFiles.file?.path === path) containerFiles.file = { path, text, loading: false, error: null }
  } catch (e) {
    if (containerFiles.file?.path === path) containerFiles.file = { path, text: null, loading: false, error: String(e) }
  }
}

/** Downloads a file, or a folder (`dir`) as a `.tar.gz`: where the computer's dialog says, or into
 * the phone's downloads. */
export async function download(path: string, dir: boolean) {
  const target = containerFiles.target
  if (!target || containerFiles.busy) return
  const name = downloadName(path, dir)
  containerFiles.busy = path
  containerFiles.note = null
  try {
    if (isDesktop()) {
      const [{ save }, { downloadDir, join }] = await Promise.all([import('@tauri-apps/plugin-dialog'), import('@tauri-apps/api/path')])
      const dest = await save({ defaultPath: await join(await downloadDir(), name) })
      if (!dest) return
      await invoke('container_files_export', { target, path, dir, dest })
      containerFiles.note = { ok: true, text: t('{name} downloaded', { name }) }
    } else {
      const d = await invoke<AgentDownload>('container_files_download', { target, path, dir })
      const link = Object.assign(document.createElement('a'), { href: d.url, download: d.name })
      document.body.append(link)
      link.click()
      link.remove()
      containerFiles.note = { ok: true, text: t('{name} downloaded', { name: d.name }) }
    }
  } catch (e) {
    containerFiles.note = { ok: false, text: String(e) }
  } finally {
    containerFiles.busy = null
  }
}

/** Send…: files of the computer picked in its dialog, into the open folder. */
export async function pickAndSend() {
  if (!containerFiles.write || !containerFiles.listing) return
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ multiple: true, directory: false })
  const paths = picked === null ? [] : Array.isArray(picked) ? picked : [picked]
  if (paths.length) offer({ paths, files: [] })
}

/** Files dropped on the drawer, into the open folder. */
export function dropFiles(files: File[]) {
  if (!containerFiles.write || !containerFiles.listing || !files.length) return
  const big = files.find((f) => f.size > DROP_MAX)
  if (big) {
    containerFiles.note = { ok: false, text: t('{name} is over {size} MB: send it with Send…', { name: big.name, size: DROP_MAX >> 20 }) }
    return
  }
  offer({ paths: [], files })
}

const nameOf = (p: string) => p.split(/[\\/]/).pop() ?? p

/** Sent at once, unless it goes into prod or replaces files: then it waits for Send. */
function offer(what: { paths: string[]; files: File[] }) {
  const names = [...what.paths.map(nameOf), ...what.files.map((f) => f.name)]
  const replaced = replacedBy(names, containerFiles.listing)
  if (containerFiles.prod || replaced.length) containerFiles.pending = { ...what, replaced }
  else void send(what)
}

export function cancelSend() {
  containerFiles.pending = null
}

export async function confirmSend() {
  const p = containerFiles.pending
  if (!p) return
  containerFiles.pending = null
  await send(p)
}

async function send({ paths, files }: { paths: string[]; files: File[] }) {
  const target = containerFiles.target
  const dir = containerFiles.listing?.path
  if (!target || !dir || containerFiles.busy) return
  containerFiles.busy = dir
  containerFiles.note = null
  try {
    if (paths.length) await invoke<number>('container_files_upload', { target, dir, sources: paths })
    for (const f of files) await invoke('container_files_put', { target, dir, name: f.name, data: await base64(f) })
    const n = paths.length + files.length
    containerFiles.note = { ok: true, text: n === 1 ? t('{name} sent into {dir}', { name: nameOf(paths[0] ?? files[0].name), dir }) : t('{n} files sent into {dir}', { n, dir }) }
    if (containerFiles.listing?.path === dir) await go(dir)
  } catch (e) {
    containerFiles.note = { ok: false, text: String(e) }
  } finally {
    containerFiles.busy = null
  }
}

/** A file's bytes in base64. */
function base64(f: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result).replace(/^data:[^,]*,/, ''))
    r.onerror = () => reject(r.error)
    r.readAsDataURL(f)
  })
}
