import { openThread, parseThreadAttr } from './thread.svelte'
import { currentView, isCollapsed, isShort, setMode, setView, toggleZoom, zoom } from './layout.svelte'
import { views } from './views'
import { isMac } from './platform'

/**
 * Keyboard control, vim-style. A cursor walks the lines of the active block (tickets, MRs,
 * alerts, sessions, events…), marked by `data-cursor`; the active block by `data-kbd`.
 * Keys only apply outside an input field and without ⌘ / ⌥. Line commands
 * just click what the mouse would click: no action has its own path.
 * Focus can move into the pane open on the right (`Ctrl-w l`, Tab): j / k then
 * scroll it, J / K move from one action to the next; `Ctrl-w h`, Tab or Esc go back to the grid.
 */
export const keys = $state({ help: false, pending: '' as '' | 'g' | 'z' | 'w', pane: 'grid' as 'grid' | 'drawer' })

/** Navigable lines of a block, whatever it is. */
const ITEMS = '.tk, .mr, .row, .card, .s, .ev, .srow, .grp, .group, [data-nav]'
const PENDING_MS = 900

let block: string | null = null
let item: HTMLElement | null = null
/** Set by a click while the marks were hidden: the next move first shows the cursor where it is. */
let unseen = false
let timer: ReturnType<typeof setTimeout> | undefined

const blocks = () => [...document.querySelectorAll<HTMLElement>('main [data-block]')].filter((b) => b.offsetParent !== null)
const blockEl = () => (block ? document.querySelector<HTMLElement>(`main [data-block="${block}"]`) : null)

/** Visible lines of the block, without those nested in another line. */
function itemsOf(el: HTMLElement | null): HTMLElement[] {
  if (!el) return []
  const all = [...el.querySelectorAll<HTMLElement>(ITEMS)].filter((x) => x.offsetParent !== null && !x.closest('[role=menu]'))
  return all.filter((x) => !all.some((p) => p !== x && p.contains(x)))
}

function mark() {
  unseen = false
  document.querySelectorAll('[data-cursor]').forEach((x) => x.removeAttribute('data-cursor'))
  document.querySelectorAll('[data-kbd]').forEach((x) => x.removeAttribute('data-kbd'))
  if (keys.pane === 'drawer') {
    const d = drawer()
    d?.setAttribute('data-kbd', '')
    if (action?.isConnected && d?.contains(action)) {
      action.setAttribute('data-cursor', '')
      action.focus({ preventScroll: true })
      action.scrollIntoView({ block: 'nearest' })
    }
    return
  }
  const b = blockEl()
  b?.setAttribute('data-kbd', '')
  if (item?.isConnected) {
    item.setAttribute('data-cursor', '')
    item.scrollIntoView({ block: 'nearest' })
  }
}

function focusBlock(id: string | null, at: 'first' | 'last' | 'keep' = 'first') {
  block = id
  const list = itemsOf(blockEl())
  item = at === 'keep' && item && list.includes(item) ? item : (at === 'last' ? list.at(-1) : list[0]) ?? null
  mark()
}

/** The active block, or the view's first; and the current line, found again if the list moved. */
function ensure(): HTMLElement[] {
  const all = blocks()
  if (!block || !all.some((b) => b.dataset.block === block)) block = all[0]?.dataset.block ?? null
  const list = itemsOf(blockEl())
  if (!item || !list.includes(item)) item = list[0] ?? null
  return list
}

function move(delta: number) {
  // First press: the cursor appears on the first line, without advancing.
  const fresh = !item?.isConnected || unseen
  const list = ensure()
  if (fresh && item) return mark()
  if (!list.length) return moveBlock(delta > 0 ? 1 : -1)
  const i = item ? list.indexOf(item) : -1
  const next = i + delta
  // At the end of the block, j / k move to the next block in the column.
  if (next < 0 || next >= list.length) {
    if (Math.abs(delta) === 1) return moveBlock(delta, true)
    item = list[Math.max(0, Math.min(list.length - 1, next))]
  } else item = list[next]
  mark()
}

/** Next or previous block; `sameColumn`: only in the active block's column. */
function moveBlock(delta: number, sameColumn = false) {
  ensure()
  const all = blocks()
  const cur = blockEl()
  const pool = sameColumn && cur ? all.filter((b) => b.parentElement === cur.parentElement) : all
  const i = cur ? pool.indexOf(cur) : -1
  const next = pool[i + delta]
  if (next) focusBlock(next.dataset.block!, delta > 0 ? 'first' : 'last')
  else mark()
}

