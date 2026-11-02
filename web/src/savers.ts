/**
 * Screensavers drawn on a canvas, next to Pipes and Bounce (Screensaver.svelte):
 * each one resets to the window size, then draws one image per frame, with the elapsed
 * time as a parameter (same speed whatever the frame rate).
 */
export interface CanvasSaver {
  /** `words`: words of the moment (tickets, MRs, branches), for those that write. */
  reset(w: number, h: number, words: string[]): void
  frame(g: CanvasRenderingContext2D, dt: number): void
}

const BG = '#050507'
const HUES = ['#ff5f6d', '#ffc371', '#fff36e', '#7cf29a', '#5ec8ff', '#a78bfa']
const rand = (a: number, b: number) => a + Math.random() * (b - a)

/** Code rain, the columns writing the tickets, MRs and branches of the moment. */
function matrix(): CanvasSaver {
  const SIZE = 15
  let w = 0
  let h = 0
  let words: string[] = []
  let cols: { y: number; speed: number; word: string; i: number }[] = []
  let font = ''
  const pick = () => (words.length ? words[(Math.random() * words.length) | 0] : 'PRISM')
  return {
    reset(W, H, list) {
      w = W
      h = H
      words = list.length ? list : ['PRISM']
      // The canvas does not read CSS variables: the app's mono font, resolved.
      font = `600 ${SIZE - 2}px ${getComputedStyle(document.documentElement).getPropertyValue('--mono').trim() || 'monospace'}`
      cols = Array.from({ length: Math.ceil(w / SIZE) }, () => ({ y: rand(-h, 0), speed: rand(8, 22), word: pick(), i: 0 }))
    },
    frame(g, dt) {
      g.fillStyle = 'rgba(5,5,7,0.05)'
      g.fillRect(0, 0, w, h)
      g.font = font
      g.textAlign = 'center'
      cols.forEach((c, x) => {
        const before = Math.floor(c.y / SIZE)
        c.y += (c.speed * SIZE * dt) / 1000
        const after = Math.floor(c.y / SIZE)
        // One more character of the word at each cell crossed; the head in white, the trail in green.
        for (let row = before + 1; row <= after; row++) {
          if (row < 0) continue
          const ch = c.word[c.i % c.word.length]
          c.i++
          g.fillStyle = '#7ee0a8'
          g.fillText(c.word[(c.i + c.word.length - 2) % c.word.length], x * SIZE + SIZE / 2, (row - 1) * SIZE)
          g.fillStyle = '#e8fff1'
          g.fillText(ch, x * SIZE + SIZE / 2, row * SIZE)
        }
        if (c.y > h + rand(0, h)) Object.assign(c, { y: rand(-h / 2, 0), speed: rand(8, 22), word: `${pick()} `, i: 0 })
      })
    },
  }
}

/** Starfield in hyperspace. */
function stars(): CanvasSaver {
  const N = 420
  let w = 0
  let h = 0
  let list: { x: number; y: number; z: number; pz: number; hue: string }[] = []
  const spawn = (far = true) => {
    const z = far ? rand(0.6, 1) : rand(0.05, 1)
    return { x: rand(-1, 1), y: rand(-1, 1), z, pz: z, hue: Math.random() < 0.15 ? HUES[(Math.random() * HUES.length) | 0] : '#dfe4ff' }
  }
  return {
    reset(W, H) {
      w = W
      h = H
      list = Array.from({ length: N }, () => spawn(false))
    },
    frame(g, dt) {
      g.fillStyle = 'rgba(5,5,7,0.35)'
      g.fillRect(0, 0, w, h)
      const cx = w / 2
      const cy = h / 2
      const scale = Math.max(w, h) * 0.5
      for (const s of list) {
        s.pz = s.z
        s.z -= (dt / 1000) * 0.32
        if (s.z <= 0.02) {
          Object.assign(s, spawn())
          continue
        }
        const px = cx + (s.x / s.pz) * scale
        const py = cy + (s.y / s.pz) * scale
        const x = cx + (s.x / s.z) * scale
        const y = cy + (s.y / s.z) * scale
        if (x < 0 || x > w || y < 0 || y > h) {
          Object.assign(s, spawn())
          continue
        }
        g.strokeStyle = s.hue
        g.globalAlpha = Math.min(1, 1.2 - s.z)
        g.lineWidth = Math.max(0.6, 2.4 * (1 - s.z))
        g.beginPath()
        g.moveTo(px, py)
        g.lineTo(x, y)
        g.stroke()
      }
      g.globalAlpha = 1
    },
  }
}

