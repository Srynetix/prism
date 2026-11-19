// Prism's annotation script, added to every HTML page Prism serves (`prismfile:`, a phone's
// `/page/`): inert until the window that frames the page turns its Comment mode on. It then outlines
// the element under the pointer, picks one on a click (the page itself not pressed), draws the
// remarks' pins and finds their elements again once the page changed. It talks to its parent only
// by `postMessage` ({ prism: 'annotate', … }); the window trusts nothing it says beyond these fields.
;(() => {
  if (window.__prismAnnotate || window.parent === window) return
  window.__prismAnnotate = true
  const P = '__prism-annotate'
  let on = false
  let hover = null
  let chosen = null
  let pins = []
  let drawing = 0
  const say = (m) => window.parent.postMessage(Object.assign({ prism: 'annotate' }, m), '*')
  const style = document.createElement('style')
  style.setAttribute('data-prism', '')
  style.textContent =
    `.${P}-hover{outline:2px solid #a78bfa!important;outline-offset:2px!important;cursor:crosshair!important}` +
    `.${P}-chosen{outline:2px solid #7c3aed!important;outline-offset:2px!important;box-shadow:0 0 0 6px rgba(167,139,250,.28)!important}` +
    `.${P}-pin{position:absolute;z-index:2147483647;width:20px;height:20px;border-radius:50%;background:#a78bfa;color:#17141f;` +
    `font:700 11px/20px -apple-system,BlinkMacSystemFont,sans-serif;text-align:center;box-shadow:0 1px 5px rgba(0,0,0,.45);` +
    `pointer-events:none;transform:translate(-40%,-40%)}.${P}-pin.${P}-hl{background:#7c3aed;color:#fff}`
  ;(document.head || document.documentElement).appendChild(style)

  const mine = (el) => el && el.classList && el.classList.contains(`${P}-pin`)
  const pickable = (el) => el && el.nodeType === 1 && el !== document.body && el !== document.documentElement && !mine(el)
  const clean = (s) => String(s || '').replace(/\s+/g, ' ').trim()
  /** What the element says: its text, or what stands for it (an image's alt, a field's placeholder). */
  const textOf = (el) => clean(el.innerText || el.getAttribute('alt') || el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.value || el.getAttribute('title')).slice(0, 80)
  const ownClass = (el) => [...el.classList].find((c) => !c.startsWith(P) && /^[A-Za-z_][\w-]*$/.test(c))
  /** A selector for `el`: from the nearest id up, each step its tag, a class and its rank among its kind. */
  function selector(el) {
    const steps = []
    while (el && el.nodeType === 1 && el !== document.body && el !== document.documentElement && steps.length < 6) {
      if (el.id && /^[A-Za-z][\w-]*$/.test(el.id) && document.querySelectorAll('#' + el.id).length === 1) {
        steps.unshift('#' + el.id)
        break
      }
      let step = el.localName
      const c = ownClass(el)
      if (c) step += '.' + c
      const kin = el.parentElement ? [...el.parentElement.children].filter((x) => x.localName === el.localName) : []
      if (kin.length > 1) step += `:nth-of-type(${kin.indexOf(el) + 1})`
      steps.unshift(step)
      el = el.parentElement
    }
    return steps.join(' > ')
  }
  /** A remark's element in the page as it is now: same tag and text first (the page rewritten), then its selector. */
  function locate(p) {
    let bySel = null
    try {
      bySel = document.querySelector(p.sel)
    } catch {}
    if (bySel && bySel.localName === p.tag && (!p.text || textOf(bySel) === p.text)) return bySel
    if (p.text) {
      const same = [...document.getElementsByTagName(p.tag)].filter((el) => textOf(el) === p.text)
      if (same.length) return same[0]
    }
    return bySel && bySel.localName === p.tag ? bySel : null
  }
  function draw() {
    drawing = 0
    document.querySelectorAll(`.${P}-pin`).forEach((n) => n.remove())
    const found = []
    for (const p of pins) {
      const el = locate(p)
      found.push({ n: p.n, missing: !el })
      if (!el) continue
      const r = el.getBoundingClientRect()
      const pin = document.createElement('div')
      pin.className = `${P}-pin` + (p.hl ? ` ${P}-hl` : '')
      pin.textContent = String(p.n)
      pin.style.left = r.left + window.scrollX + 'px'
      pin.style.top = r.top + window.scrollY + 'px'
      document.body.appendChild(pin)
    }
    say({ type: 'found', pins: found })
  }
  const redraw = () => {
    if (pins.length && !drawing) drawing = requestAnimationFrame(draw)
  }
  window.addEventListener('resize', redraw)
  if (window.ResizeObserver) new ResizeObserver(redraw).observe(document.documentElement)
  // The page's own changes (a list filled by its script, a tab opened), not the pins'.
  // Prism's own changes: its pins, its style, and its classes on the page's elements (read once the
  // pointer has moved on: compared without them, the class did not change).
  const bare = (c) => String(c || '').split(/\s+/).filter((x) => x && !x.startsWith(P)).join(' ')
  const ours = (m) =>
    m.type === 'childList'
      ? [...m.addedNodes, ...m.removedNodes].every(mine)
      : mine(m.target) || m.target === style || (m.attributeName === 'class' && bare(m.oldValue) === bare(m.target.getAttribute('class')))
  new MutationObserver((list) => {
    if (list.some((m) => !ours(m))) redraw()
  }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeOldValue: true, characterData: true })

  const unhover = () => {
    if (hover) hover.classList.remove(`${P}-hover`)
    hover = null
  }
  document.addEventListener('mouseover', (e) => {
    if (!on) return
    e.stopPropagation()
    if (!pickable(e.target)) return
    unhover()
    hover = e.target
    hover.classList.add(`${P}-hover`)
  }, true)
  document.addEventListener('mouseout', (e) => {
    if (on) e.stopPropagation()
    unhover()
  }, true)
  // While commenting, the page is not pressed: a click picks, the rest stops here.
  const hold = (e) => {
    if (!on) return
    e.stopPropagation()
    // A touch's default is kept: it scrolls, and a tap still makes the click that picks.
    if (e.cancelable && !e.type.startsWith('touch')) e.preventDefault()
  }
  for (const type of ['mousedown', 'mouseup', 'pointerdown', 'pointerup', 'touchstart', 'touchend', 'dblclick', 'submit', 'input', 'change', 'focusin']) {
    document.addEventListener(type, hold, true)
  }
  document.addEventListener('keydown', (e) => {
    if (!on) return
    e.stopPropagation()
    e.preventDefault()
    if (e.key === 'Escape') say({ type: 'escape' })
  }, true)
  document.addEventListener('click', (e) => {
    if (!on) return
    e.stopPropagation()
    e.preventDefault()
    let el = e.target
    if (!pickable(el)) return
    // ⌥-click: the element around it (a whole card rather than its title).
    if (e.altKey && pickable(el.parentElement)) el = el.parentElement
    if (chosen) chosen.classList.remove(`${P}-chosen`)
    chosen = el
    el.classList.add(`${P}-chosen`)
    const r = el.getBoundingClientRect()
    say({ type: 'pick', sel: selector(el), tag: el.localName, text: textOf(el), rect: { x: r.left, y: r.top, w: r.width, h: r.height } })
  }, true)

  window.addEventListener('message', (e) => {
    if (e.source !== window.parent) return
    const m = e.data
    if (!m || m.prism !== 'annotate') return
    if (m.type === 'mode') {
      on = !!m.on
      if (!on) unhover()
    }
    if ((m.type === 'clear' || (m.type === 'mode' && !on)) && chosen) {
      chosen.classList.remove(`${P}-chosen`)
      chosen = null
    }
    if (m.type === 'pins' && Array.isArray(m.pins)) {
      pins = m.pins.filter((p) => p && typeof p.sel === 'string' && typeof p.tag === 'string').map((p) => ({ n: +p.n, sel: p.sel, tag: p.tag, text: String(p.text || ''), hl: !!p.hl }))
      draw()
    }
  })
  say({ type: 'ready' })
})()
