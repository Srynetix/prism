import type { BlockDef } from '../block'
import { micProblem, type SoundState } from './types'
import { t } from '../../i18n.svelte'

/** Sound: the output and the microphone, and the microphone too low. */
export const sound = {
  id: 'sound' as const,
  label: t('Sound'),
  category: 'Infra',
  help: {
    purpose: t('Will they hear you? The output and microphone at a glance, and a warning when the microphone is too low before you talk into it.'),
    points: [
      t('Output and microphone: the system’s default device (a list to change it), its volume and mute. The microphone’s volume is its input volume (its gain), as in the system’s sound settings.'),
      t('Who records: the applications using a microphone now (a call in Chrome, Zoom, Teams…).'),
      t('In Alerts: the microphone’s input volume under [sound] min_input_gain (40 % by default), with “Raise” in one click (a muted microphone is not an alert, nor one under [sound] muted_under, 3 % by default: a headset’s boom folded up). Orange at once; red and notified while an application records from it.'),
      t('In the status bar: the output volume and the microphone’s beside it (orange while too low); the wheel sets the volume it is over (output or microphone), a click opens a panel with the output and the microphone (mute, volume, device, “Raise”). With [sound] status_volume off: nothing, unless the microphone is too low.'),
      t('Short: one line for the output, one for the microphone, each with its mute and its volume slider.'),
    ],
    source: t('Local: the audio devices’ settings (CoreAudio, PulseAudio, Windows’ Core Audio) every [sound] poll_seconds, and at once after a change. Nothing is listened to and no permission is asked: Prism reads the settings, never the sound. macOS, Linux and Windows.'),
  },
  status: { label: t('Sound'), side: 'right', after: 'system' },
  settings: {
    parent: 'infra',
    node: { id: 'sound', label: t('Sound'), sections: ['sound'], toggle: 'sound' },
    sections: [
      {
        id: 'sound', source: 'sound', label: t('Sound'), table: '[sound]', path: ['sound'],
        intro: t('The Sound block (macOS, Linux, Windows): the output and the microphone, their device, volume and mute, changed from the block. Read from the system’s audio settings: nothing is listened to, no permission asked. Read at launch.'),
        fields: [
          { key: 'enabled', label: t('Sound'), hint: t('On by default: the audio devices’ settings, read locally.'), kind: 'toggle' },
          { key: 'min_input_gain', label: t('Microphone too low'), hint: t('The microphone’s input volume under this in Alerts, notified while an application records from it. 0: never.'), kind: 'number', unit: '%' },
          { key: 'muted_under', label: t('Turned off under'), hint: t('An input volume under this is a microphone turned off, like a muted one: no alert (a headset’s boom folded up sets it to 0–1 %). 0: every volume counts.'), kind: 'number', unit: '%' },
          { key: 'restore_gain', label: t('Raise to'), hint: t('What “Raise” puts the input volume back to.'), kind: 'number', unit: '%' },
          { key: 'status_volume', label: t('Volume in the status bar'), hint: t('The status bar item always there with the output and microphone volumes: the wheel sets the one it is over, a click opens the output and the microphone. Off: only while the microphone is too low.'), kind: 'toggle' },
          { key: 'poll_seconds', label: t('Read every'), hint: t('The audio devices read again this often; at once after a change from the block. 1 at least.'), kind: 'number', unit: t('seconds') },
        ],
      },
    ],
  },
  badge: (s) => {
    const n = micProblem(s) && s.recording.length ? 1 : 0
    return { count: n, title: n ? t('Microphone too low while recording') : '' }
  },
} satisfies BlockDef<SoundState>