/** Block of the neighboring column, at the height of the current line. */
function moveColumn(dir: -1 | 1) {
  ensure()
  const cur = blockEl()
  if (!cur) return
  const r = cur.getBoundingClientRect()
  const y = item?.getBoundingClientRect().top ?? r.top
  const candidates = blocks().filter((b) => {
    const br = b.getBoundingClientRect()
    return dir > 0 ? br.left >= r.right - 1 : br.right <= r.left + 1
  })
  if (!candidates.length) return
  const dist = (b: HTMLElement) => {
    const br = b.getBoundingClientRect()
    return Math.abs(br.left - r.left) * 10 + (y < br.top ? br.top - y : y > br.bottom ? y - br.bottom : 0)
  }
  const target = candidates.sort((a, b) => dist(a) - dist(b))[0]
  focusBlock(target.dataset.block!)
}

/** Click on the line: its pane or its main link, as with the mouse. */
function open(external = false) {
  ensure()
  if (!item) return
  if (external) {
    const a = item.querySelector<HTMLAnchorElement>('a[href^="http"]')
    // ⌘-click (Ctrl-click off macOS): the link goes to the browser instead of the preview.
    a?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, metaKey: isMac, ctrlKey: !isMac }))
    return
  }
  if (item.matches('[role=button], .card, button')) item.click()
  else item.querySelector<HTMLElement>('a[href], button')?.click()
}

/** The thread of the line (`data-thread`, on it or inside it): its drawer. */
function thread() {
  ensure()
  const el = item?.closest<HTMLElement>('[data-thread]') ?? item?.querySelector<HTMLElement>('[data-thread]')
  const ref = el?.dataset.thread ? parseThreadAttr(el.dataset.thread) : null
  if (ref) openThread(ref)
}

function within(selector: string) {
  ensure()
  item?.querySelector<HTMLElement>(selector)?.click()
}

/** ⋯ menu of the line, its first choice selected: j / k and Enter navigate it. */
function openMenu() {
  within('button.more')
  setTimeout(() => menuItems()[0]?.focus(), 0)
}

const menuItems = () => [...document.querySelectorAll<HTMLElement>('[role=menu] :is(a, button):not(:disabled)')]

function inMenu(e: KeyboardEvent): boolean {
  const items = menuItems()
  if (!items.length) return false
  const i = items.indexOf(document.activeElement as HTMLElement)
  if (e.key === 'j' || e.key === 'ArrowDown') items[(i + 1) % items.length].focus()
  else if (e.key === 'k' || e.key === 'ArrowUp') items[(i - 1 + items.length) % items.length].focus()
  // Enter and Space activate the selected entry, Esc closes the menu: their native behavior.
  else return e.key === 'Enter' || e.key === 'Escape' || e.key === ' '
  e.preventDefault()
  return true
}

function cycleView(delta: number) {
  const all = views()
  const i = all.findIndex((v) => v.id === currentView().id)
  setView(all[(i + delta + all.length) % all.length].id)
  reset()
}

function blockMode(mode: 'collapsed' | 'short') {
  ensure()
  if (!block) return
  const on = mode === 'collapsed' ? isCollapsed(block) : isShort(block)
  setMode(block, on ? 'normal' : mode)
  setTimeout(() => focusBlock(block, 'keep'), 0)
}

/* ---------- Right pane ---------- */

let action: HTMLElement | null = null

/** The pane open in the foreground (highest `data-drawer` rank), if there is one. */
function drawer(): HTMLElement | null {
  // A pane has fixed position: `offsetParent` is null even when shown, so its boxes are tested.
  const open = [...document.querySelectorAll<HTMLElement>('aside[data-drawer]')].filter((d) => d.getClientRects().length > 0)
  return open.sort((a, b) => Number(b.dataset.drawer) - Number(a.dataset.drawer))[0] ?? null
}

/** Buttons, links and selects of the pane, in reading order. */
function actions(d: HTMLElement): HTMLElement[] {
  return [...d.querySelectorAll<HTMLElement>('a[href], button:not(:disabled), select:not(:disabled), [role=button]')].filter(
    (x) => x.offsetParent !== null && !x.closest('[role=menu]'),
  )
}

