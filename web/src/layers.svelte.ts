import { untrack } from 'svelte'

/**
 * The window's layers: side drawers (an MR, a ticket, logs…), dialogs (the palette, a new ticket…)
 * and the menus that ask for it, in the order they were opened. The stack alone decides:
 * - Escape closes the layer on top, whatever the order the components were mounted in; in a
 *   field of a drawer, the field has it first (a comment box cancels), else it leaves the field
 *   (a second Escape closes);
 * - a click outside a drawer closes it, and the drawers above it, unless it is pinned;
 * - focus goes into a layer when it opens and back where it came from when it closes;
 * - a drawer's rank (`z-index`) is its place in the stack, so the last opened is on top;
 * - a drawer opened from another stacks over it, the one below steps back, and the crumbs of the
 *   one on top lead back down (`closeAbove`). Opening again what is already in the stack goes back
 *   to it (`reveal`) rather than opening it twice.
 * Drawer.svelte, Modal.svelte and MenuShell register through `pushLayer`.
 */

export type LayerKind = 'drawer' | 'modal' | 'menu'

export interface LayerSpec {
  kind: LayerKind
  /** What it shows, for `reveal` and the crumbs: 'mr', 'ticket', 'logs'… */
  key: string
  /** Its short name in the crumbs of the drawers above it: `shop!412`, `PAY-142`. */
  label?: () => string
  el: () => HTMLElement | undefined
  close: () => void
  /** A pinned drawer stays when one clicks elsewhere. */
  pinned?: () => boolean
  /** Escape closes it (true): a terminal keeps its Escape. */
  escape?: boolean
  /** A click outside closes it: drawers yes, dialogs (their scrim does) and menus (their anchor) no. */
  outside?: boolean
}

export interface Layer extends LayerSpec {
  id: number
  /** Where the focus was when it opened. */
  from: Element | null
}

let next = 1
export const layers = $state({ stack: [] as Layer[] })

const BASE: Record<LayerKind, string> = { drawer: '--z-drawer', modal: '--z-modal', menu: '--z-menu' }

/** A layer's `z-index`: its kind's floor, plus its place in the stack. */
export function zIndex(id: number): string {
  const i = layers.stack.findIndex((l) => l.id === id)
  const l = layers.stack[i]
  return l ? `calc(var(${BASE[l.kind]}) + ${i})` : `var(--z-drawer)`
}

/** The drawers under this one, bottom first: its crumbs. */
export function below(id: number): Layer[] {
  const i = layers.stack.findIndex((l) => l.id === id)
  return i < 0 ? [] : layers.stack.slice(0, i).filter((l) => l.kind === 'drawer' && l.label)
}

/** Is it the top drawer (the others step back)? */
export function isTop(id: number): boolean {
  const drawers = layers.stack.filter((l) => l.kind === 'drawer')
  return drawers.at(-1)?.id === id
}

/** Opens a layer on top of the others; the function returned removes it (on close). */
export function pushLayer(spec: LayerSpec): { id: number; remove: () => void } {
  const layer: Layer = { escape: true, outside: spec.kind === 'drawer', ...spec, id: next++, from: document.activeElement }
  // Called from components' effects: the stack written, never read into their dependencies.
  untrack(() => layers.stack.push(layer))
  queueMicrotask(() => focusInto(layer))
  return { id: layer.id, remove: () => remove(layer.id) }
}

function remove(id: number) {
  untrack(() => removeNow(id))
}

function removeNow(id: number) {
  const i = layers.stack.findIndex((l) => l.id === id)
  if (i < 0) return
  const [layer] = layers.stack.splice(i, 1)
  const el = layer.el()
  const active = document.activeElement
  const lost = !active || active === document.body || !active.isConnected || (el?.contains(active) ?? false)
  if (lost && layer.from instanceof HTMLElement && layer.from.isConnected) layer.from.focus({ preventScroll: true })
}

/** Puts an open layer on top (its content changed: another MR in the MR drawer). */
export function front(id: number) {
  untrack(() => frontNow(id))
}

