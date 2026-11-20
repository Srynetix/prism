#!/usr/bin/env node
/**
 * Overlaps (`pnpm --dir web overlaps`): the browser preview (`dev-snapshot.json`) opened in a
 * headless Chromium at 390, 720, 960 and 1 440 px wide, each page of the window shown in turn (the
 * phone's tabs at 390), and the interactive elements that cover one another or leave the window,
 * listed. Exit code 1 when any; run it before a pull request that touches the interface.
 *
 *   node scripts/overlaps.mjs [--widths 720,960] [--only agents] [--theme sand] [--url http://localhost:5173] [--shots dir] [--eval js]
 *
 * Chromium: `PRISM_CHROME`, else Playwright's cache (`npx playwright install chromium-headless-shell`),
 * else Google Chrome or Chromium installed. No dependency: the DevTools protocol over Node's
 * WebSocket, Vite started here unless `--url` names a running one.
 */
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const args = process.argv.slice(2)
const opt = (name) => {
  const i = args.indexOf(`--${name}`)
  return i >= 0 ? args[i + 1] : undefined
}
const WIDTHS = (opt('widths') ?? '390,720,960,1440').split(',').map(Number)
const ONLY = opt('only')?.toLowerCase()
const SHOTS = opt('shots')
const HEIGHT = 800
const PHONE = 390
const WEB = fileURLToPath(new URL('..', import.meta.url))
const wait = (ms) => new Promise((r) => setTimeout(r, ms))

function chromium() {
  if (process.env.PRISM_CHROME) return process.env.PRISM_CHROME
  const caches = [join(homedir(), 'Library/Caches/ms-playwright'), join(homedir(), '.cache/ms-playwright'), join(homedir(), 'AppData/Local/ms-playwright')]
  for (const cache of caches) {
    if (!existsSync(cache)) continue
    for (const dir of readdirSync(cache).filter((d) => d.startsWith('chromium_headless_shell-')).sort().reverse()) {
      for (const sub of readdirSync(join(cache, dir))) {
        for (const exe of ['chrome-headless-shell', 'chrome-headless-shell.exe']) if (existsSync(join(cache, dir, sub, exe))) return join(cache, dir, sub, exe)
      }
    }
  }
  for (const p of ['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', '/Applications/Chromium.app/Contents/MacOS/Chromium', '/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome']) if (existsSync(p)) return p
  throw new Error('No Chromium: set PRISM_CHROME, or `npx playwright install chromium-headless-shell`')
}

/** The preview's address: `--url`, or a Vite started here (stopped at the end). */
async function preview() {
  const url = opt('url')
  if (url) return { url: url.replace(/\/$/, ''), close: async () => {} }
  const { createServer } = await import(join(WEB, 'node_modules/vite/dist/node/index.js'))
  const server = await createServer({ root: WEB, logLevel: 'silent', server: { port: 0, strictPort: false, host: '127.0.0.1' } })
  await server.listen()
  const { port } = server.httpServer.address()
  return { url: `http://127.0.0.1:${port}`, close: () => server.close() }
}

/** One page over the DevTools protocol: `send`, and the events awaited. */
async function browser() {
  const profile = mkdtempSync(join(tmpdir(), 'prism-overlaps-'))
  const proc = spawn(chromium(), ['--headless', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--hide-scrollbars', '--force-device-scale-factor=1', 'about:blank'], { stdio: 'ignore' })
  let port
  for (let i = 0; i < 100 && !port; i++) {
    await wait(100)
    try {
      port = readFileSync(join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]
    } catch {}
  }
  if (!port) throw new Error('Chromium did not start')
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
  const ws = new WebSocket(targets.find((t) => t.type === 'page').webSocketDebuggerUrl)
  await new Promise((r, j) => ((ws.onopen = r), (ws.onerror = j)))
  let id = 0
  const pending = new Map()
  const listeners = new Set()
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data)
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id)
      pending.delete(msg.id)
      msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result)
    } else for (const l of listeners) l(msg)
  }
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const n = ++id
    pending.set(n, { resolve, reject })
    ws.send(JSON.stringify({ id: n, method, params }))
  })
  const once = (method) => new Promise((r) => {
    const l = (msg) => {
      if (msg.method === method) {
        listeners.delete(l)
        r(msg.params)
      }
    }
    listeners.add(l)
  })
  const evaluate = async (expression) => {
    const { result, exceptionDetails } = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text)
    return result.value
  }
  await send('Page.enable')
  // No motion: a page sliding in over the last one is not an overlap.
  await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-reduced-motion', value: 'reduce' }] })
  await send('Runtime.enable')
  const close = () => {
    ws.close()
    proc.kill()
    rmSync(profile, { recursive: true, force: true })
  }
  return { send, once, evaluate, close }
}

