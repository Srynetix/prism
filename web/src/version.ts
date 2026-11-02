/**
 * Prism version and the commit it was built from, injected by Vite (vite.config.ts). Outside this
 * config (another dev server), `dev` and no commit.
 */
declare const __PRISM_VERSION__: string
declare const __PRISM_COMMIT__: string
declare const __PRISM_EXTRA__: string

export const VERSION = typeof __PRISM_VERSION__ === 'string' ? __PRISM_VERSION__ : 'dev'
/** Short hash; `+` at the end: built with uncommitted changes. Empty without git. */
export const COMMIT = typeof __PRISM_COMMIT__ === 'string' ? __PRISM_COMMIT__ : ''
/** The team's blocks built in (`PRISM_EXTRA_BLOCKS`): their name and commit, `acme 5786d86`. Empty without. */
export const EXTRA = typeof __PRISM_EXTRA__ === 'string' ? __PRISM_EXTRA__ : ''
/** `1.0.0 · 3e11fc2`, `1.0.0 · 3e11fc2 + acme 5786d86` with a team's blocks. */
export const versionLabel = () => `${COMMIT ? `${VERSION} · ${COMMIT}` : VERSION}${EXTRA ? ` + ${EXTRA}` : ''}`
