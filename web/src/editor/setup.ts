import { autocompletion, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, foldGutter, foldKeymap, HighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language'
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search'
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import { crosshairCursor, drawSelection, dropCursor, EditorView, highlightActiveLine, highlightActiveLineGutter, highlightSpecialChars, keymap, lineNumbers, rectangularSelection } from '@codemirror/view'
import { tags as t } from '@lezer/highlight'
import { lang } from '../i18n.svelte'

/**
 * The Files tab's editor: CodeMirror 6, its colors Prism's theme (the `--shiki-token-*` the
 * diffs use, app.css), so it follows the theme as it changes. What a setting or a file changes
 * lives in a compartment: language, Vim, wrapping, size, read-only, the Compare view.
 */

export const compartments = {
  language: new Compartment(),
  vim: new Compartment(),
  wrap: new Compartment(),
  size: new Compartment(),
  readOnly: new Compartment(),
  merge: new Compartment(),
}

export interface EditorPrefs {
  vim: boolean
  wrap: boolean
  fontSize: number
  readOnly: boolean
}

const tok = (name: string) => `var(--shiki-token-${name})`

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword, t.definitionKeyword, t.modifier], color: tok('keyword') },
  { tag: [t.string, t.special(t.string), t.regexp, t.character], color: tok('string') },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment, t.meta], color: tok('comment'), fontStyle: 'italic' },
  { tag: [t.number, t.bool, t.null, t.atom, t.constant(t.variableName), t.standard(t.variableName), t.escape], color: tok('constant') },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.function(t.definition(t.variableName)), t.macroName], color: tok('function') },
  { tag: [t.typeName, t.className, t.namespace, t.definition(t.typeName), t.attributeName, t.labelName], color: tok('parameter') },
  { tag: [t.punctuation, t.separator, t.bracket, t.operator, t.derefOperator], color: tok('punctuation') },
  { tag: [t.tagName, t.self], color: tok('keyword') },
  { tag: [t.propertyName, t.definition(t.propertyName)], color: 'var(--shiki-foreground)' },
  { tag: [t.link, t.url], color: tok('link'), textDecoration: 'underline' },
  { tag: t.heading, color: tok('keyword'), fontWeight: '700' },
  { tag: t.strong, fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },
  { tag: [t.inserted], color: 'var(--ok)' },
  { tag: [t.deleted], color: 'var(--ko)' },
  { tag: t.invalid, color: 'var(--ko)' },
])

