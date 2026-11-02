import { live } from './live.svelte'
import { gitlabUrl } from './conventions'
import type { ForgeProvider } from './bindings/ForgeProvider'

/** On GitHub (`[forge] provider`) rather than GitLab; `on`: an MR, on its own forge (`mrForge`). */
export const onGithub = (on?: { forge?: ForgeProvider | null } | null): boolean => (on?.forge ?? live.snapshot?.forge) === 'github'

/** The forge's name, for labels; `forge`: absent, the profile's. */
export const forgeName = (forge?: ForgeProvider | null): string => ((forge ?? profileForge()) === 'github' ? 'GitHub' : 'GitLab')

/** The forge's logo (`Logo` name). */
export const forgeLogo = (): 'github' | 'gitlab' => (onGithub() ? 'github' : 'gitlab')

/** The profile's forge (`[forge] provider`). */
export const profileForge = (): ForgeProvider => (onGithub() ? 'github' : 'gitlab')

/** A forge's name, for labels; absent: the profile's. */
export const forgeLabel = (forge?: ForgeProvider | null): string => ((forge ?? profileForge()) === 'github' ? 'GitHub' : 'GitLab')

/** Web page of a project (`group/project`, `owner/repo`), on `forge` (absent: the profile's). */
export const forgeUrl = (project: string, forge?: ForgeProvider | null): string =>
  (forge ?? profileForge()) === 'github' ? `https://github.com/${project.replace(/^\//, '')}` : gitlabUrl(project)

/** On GitHub: `forge` (an MR's, see `mrForge`), or the profile's when absent. */
const github = (forge?: ForgeProvider | null): boolean => (forge ?? profileForge()) === 'github'

/** A project's web address (`forgeUrl`) is on GitHub: Prism only knows github.com there. */
export const onGithubWeb = (web: string): boolean => /^https:\/\/github\.com\//.test(web)

/**
 * A root-relative link of a forge text (`/uploads/…`), against the project's web address `web`:
 * GitLab reads it in the project, GitHub at its root.
 */
export const forgeLink = (web: string, path: string): string => (onGithubWeb(web) ? new URL(path, web).href : web + path)

/**
 * The forge of an MR when it is not the profile's (`[[repos]] forge`): its own, or its repository's
 * for a mere reference. Null: the profile's.
 */
export const mrForge = (r: { repo?: string | null; forge?: ForgeProvider | null }): ForgeProvider | null =>
  r.forge ?? (r.repo ? (live.snapshot?.repositories.find((x) => x.name === r.repo)?.forge ?? null) : null)

/** Sign of a merge request's number: `!12` on GitLab, `#12` on GitHub; `forge`: absent, the profile's. */
export const mrSign = (forge?: ForgeProvider | null): string => (github(forge) ? '#' : '!')

/** A merge request's number as its forge writes it. */
export const mrNumber = (iid: number | string, forge?: ForgeProvider | null): string => `${mrSign(forge)}${iid}`

/** `MR` on GitLab, `PR` on GitHub. */
export const mrWord = (forge?: ForgeProvider | null): string => (github(forge) ? 'PR' : 'MR')

/**
 * Your login on an MR's forge: the profile's (`me`), or the other forge's (`me_elsewhere`) for an
 * MR of a repository there.
 */
export const meOn = (s: { me: string | null; me_elsewhere?: string | null } | null | undefined, m: { forge?: ForgeProvider | null }): string | null =>
  (m.forge ? s?.me_elsewhere : s?.me) ?? null