/**
 * In the page: the interactive elements seen (their part not clipped by a scrolling or hidden
 * ancestor), those that leave the window, and the pairs that cover one another. An element under
 * a layer above it (a drawer, a menu) is not seen; one faded out or that lets clicks through
 * (`pointer-events: none`) neither.
 */
const MEASURE = String.raw`(() => {
  const W = innerWidth, H = innerHeight
  const SEL = 'button, a[href], input:not([type=hidden]), select, textarea, summary, [role=button], [role=tab], [role=menuitem], [role=switch], [role=checkbox]'
  const name = (el) => {
    const label = (el.getAttribute('aria-label') || el.getAttribute('title') || el.textContent || el.getAttribute('placeholder') || el.value || '').replace(/\s+/g, ' ').trim().slice(0, 40)
    const cls = typeof el.className === 'string' && el.className ? '.' + el.className.trim().split(/\s+/).filter((c) => !c.startsWith('svelte-')).slice(0, 2).join('.') : ''
    return el.tagName.toLowerCase() + cls + (label ? ' "' + label + '"' : '')
  }
  const where = (el) => {
    const parts = []
    for (let p = el.parentElement; p && parts.length < 3 && p !== document.body; p = p.parentElement) {
      const c = typeof p.className === 'string' ? p.className.trim().split(/\s+/).filter((c) => !c.startsWith('svelte-'))[0] : ''
      if (c) parts.unshift(c)
    }
    const block = el.closest('[data-block]')?.getAttribute('data-block')
    return (block ? block + ': ' : '') + parts.join(' › ')
  }
  const seen = []
  for (const el of document.querySelectorAll(SEL)) {
    if (el.parentElement?.closest(SEL)) continue
    let r = el.getBoundingClientRect()
    if (r.width < 2 || r.height < 2) continue
    const cs = getComputedStyle(el)
    if (cs.visibility === 'hidden' || cs.pointerEvents === 'none') continue
    let box = { l: r.left, t: r.top, r: r.right, b: r.bottom }
    let shown = true
    for (let p = el.parentElement; p && p !== document.documentElement; p = p.parentElement) {
      const ps = getComputedStyle(p)
      if (Number(ps.opacity) < 0.05 || ps.pointerEvents === 'none' && p !== el) { shown = false; break }
      if (ps.overflowX !== 'visible' || ps.overflowY !== 'visible') {
        const pr = p.getBoundingClientRect()
        if (ps.overflowX !== 'visible') { box.l = Math.max(box.l, pr.left); box.r = Math.min(box.r, pr.right) }
        if (ps.overflowY !== 'visible') { box.t = Math.max(box.t, pr.top); box.b = Math.min(box.b, pr.bottom) }
      }
      if (box.r - box.l < 2 || box.b - box.t < 2) { shown = false; break }
    }
    if (!shown || Number(cs.opacity) < 0.05) continue
    // Under a layer (a drawer, a menu): what is at its centre is something else, not inside it.
    const cx = (Math.max(box.l, 0) + Math.min(box.r, W)) / 2, cy = (Math.max(box.t, 0) + Math.min(box.b, H)) / 2
    if (cx < 0 || cx > W || cy < 0 || cy > H) continue
    const top = document.elementFromPoint(cx, cy)
    if (top && !el.contains(top) && !top.contains(el)) {
      // Covered at its centre: by another interactive one (an overlap, kept), or by a layer (skipped).
      const other = top.closest(SEL)
      if (!other) continue
    }
    let layer = null
    for (let p = el; p && !layer; p = p.parentElement) {
      const ps = getComputedStyle(p)
      if (['fixed', 'sticky'].includes(ps.position) || ps.position === 'absolute' && ps.zIndex !== 'auto') layer = p
    }
    seen.push({ el, box, name: name(el), where: where(el), layer })
  }
  const out = []
  for (const s of seen) {
    const over = Math.max(0, s.box.r - W) , under = Math.max(0, -s.box.l)
    // Out on a side (below, the page scrolls).
    if (over > 1 || under > 1) out.push({ kind: 'out', a: s.name, at: s.where, by: Math.round(Math.max(over, under)) })
  }
  for (let i = 0; i < seen.length; i++) for (let j = i + 1; j < seen.length; j++) {
    const a = seen[i], b = seen[j]
    if (a.el.contains(b.el) || b.el.contains(a.el)) continue
    // One in a layer the other is not in (a menu, a drawer, a sticky bar the page scrolls under):
    // layers cover, as meant.
    if (a.layer !== b.layer) continue
    const w = Math.min(a.box.r, b.box.r) - Math.max(a.box.l, b.box.l), h = Math.min(a.box.b, b.box.b) - Math.max(a.box.t, b.box.t)
    if (w > 2 && h > 2) out.push({ kind: 'overlap', a: a.name, b: b.name, at: a.where, by: Math.round(w) + '×' + Math.round(h), xy: Math.round(Math.max(a.box.l, b.box.l)) + ',' + Math.round(Math.max(a.box.t, b.box.t)) })
  }
  return out
})()`

