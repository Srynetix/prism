/**
 * The Files tab shown last: its agent's files offered by the ⌘K palette. A module of its own, so
 * that the palette (a phone's too) does not bring the editor, CodeMirror and all, with it.
 */
export const paletteFiles = $state<{ agent: string | null; title: string; paths: string[] }>({ agent: null, title: '', paths: [] })