function frontNow(id: number) {
  const i = layers.stack.findIndex((l) => l.id === id)
  if (i < 0 || i === layers.stack.length - 1) return
  const [layer] = layers.stack.splice(i, 1)
  layers.stack.push(layer)
}

/** Closes every layer above this one, the top first. */
export function closeAbove(id: number) {
  const i = layers.stack.findIndex((l) => l.id === id)
  if (i < 0) return
  for (const l of layers.stack.slice(i + 1).reverse()) l.close()
}

/**
 * Opening what is already open: back to it (the layers above it close) and true; false when it is
 * not open, to be opened.
 */
export function reveal(key: string): boolean {
  const l = layers.stack.findLast((x) => x.key === key)
  if (!l) return false
  closeAbove(l.id)
  return true
}

/** Is this element inside an open layer? */
export const inLayer = (target: EventTarget | null) => target instanceof Node && layers.stack.some((l) => l.el()?.contains(target))

/** Is a layer of this key open? */
export const isOpen = (key: string) => layers.stack.some((l) => l.key === key)

function focusInto(layer: Layer) {
  const el = layer.el()
  if (!el || el.contains(document.activeElement)) return
  const target = el.querySelector<HTMLElement>('[autofocus], [data-autofocus]') ?? (layer.kind === 'drawer' ? el : el.querySelector<HTMLElement>('input, textarea, select, button, [tabindex]:not([tabindex="-1"])') ?? el)
  target.focus({ preventScroll: true })
}

const FIELD = 'input, textarea, select, [contenteditable=""], [contenteditable="true"], .cm-editor'

function onKey(e: KeyboardEvent) {
  if (e.key !== 'Escape' || e.defaultPrevented || e.isComposing) return
  const top = layers.stack.at(-1)
  if (!top) return
  const target = e.target as Element | null
  // A list of choices of a field (a select's, a suggestion's) or a menu not in the stack: its own.
  const own = target?.closest?.('[role=listbox], [role=menu], .suggest')
  if (own && !layers.stack.some((l) => l.el()?.contains(own))) return
  if (top.escape === false && top.el()?.contains(target)) return
  // In a field, or in what handles its own Escape (`data-esc-own`: a comment box cancels, a filter
  // clears, a list of suggestions closes), the field has it first; once the event went through,
  // unused (no `preventDefault`, however far it went), `afterField` acts.
  if (target instanceof HTMLElement && target.closest(`${FIELD}, [data-esc-own]`) && top.el()?.contains(target)) {
    setTimeout(() => afterField(e, top, target))
    return
  }
  e.preventDefault()
  e.stopImmediatePropagation()
  top.close()
}

/**
 * Escape from a field, gone through unused: out of the field in a drawer (what is typed stays, a
 * second Escape closes); in a dialog or a menu (the palette's field), it closes.
 */
function afterField(e: KeyboardEvent, top: Layer, target: HTMLElement) {
  if (e.defaultPrevented || !layers.stack.includes(top)) return
  if (top.kind === 'drawer' && target.closest(FIELD)) {
    target.blur()
    top.el()?.focus({ preventScroll: true })
  } else if (top.escape !== false) top.close()
}

/** Never closes on: a menu, a list of choices, an enlarged image, a toast, a tooltip. */
const OWN = '[role=menu], [role=listbox], .suggest, .lightbox, .toasts, .popover, [role=dialog]:not([data-layer])'

function onPointer(e: PointerEvent) {
  const target = e.target as Element | null
  if (!target?.isConnected || target.closest(OWN)) return
  const stack = layers.stack
  // The deepest layer the click is in: the ones above it close, if they close on a click outside.
  let inside = -1
  for (let i = stack.length - 1; i >= 0; i--) {
    if (stack[i].el()?.contains(target)) {
      inside = i
      break
    }
  }
  for (const l of stack.slice(inside + 1).reverse()) {
    if (l.kind === 'modal') return
    if (l.outside && !l.pinned?.()) l.close()
  }
}

if (typeof window !== 'undefined') {
  window.addEventListener('keydown', onKey, true)
  window.addEventListener('pointerdown', onPointer, true)
}
