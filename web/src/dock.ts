import { invoke, isDesktop } from './bridge'
import { initial, profileColor } from './profiles.svelte'

/**
 * macOS Dock: with several profiles, this Prism's icon carries its profile's letter on its color,
 * so that two Prism open side by side are told apart in the Dock and ⌘-Tab. Drawn here (a canvas
 * renders text and the theme's colors), applied by the core (`set_dock_icon`).
 */
let drawn = ''

export async function markDock(profile: { id: string; name: string; color: string; shared: boolean } | undefined) {
  if (!isDesktop() || !profile?.shared || !navigator.userAgent.includes('Mac')) return
  const key = `${profile.id}|${profile.name}|${profile.color}`
  if (key === drawn) return
  drawn = key
  try {
    const bytes = new Uint8Array(await invoke<ArrayBuffer>('app_icon'))
    const icon = await createImageBitmap(new Blob([bytes], { type: 'image/png' }))
    const size = 512
    const canvas = document.createElement('canvas')
    canvas.width = canvas.height = size
    const g = canvas.getContext('2d')!
    g.drawImage(icon, 0, 0, size, size)
    // The badge: a rounded square at the bottom right, the letter in white on the profile's color.
    const color = resolveColor(profileColor(profile.color))
    const b = size * 0.42
    const x = size - b - size * 0.04
    const y = size - b - size * 0.04
    g.fillStyle = color
    g.strokeStyle = '#ffffff'
    g.lineWidth = size * 0.025
    g.beginPath()
    g.roundRect(x, y, b, b, b * 0.24)
    g.fill()
    g.stroke()
    g.fillStyle = '#ffffff'
    g.font = `700 ${Math.round(b * 0.62)}px Inter, system-ui, sans-serif`
    g.textAlign = 'center'
    g.textBaseline = 'middle'
    g.fillText(initial(profile.name), x + b / 2, y + b / 2 + b * 0.03)
    const png = await new Promise<Blob | null>((ok) => canvas.toBlob(ok, 'image/png'))
    if (png) await invoke('set_dock_icon', { png: Array.from(new Uint8Array(await png.arrayBuffer())) })
  } catch (e) {
    drawn = ''
    console.error('dock icon', e)
  }
}

/** `var(--src-clickup)` → its value in the current theme. */
function resolveColor(css: string): string {
  const probe = document.createElement('span')
  probe.style.color = css
  document.body.appendChild(probe)
  const value = getComputedStyle(probe).color
  probe.remove()
  return value || '#c96442'
}
