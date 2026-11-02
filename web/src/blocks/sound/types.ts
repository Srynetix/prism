import { t } from '../../i18n.svelte'
import type { SoundSide } from '../../bindings/SoundSide'
import type { SoundState } from '../../bindings/SoundState'

/** The Sound block's state: Rust's types (`crates/blocks/src/sound`), generated in `bindings/`. */
export type { SoundSide, SoundState }

export type Transport = 'builtin' | 'bluetooth' | 'usb' | 'display' | 'airplay' | 'virtual' | 'continuity' | 'thunderbolt' | 'other'

export type Problem = { volume: number }

/** What is wrong with the microphone, by `[sound]`: its input volume under the minimum. Muted is not an alert, nor a volume
 *  under `muted_under` (a headset's boom folded up sets it to 0–1 %). */
export function micProblem(s: SoundState | undefined): Problem | null {
  const i = s?.input
  if (!s?.updated_at || !i) return null
  if (i.muted) return null
  if (i.volume !== null && i.volume < (s.limits.muted_under ?? 0)) return null
  if (s.limits.min_input_gain > 0 && i.volume !== null && i.volume < s.limits.min_input_gain) return { volume: i.volume }
  return null
}

/** Who records, in words: `Google Chrome`, `Zoom and Slack`. */
export const recorders = (s: SoundState) => s.recording.map((r) => r.app).join(t(' and '))