const theme = EditorView.theme({
  '&': { height: '100%', color: 'var(--shiki-foreground, var(--text))', backgroundColor: 'var(--surface)' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: 'var(--mono)', lineHeight: '1.55' },
  '.cm-content': { caretColor: 'var(--src-claude)', padding: '4px 0' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--src-claude)', borderLeftWidth: '1.5px' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'color-mix(in srgb, var(--info) 24%, transparent) !important' },
  '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--text) 4%, transparent)' },
  '.cm-gutters': { backgroundColor: 'var(--surface)', color: 'var(--text-muted)', border: 'none' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text)' },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 6px 0 8px', minWidth: '32px', opacity: '0.7' },
  '.cm-foldGutter .cm-gutterElement': { padding: '0 2px', opacity: '0.6', cursor: 'pointer' },
  '.cm-foldPlaceholder': { backgroundColor: 'var(--surface-muted)', border: '1px solid var(--border)', color: 'var(--text-muted)', borderRadius: '4px', padding: '0 4px' },
  '.cm-matchingBracket': { backgroundColor: 'transparent', outline: '1px solid var(--border-strong, var(--border))', borderRadius: '2px' },
  '.cm-nonmatchingBracket': { color: 'var(--ko)' },
  '.cm-searchMatch': { backgroundColor: 'color-mix(in srgb, var(--warn) 30%, transparent)', borderRadius: '2px' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'color-mix(in srgb, var(--warn) 60%, transparent)' },
  '.cm-selectionMatch': { backgroundColor: 'color-mix(in srgb, var(--info) 14%, transparent)' },
  '.cm-tooltip': { backgroundColor: 'var(--surface)', border: '1px solid var(--border)', borderRadius: '6px', boxShadow: '0 6px 20px rgba(0,0,0,.14)', color: 'var(--text)' },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: 'color-mix(in srgb, var(--src-claude) 18%, transparent)', color: 'var(--text)' },
  // The search panel, at the top, in Prism's fields and buttons.
  '.cm-panels': { backgroundColor: 'var(--surface-muted)', color: 'var(--text)', fontFamily: 'var(--sans)', fontSize: 'var(--fs-meta)' },
  '.cm-panels-top': { borderBottom: '1px solid var(--border)' },
  '.cm-panels-bottom': { borderTop: '1px solid var(--border)' },
  '.cm-panel.cm-search': { padding: '5px 28px 5px 8px', display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: '4px 6px' },
  '.cm-panel.cm-search br': { flexBasis: '100%', height: 0 },
  '.cm-panel.cm-search label': { display: 'inline-flex', alignItems: 'center', gap: '3px', color: 'var(--text-muted)', fontSize: 'var(--fs-tag)' },
  '.cm-panel.cm-search [name=close]': { top: '4px', right: '6px', color: 'var(--text-muted)', fontSize: '15px', cursor: 'pointer' },
  '.cm-textfield': { padding: '2px 7px', border: '1px solid var(--border)', borderRadius: 'var(--r6)', background: 'var(--bg)', color: 'var(--text)', font: 'var(--fs-meta) var(--mono)', outline: 'none' },
  '.cm-textfield:focus': { borderColor: 'var(--src-claude)' },
  '.cm-button': { padding: '1px 8px', border: '1px solid var(--border)', borderRadius: 'var(--r6)', backgroundImage: 'none', background: 'var(--surface)', color: 'var(--text-muted)', font: 'inherit', fontSize: 'var(--fs-meta)', cursor: 'pointer' },
  '.cm-button:hover': { color: 'var(--text)' },
  // Vim's command line and status.
  '.cm-vim-panel': { padding: '2px 8px', fontFamily: 'var(--mono)' },
  '.cm-vim-panel input': { background: 'transparent', color: 'var(--text)', border: 'none', outline: 'none', font: 'inherit' },
  '.cm-fat-cursor': { background: 'color-mix(in srgb, var(--src-claude) 60%, transparent) !important', color: 'var(--surface) !important' },
  '&:not(.cm-focused) .cm-fat-cursor': { background: 'none !important', outline: '1px solid var(--src-claude)' },
  // The git gutter.
  '.cm-git-gutter': { width: '4px', cursor: 'pointer' },
  '.cm-git-gutter .cm-gutterElement': { padding: '0' },
  '.cm-git-add': { background: 'var(--ok)' },
  '.cm-git-mod': { background: 'var(--info)' },
  '.cm-git-del': { position: 'relative' },
  '.cm-git-del::after': { content: '""', position: 'absolute', left: '0', bottom: '-4px', borderStyle: 'solid', borderWidth: '4px 0 4px 5px', borderColor: 'transparent transparent transparent var(--ko)' },
  '.cm-git-peek': { margin: '2px 10px 6px 6px', border: '1px solid var(--border)', borderRadius: '6px', overflow: 'hidden', fontSize: 'var(--fs-meta)', background: 'var(--surface)' },
  '.cm-git-peek-head': { display: 'flex', alignItems: 'center', gap: '5px', padding: '3px 6px 3px 8px', background: 'var(--surface-muted)', fontFamily: 'var(--sans)' },
  '.cm-git-peek-kind': { color: 'var(--text-muted)' },
  '.cm-git-peek-grow': { flex: '1' },
  '.cm-git-peek-line': { padding: '0 8px', whiteSpace: 'pre', fontFamily: 'var(--mono)', background: 'color-mix(in srgb, var(--ko) 12%, transparent)' },
  '.cm-git-peek-note': { padding: '3px 8px', color: 'var(--text-muted)', fontFamily: 'var(--sans)' },
  // Compare: the agent's version against yours.
  '.cm-chunkButtons button': { padding: '0 8px', marginLeft: '4px', border: '1px solid var(--border)', borderRadius: 'var(--r6)', background: 'var(--surface)', color: 'var(--text)', font: 'var(--fs-tag) var(--sans)', cursor: 'pointer' },
  '.cm-chunkButtons button[name=accept]': { borderColor: 'var(--ok)', color: 'var(--ok)' },
  '.cm-chunkButtons button[name=reject]': { borderColor: 'var(--ko)', color: 'var(--ko)' },
  '.cm-deletedChunk': { backgroundColor: 'color-mix(in srgb, var(--ko) 10%, transparent)' },
  '.cm-changedLine': { backgroundColor: 'color-mix(in srgb, var(--ok) 8%, transparent)' },
  // A line the agent just changed (live reload).
  '.cm-flash': { animation: 'cm-flash 1.6s ease-out' },
  '@keyframes cm-flash': { from: { backgroundColor: 'color-mix(in srgb, var(--src-claude) 30%, transparent)' }, to: { backgroundColor: 'transparent' } },
})