/** The window's pages, in the rail's order: its pages, Agents, Projects, Help, Settings. */
const PAGES = String.raw`[...document.querySelectorAll('nav.rail button.v:not([aria-expanded])')].map((b, i) => ({ i, label: (b.querySelector('span')?.textContent || b.getAttribute('title') || '').trim() })).filter((p) => p.label)`
const PHONE_TABS = String.raw`[...document.querySelectorAll('.app > nav button, nav button')].map((b, i) => ({ i, label: (b.querySelector('span')?.textContent || '').trim() })).filter((p) => p.label)`

/** `--shots <dir>`: each page as seen, `<width>-<page>.png`. */
async function shot(b, width, page) {
  if (!SHOTS) return
  mkdirSync(SHOTS, { recursive: true })
  const { data } = await b.send('Page.captureScreenshot', { format: 'png' })
  writeFileSync(join(SHOTS, `${width}-${page.replace(/[^\p{L}\p{N}]+/gu, '_')}.png`), Buffer.from(data, 'base64'))
}

async function main() {
  const server = await preview()
  const b = await browser()
  const problems = []
  const report = (width, page, list) => {
    for (const p of list) problems.push({ width, page, ...p })
  }
  try {
    for (const width of WIDTHS) {
      await b.send('Emulation.setDeviceMetricsOverride', { width, height: width === PHONE ? 2400 : HEIGHT, deviceScaleFactor: 1, mobile: width === PHONE })
      const phone = width === PHONE
      const loaded = b.once('Page.loadEventFired')
      await b.send('Page.navigate', { url: `${server.url}/${phone ? 'remote.html?preview' : '?core'}` })
      await loaded
      // The app is up once its rail (its tabs) shows, after the boot.
      let pages = []
      for (let i = 0; i < 60 && !pages.length; i++) {
        await wait(500)
        pages = await b.evaluate(phone ? PHONE_TABS : PAGES)
      }
      // `--theme sand`: that theme, kept over the one the app sets (some change the sizes).
      if (opt('theme')) await b.evaluate(`(() => { const th = ${JSON.stringify(opt('theme'))}, root = document.documentElement, set = () => root.dataset.theme !== th && (root.dataset.theme = th); set(); new MutationObserver(set).observe(root, { attributes: true, attributeFilter: ['data-theme'] }); return true })()`)
      // The boot screen, skipped as a key does, then gone.
      await b.evaluate(`dispatchEvent(new KeyboardEvent('keydown', { key: 'Shift' })), true`)
      await wait(1500)
      if (!pages.length) await shot(b, width, 'start')
      if (!pages.length) throw new Error(`No page found at ${width} px: is the preview up?`)
      for (const p of pages) {
        if (ONLY && !p.label.toLowerCase().includes(ONLY)) continue
        await b.evaluate(`(${phone ? PHONE_TABS : PAGES}, document.querySelectorAll(${JSON.stringify(phone ? 'nav button' : 'nav.rail button.v:not([aria-expanded])')})[${p.i}].click(), true)`)
        await wait(900)
        if (opt('eval')) console.log(width, p.label, JSON.stringify(await b.evaluate(opt('eval'))))
        report(width, p.label, await b.evaluate(MEASURE))
        await shot(b, width, p.label)
      }
    }
  } finally {
    b.close()
    await server.close()
  }
  const seen = new Set()
  for (const p of problems) {
    const line = p.kind === 'out' ? `${p.width}px ${p.page}: ${p.a} leaves the window by ${p.by}px (${p.at})` : `${p.width}px ${p.page}: ${p.a} covers ${p.b} by ${p.by} at ${p.xy} (${p.at})`
    if (seen.has(line)) continue
    seen.add(line)
    console.log(line)
  }
  console.log(seen.size ? `\n${seen.size} problem(s).` : 'No overlap.')
  process.exit(seen.size ? 1 : 0)
}

main().catch((e) => {
  console.error(e.message ?? e)
  process.exit(2)
})
