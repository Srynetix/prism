import type { Dictionary } from './index'

/** French, every file of `i18n/fr/` in one: a chunk of its own, read only by a window in French. */
const files = import.meta.glob<{ default: Dictionary }>('./fr/*.ts', { eager: true })
export default Object.assign({}, ...Object.values(files).map((f) => f.default)) as Dictionary