/** Game of life: one color per cell age; reseeds when it falls asleep. */
function life(): CanvasSaver {
  const CELL = 10
  const STEP_MS = 45
  let cols = 0
  let rows = 0
  let w = 0
  let h = 0
  let age = new Uint16Array(0)
  let acc = 0
  let gens = 0
  let still = 0
  let last = 0
  const seed = () => {
    age = new Uint16Array(cols * rows).map(() => (Math.random() < 0.24 ? 1 : 0))
    gens = 0
    still = 0
  }
  function step() {
    const next = new Uint16Array(cols * rows)
    let alive = 0
    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        let n = 0
        for (let dy = -1; dy <= 1; dy++)
          for (let dx = -1; dx <= 1; dx++) if ((dx || dy) && age[((y + dy + rows) % rows) * cols + ((x + dx + cols) % cols)]) n++
        const i = y * cols + x
        if (age[i] ? n === 2 || n === 3 : n === 3) {
          next[i] = Math.min(age[i] + 1, 999)
          alive++
        }
      }
    }
    age = next
    gens++
    still = Math.abs(alive - last) < 3 ? still + 1 : 0
    last = alive
    if (still > 40 || gens > 900 || alive === 0) seed()
  }
  return {
    reset(W, H) {
      w = W
      h = H
      cols = Math.floor(w / CELL)
      rows = Math.floor(h / CELL)
      seed()
    },
    frame(g, dt) {
      acc = Math.min(acc + dt, 1000)
      if (acc < STEP_MS) return
      while (acc >= STEP_MS) {
        acc -= STEP_MS
        step()
      }
      g.fillStyle = BG
      g.fillRect(0, 0, w, h)
      for (let i = 0; i < age.length; i++) {
        if (!age[i]) continue
        g.fillStyle = HUES[Math.min(HUES.length - 1, Math.floor(Math.log2(age[i])))]
        g.fillRect((i % cols) * CELL + 1, Math.floor(i / cols) * CELL + 1, CELL - 2, CELL - 2)
      }
    },
  }
}

/** Prism's prism: a white ray that oscillates, the rainbow coming out of it, photons racing along. */
function spectrum(): CanvasSaver {
  let w = 0
  let h = 0
  let t = 0
  let photons: { ray: number; p: number; v: number }[] = []
  return {
    reset(W, H) {
      w = W
      h = H
      t = 0
      photons = Array.from({ length: 90 }, () => ({ ray: (Math.random() * (HUES.length + 1)) | 0, p: Math.random(), v: rand(0.12, 0.3) }))
    },
    frame(g, dt) {
      t += dt / 1000
      const size = Math.min(w, h) * 0.22
      const cx = w / 2
      const cy = h / 2
      // Vertices of the prism, and the ray's entry point on its left face.
      const top = [cx, cy - size * 0.62]
      const left = [cx - size * 0.62, cy + size * 0.45]
      const right = [cx + size * 0.62, cy + size * 0.45]
      const inY = cy + Math.sin(t * 0.4) * size * 0.12
      const entry = [cx - size * 0.32, inY]
      const exit = [cx + size * 0.3, inY + size * 0.05]
      const start = [0, inY - Math.sin(t * 0.4) * size * 0.6 - size * 0.1]
      const fan = 0.06 + 0.025 * Math.sin(t * 0.7)
      const ray = (i: number) => {
        const a = 0.05 + (i - (HUES.length - 1) / 2) * fan + Math.sin(t * 0.4) * 0.05
        return [exit[0] + Math.cos(a) * w, exit[1] + Math.sin(a) * w]
      }
      const at = (from: number[], to: number[], p: number) => [from[0] + (to[0] - from[0]) * p, from[1] + (to[1] - from[1]) * p]

      g.fillStyle = BG
      g.fillRect(0, 0, w, h)
      g.lineCap = 'round'
      // White ray.
      g.shadowColor = '#fff'
      g.shadowBlur = 14
      g.strokeStyle = '#fff'
      g.lineWidth = 3
      g.beginPath()
      g.moveTo(start[0], start[1])
      g.lineTo(entry[0], entry[1])
      g.lineTo(exit[0], exit[1])
      g.stroke()
      // Rainbow.
      HUES.forEach((c, i) => {
        const [x, y] = ray(i)
        g.shadowColor = c
        g.strokeStyle = c
        g.globalAlpha = 0.75 + 0.25 * Math.sin(t * 2 + i)
        g.lineWidth = 4
        g.beginPath()
        g.moveTo(exit[0], exit[1])
        g.lineTo(x, y)
        g.stroke()
      })
      g.globalAlpha = 1
      // Photons: along the white ray (ray = HUES.length), then along their color.
      g.shadowBlur = 8
      for (const ph of photons) {
        ph.p += (ph.v * dt) / 1000
        if (ph.p > 1) Object.assign(ph, { ray: (Math.random() * (HUES.length + 1)) | 0, p: 0 })
        const white = ph.ray === HUES.length
        const [x, y] = white ? at(start, entry, ph.p) : at(exit, ray(ph.ray), ph.p)
        g.fillStyle = white ? '#fff' : HUES[ph.ray]
        g.shadowColor = g.fillStyle
        g.beginPath()
        g.arc(x, y, 2, 0, Math.PI * 2)
        g.fill()
      }
      // The prism on top.
      g.shadowColor = '#a78bfa'
      g.shadowBlur = 18
      g.fillStyle = 'rgba(167,139,250,0.22)'
      g.strokeStyle = '#a78bfa'
      g.lineWidth = 2
      g.beginPath()
      g.moveTo(top[0], top[1])
      g.lineTo(right[0], right[1])
      g.lineTo(left[0], left[1])
      g.closePath()
      g.fill()
      g.stroke()
      g.shadowBlur = 0
    },
  }
}

export const CANVAS_SAVERS = { matrix, stars, life, spectrum } satisfies Record<string, () => CanvasSaver>