/** Scrolling area: that of the current action, otherwise the pane's largest. */
function scroller(d: HTMLElement): HTMLElement {
  const scrolls = (x: HTMLElement) => x.scrollHeight > x.clientHeight + 1 && /auto|scroll/.test(getComputedStyle(x).overflowY)
  for (let x = action; x && x !== d; x = x.parentElement) if (scrolls(x)) return x
  const all = [...d.querySelectorAll<HTMLElement>('*')].filter(scrolls)
  return all.sort((a, b) => b.clientHeight * b.clientWidth - a.clientHeight * a.clientWidth)[0] ?? d
}

export function setPane(pane: 'grid' | 'drawer') {
  if (pane === 'drawer' && !drawer()) return
  keys.pane = pane
  if (pane === 'drawer') action = null
  else (document.activeElement as HTMLElement | null)?.blur?.()
  mark()
}

function stepAction(delta: number) {
  const d = drawer()
  if (!d) return
  const list = actions(d)
  if (!list.length) return
  const i = action ? list.indexOf(action) : -1
  action = list[i < 0 ? (delta > 0 ? 0 : list.length - 1) : Math.max(0, Math.min(list.length - 1, i + delta))]
  mark()
}

function drawerKey(e: KeyboardEvent, run: (f: () => void) => boolean): boolean {
  const d = drawer()
  if (!d) {
    setPane('grid')
    return false
  }
  const s = scroller(d)
  switch (e.key) {
    case 'j': case 'ArrowDown': return run(() => s.scrollBy({ top: 60 }))
    case 'k': case 'ArrowUp': return run(() => s.scrollBy({ top: -60 }))
    case 'J': return run(() => stepAction(1))
    case 'K': return run(() => stepAction(-1))
    case 'G': return run(() => s.scrollTo({ top: s.scrollHeight }))
    case 'h': case 'ArrowLeft': return run(() => setPane('grid'))
    // Enter activates the selected action (native behavior of the focused button or link).
    case 'Enter': return document.activeElement === action && !!action
    case 'o': return run(() => action?.click())
    case '?': return run(() => (keys.help = !keys.help))
  }
  return false
}

/**
 * Esc with focus in a pane: back to the grid, the pane stays open. Listened to in
 * capture on the window, before the panes which, for their part, close on Esc.
 */
if (typeof window !== 'undefined') {
  window.addEventListener(
    'keydown',
    (e) => {
      if (e.key !== 'Escape' || keys.pane !== 'drawer') return
      // Esc in a field (comment in progress): up to the field to decide.
      if ((e.target as HTMLElement | null)?.closest?.('textarea, input')) return
      // An open menu or an enlarged image closes first, as usual.
      if (document.querySelector('[role=menu], .lightbox')) return
      e.preventDefault()
      e.stopImmediatePropagation()
      setPane('grid')
    },
    true,
  )
}

export function reset() {
  block = null
  item = null
  mark()
}

/**
 * A click in a block of the grid makes it the current one for the keys, and the clicked line the
 * current line: after a click on the Merge requests block, `zz` zooms it, `j` goes on from the
 * clicked MR. The marks only show if the keys were already in use, so as not to frame every
 * block clicked with the mouse. Menus (attached to `<body>`) and the right pane are left alone.
 */
export function followPointer(e: PointerEvent) {
  const t = e.target as HTMLElement | null
  if (!t?.closest || t.closest('[role=menu], aside[data-drawer]')) return
  const el = t.closest<HTMLElement>('main [data-block]')
  if (!el?.dataset.block) return
  const shown = !!document.querySelector('[data-kbd]')
  keys.pane = 'grid'
  block = el.dataset.block
  item = itemsOf(el).find((x) => x.contains(t)) ?? (item && el.contains(item) ? item : null)
  if (shown) mark()
  else unseen = true
}

