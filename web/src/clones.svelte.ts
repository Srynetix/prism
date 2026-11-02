import { invoke, isDesktop } from './bridge'
import { pref, setPref } from './layout.svelte'
import { live } from './live.svelte'
import { wizard, wizardDue } from './wizard.svelte'
import type { ForgeProvider } from './bindings/ForgeProvider'
import { t } from './i18n.svelte'
import type { Folder } from './bindings/Folder'
import type { Created } from './bindings/Created'
export type { Folder, Created }

/**
 * The local clones of the repositories (`[[repos]] path`): a pack names a repository and its
 * project, each member says where their clone is. Found in a folder the user picks (nothing is
 * scanned unasked), chosen and checked against the project's remote, or cloned with glab or gh.
 */

/** A repository to give a clone: `note` says why (none, or its folder is gone). */
export interface CloneItem {
  name: string
  project: string | null
  note: string
}

/** The clones of `projects` in `root` and below (a few levels), recognized by their remote. */
export const findClones = (root: string, projects: string[]) => invoke<{ project: string; path: string }[]>('find_clones', { root, projects })

/** Clones `project` into `parent/<its name>`; its path, `~/…`. */
export const cloneRepo = (project: string, parent: string, forge: ForgeProvider | null = null) => invoke<string>('clone_repo', { project, parent, forge })

/** Is `path` a clone of `project`? Null: yes; otherwise why not. */
export async function notCloneOf(path: string, project: string | null): Promise<string | null> {
  const o = await invoke<{ ok: boolean; message: string }>('check_setting', { check: project ? { kind: 'git_repo', path, project } : { kind: 'git_repo', path } })
  return o.ok ? null : o.message
}

/* ---------- At launch ---------- */

export const clonesPrompt = $state({ open: false })

/** Repositories offered once: closing the prompt leaves them to Settings › Repositories. */
const skipped = () => pref<string[]>('clones.offered', [])

/** The repositories read at launch without a usable clone, and with a project to find it by. */
export const missingClones = (): CloneItem[] =>
  (live.snapshot?.repositories ?? [])
    .filter((r) => (!r.clone || r.clone_missing) && r.project)
    .map((r) => ({ name: r.name, project: r.project, note: r.clone_missing ? t('its folder is gone') : t('no local clone') }))

/** After the first launch's setup and the pages wizard: the clones not offered yet. */
export const clonesDue = () =>
  isDesktop() && !!live.snapshot && !live.snapshot.profile?.fresh && !wizard.open && !wizardDue() && missingClones().some((c) => !skipped().includes(c.name))

export function closeClonesPrompt() {
  clonesPrompt.open = false
  setPref('clones.offered', [...new Set([...skipped(), ...missingClones().map((c) => c.name)])])
}

/* ---------- Adding a repository ---------- */

export const inspectFolder = (path: string) => invoke<Folder>('inspect_folder', { path })

/** `git init` in a folder that is not a repository, on the main branch. */
export const initRepo = (path: string) => invoke<void>('init_repo', { path })

/** A project to create on a forge: its group or owner, visibility, and forge (absent: the profile's). */
export interface NewRemote {
  namespace: string
  visibility: string
  forge?: ForgeProvider | null
}

/** `parent/name` with a git repository, a README and a first commit; with `remote`, the forge's project, pushed. */
export const createProject = (parent: string, name: string, remote: NewRemote | null) =>
  invoke<Created>('create_project', { parent, name, remote })

/** A local repository without a remote, its project created on the forge and pushed: that project. */
export const publishRepo = (path: string, remote: NewRemote) => invoke<string>('publish_repo', { path, remote })

/** Settings › Repositories' Add panel, opened from the Repos menu or the palette; `publish`: the repository to publish, by name. */
export type AddMode = 'folder' | 'clone' | 'new' | 'publish'
/**
 * The Add panel's mode, the repository to publish, the working folder a header opened it on, and
 * the working folder whose keys are being edited in its header (its index in `[[workspaces]]`).
 */
export const repoAdd = $state({ mode: null as AddMode | null, publish: '', workspace: '', editing: null as number | null })
