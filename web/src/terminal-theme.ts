import type { ITheme } from '@xterm/xterm'

/**
 * Colors of the agents' terminal, taken from the app theme (app.css): background, text, selection and
 * the 16 ANSI colors (red = ko, green = ok, yellow = warn, blue = info…). Re-read on each
 * theme change.
 */
export function terminalTheme(): ITheme {
  const css = getComputedStyle(document.documentElement)
  const v = (name: string) => css.getPropertyValue(name).trim()
  const light = css.colorScheme.includes('light') && !css.colorScheme.includes('dark')
  const bg = v('--surface')
  const fg = v('--text')
  // The "bright" ones: the color brought closer to the text, more legible on the background.
  const bright = (c: string) => mix(c, fg, 0.72)
  const red = v('--ko'), green = v('--ok'), yellow = v('--warn'), blue = v('--info')
  const magenta = v('--violet') || v('--accent'), cyan = v('--src-calendar') || blue
  // Black and white swap on a light background: ANSI "white" must stay visible.
  const black = light ? fg : v('--surface-muted')
  const white = light ? v('--surface-muted') : fg
  return {
    background: bg, foreground: fg, cursor: fg, cursorAccent: bg,
    selectionBackground: withAlpha(v('--accent'), light ? 0.25 : 0.35),
    black, red, green, yellow, blue, magenta, cyan, white,
    brightBlack: v('--text-muted'), brightRed: bright(red), brightGreen: bright(green), brightYellow: bright(yellow),
    brightBlue: bright(blue), brightMagenta: bright(magenta), brightCyan: bright(cyan), brightWhite: light ? bg : '#ffffff',
  }
}

/** `#rrggbb` → [r, g, b]; other format: null. */
function rgb(c: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{6})$/i.exec(c)
  if (!m) return null
  const n = parseInt(m[1], 16)
  return [n >> 16, (n >> 8) & 255, n & 255]
}

/** `t` of `a`, the rest of `b`; an unreadable color returns `a` as is. */
function mix(a: string, b: string, t: number): string {
  const x = rgb(a), y = rgb(b)
  if (!x || !y) return a
  return `#${x.map((c, i) => Math.round(c * t + y[i] * (1 - t)).toString(16).padStart(2, '0')).join('')}`
}

function withAlpha(c: string, alpha: number): string {
  const x = rgb(c)
  return x ? `rgba(${x.join(', ')}, ${alpha})` : c
}