/** Handling of a key; true if it was taken. Called by App after its own shortcuts. */
export function handleKey(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null
  if (e.metaKey || e.altKey) return false
  // In the pane, a select (status) can hold the action cursor: j / k do not change it.
  if (t?.closest?.(keys.pane === 'drawer' ? 'input, textarea, [contenteditable]' : 'input, textarea, select, [contenteditable]')) return false
  if (inMenu(e)) return true
  // The pane may have closed (click, Esc): focus goes back to the grid.
  if (keys.pane === 'drawer' && !drawer()) setPane('grid')

  const prefix = keys.pending
  keys.pending = ''
  clearTimeout(timer)
  const run = (f: () => void) => {
    e.preventDefault()
    f()
    return true
  }
  // Ctrl-w then l / h (with or without Ctrl) or w: pane / grid, like vim windows.
  if (prefix === 'w') {
    const k = e.key.toLowerCase()
    if (k === 'l') return run(() => setPane('drawer'))
    if (k === 'h') return run(() => setPane('grid'))
    if (k === 'w') return run(() => setPane(keys.pane === 'grid' ? 'drawer' : 'grid'))
    return false
  }
  if (e.key === 'Tab' && !e.ctrlKey && drawer()) return run(() => setPane(keys.pane === 'grid' ? 'drawer' : 'grid'))
  if (e.ctrlKey) {
    if (e.key === 'w') {
      keys.pending = 'w'
      timer = setTimeout(() => (keys.pending = ''), PENDING_MS)
      e.preventDefault()
      return true
    }
    if (e.key === 'd' || e.key === 'u') {
      const d = keys.pane === 'drawer' ? drawer() : null
      if (d) {
        const sc = scroller(d)
        return run(() => sc.scrollBy({ top: (e.key === 'd' ? 0.5 : -0.5) * sc.clientHeight }))
      }
      return run(() => move(e.key === 'd' ? 10 : -10))
    }
    return false
  }
  if (keys.pane === 'drawer') {
    if (prefix === 'g' && e.key === 'g') return run(() => { const d = drawer(); if (d) scroller(d).scrollTo({ top: 0 }) })
    if (e.key === 'g') {
      keys.pending = 'g'
      timer = setTimeout(() => (keys.pending = ''), PENDING_MS)
      e.preventDefault()
      return true
    }
    return drawerKey(e, run)
  }
  if (prefix === 'g') {
    if (e.key === 'g') return run(() => { ensure(); const l = itemsOf(blockEl()); item = l[0] ?? null; mark() })
    if (e.key === 't') return run(() => cycleView(1))
    if (e.key === 'T') return run(() => cycleView(-1))
    return false
  }
  if (prefix === 'z') {
    if (e.key === 'a' || e.key === 'c') return run(() => blockMode('collapsed'))
    if (e.key === 's') return run(() => blockMode('short'))
    if (e.key === 'z') return run(() => { ensure(); if (block) toggleZoom(block); setTimeout(() => focusBlock(block, 'keep'), 0) })
    return false
  }

  // Shift + ↓ / ↑: next / previous block, like J / K.
  if (e.shiftKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) return run(() => moveBlock(e.key === 'ArrowDown' ? 1 : -1))

  switch (e.key) {
    case 'g':
    case 'z':
      keys.pending = e.key
      timer = setTimeout(() => (keys.pending = ''), PENDING_MS)
      e.preventDefault()
      return true
    case 'j': case 'ArrowDown': return run(() => move(1))
    case 'k': case 'ArrowUp': return run(() => move(-1))
    case 'h': case 'ArrowLeft': return run(() => moveColumn(-1))
    case 'l': case 'ArrowRight': return run(() => moveColumn(1))
    case 'J': return run(() => moveBlock(1))
    case 'K': return run(() => moveBlock(-1))
    case 'G': return run(() => { ensure(); item = itemsOf(blockEl()).at(-1) ?? null; mark() })
    case 'Enter': case 'o': return run(() => open())
    case 'O': return run(() => open(true))
    case '.': return run(openMenu)
    // The row's action: its button (MRs, tickets…) or, in Alerts, its first quick fix still to run.
    case 'a': return run(() => within(':is(.act, button.fix, a.fix):not(:disabled)'))
    case 'x': return run(() => within('button.mark'))
    case 't': return run(thread)
    // The ↻ button of the active block (ClickUp), otherwise that of the first block that has one.
    case 'r': return run(() => { ensure(); (blockEl()?.querySelector<HTMLElement>('button.refresh') ?? document.querySelector<HTMLElement>('main button.refresh'))?.click() })
    case '?': return run(() => (keys.help = !keys.help))
    case 'Escape':
      if (keys.help) return run(() => (keys.help = false))
      if (zoom.id) return false
      if (block || item) return run(reset)
      return false
  }
  if (/^[1-9]$/.test(e.key) && views()[Number(e.key) - 1]) return run(() => { setView(views()[Number(e.key) - 1].id); reset() })
  return false
}