/** CodeMirror's own words (its search panel, folds, Compare's buttons), in the window's language. */
const PHRASES: Record<string, Record<string, string>> = {
  fr: {
    Find: 'Chercher', Replace: 'Remplacer', next: 'suivant', previous: 'précédent', all: 'tous', 'match case': 'casse', regexp: 'regex', 'by word': 'mot entier',
    replace: 'remplacer', 'replace all': 'tout remplacer', close: 'fermer', 'current match': 'résultat courant', 'on line': 'à la ligne',
    'replaced $ matches': '$ résultats remplacés', 'replaced match on line $': 'résultat remplacé à la ligne $', 'Go to line': 'Aller à la ligne', go: 'aller',
    'folded code': 'code replié', unfold: 'déplier', to: 'à', 'Control character': 'Caractère de contrôle', Completions: 'Complétions',
    Accept: 'Garder', Reject: 'Rejeter', 'Revert this chunk': 'Annuler ce bloc', '$ unchanged lines': '$ lignes inchangées',
  },
}

const sizeTheme = (px: number) => EditorView.theme({ '.cm-scroller': { fontSize: `${px}px` } })

let vimExt: Extension | null = null
/** Vim's keys, loaded the first time they are asked for. */
export async function loadVim(): Promise<Extension> {
  if (!vimExt) vimExt = (await import('@replit/codemirror-vim')).vim({ status: true })
  return vimExt
}
/** Vim, loaded (`loadVim`): what `:w` runs. */
export async function vimApi() {
  return (await import('@replit/codemirror-vim')).Vim
}

/** Everything but what a compartment carries; `extra`: the pane's own (gutter, keys, listeners). */
export function baseExtensions(prefs: EditorPrefs, extra: Extension[]): Extension[] {
  return [
    compartments.vim.of(prefs.vim && vimExt ? vimExt : []),
    lineNumbers(),
    highlightActiveLineGutter(),
    foldGutter({ openText: '▾', closedText: '▸' }),
    highlightSpecialChars(),
    history(),
    drawSelection(),
    dropCursor(),
    EditorState.allowMultipleSelections.of(true),
    indentOnInput(),
    syntaxHighlighting(highlight, { fallback: true }),
    bracketMatching(),
    closeBrackets(),
    autocompletion({ activateOnTyping: false }),
    rectangularSelection(),
    crosshairCursor(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    search({ top: true }),
    keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...searchKeymap, ...historyKeymap, ...foldKeymap, indentWithTab]),
    theme,
    EditorState.phrases.of(PHRASES[lang()] ?? {}),
    compartments.language.of([]),
    compartments.wrap.of(prefs.wrap ? EditorView.lineWrapping : []),
    compartments.size.of(sizeTheme(prefs.fontSize)),
    compartments.readOnly.of(EditorState.readOnly.of(prefs.readOnly)),
    compartments.merge.of([]),
    ...extra,
  ]
}

/** The effects that bring a state (made earlier, under other settings) to `prefs`. */
export function prefsEffects(prefs: EditorPrefs) {
  return [
    compartments.vim.reconfigure(prefs.vim && vimExt ? vimExt : []),
    compartments.wrap.reconfigure(prefs.wrap ? EditorView.lineWrapping : []),
    compartments.size.reconfigure(sizeTheme(prefs.fontSize)),
    compartments.readOnly.reconfigure(EditorState.readOnly.of(prefs.readOnly)),
  ]
}
