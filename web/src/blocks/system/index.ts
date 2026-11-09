import type { BlockDef } from '../block'
import { HOT_SECONDS, LOSS, diskTone, type SystemState } from './types'
import { t, tn } from '../../i18n.svelte'

/** System: the computer's CPU, memory, disks and network, and who consumes them. */
export const system = {
  id: 'system' as const,
  label: t('System'),
  category: 'Infra',
  help: {
    purpose: t('Is the computer keeping up — and if not, is it the agents, Docker, a dev server, or Chrome?'),
    points: [
      t('CPU, memory, disk and network; CPU, memory and network with their last 10 minutes. The memory’s colour is macOS’s memory pressure: green, orange when it compresses, red when it swaps.'),
      t('Who consumes, grouped by what Prism knows: its agents (their processes under tmux), Docker (its virtual machine), the shells and dev servers, Prism itself, then the three heaviest other applications (Chrome’s helpers counted with Chrome).'),
      t('The network: the latency of a TCP handshake to the services Prism calls (its forge, its tracker, its agents’ API, and [system] hosts), the loss and jitter over 5 minutes, the default gateway, the traffic, and the Wi-Fi link’s signal and rate.'),
      t('The disk: the free space of the disk your home folder is on (orange under 10 % or 25 GB, red under 5 % or 10 GB, by default: [system] disk_warn_gb and the like), and of the external volumes plugged in. On macOS, without the space the system could purge (Finder counts it as available).'),
      t('Short: one line each for CPU, memory, disk and network. In the status bar: CPU, RAM and latency; a click shows this block.'),
      t('In Alerts ([system] alerts): the memory pressure red or the disk almost full (with a notification), the disk getting full, the CPU at 90 % or more for 5 minutes, more than 5 % of the handshakes lost.'),
    ],
    source: t('Local: the system’s counters and the disks’ space (sysinfo) every [system] poll_seconds; on macOS the memory pressure (sysctl), its breakdown (vm_stat) and the Wi-Fi link (CoreWLAN, never the network’s name). The network: a TCP handshake to each host every 15 s, no data sent.'),
  },
  status: { label: t('CPU, RAM, latency'), side: 'right', after: 'usage' },
  settings: {
    parent: 'infra',
    node: { id: 'system', label: t('System'), sections: ['system'], toggle: 'system' },
    sections: [
      {
        id: 'system', source: 'system', label: t('System'), table: '[system]', path: ['system'],
        intro: t('The System block: the computer’s CPU, memory, disks and network, and who consumes them. Read locally; the network is measured by a TCP handshake to a few hosts. Read at launch.'),
        fields: [
          { key: 'enabled', label: t('System'), hint: t('On by default: local counters, and a handshake to a few hosts every 15 s.'), kind: 'toggle' },
          { key: 'hosts', label: t('Hosts'), hint: t('More hosts to measure the latency to (git.example.com, example.com:8443), beyond those Prism calls: its forge, its tracker, its agents’ API.'), kind: 'list', optional: true },
          { key: 'alerts', label: t('In Alerts'), hint: t('The memory pressure red or the disk almost full (with a notification), the disk getting full, the CPU at 90 % or more for 5 minutes, more than 5 % of the handshakes lost.'), kind: 'toggle' },
          { key: 'disk_warn_gb', label: t('Disk getting full'), hint: t('The home disk in orange, in Alerts, under this much free space. 0: not by size.'), kind: 'number', unit: t('GB') },
          { key: 'disk_warn_percent', label: t('Disk getting full (share)'), hint: t('Or under this share of the disk free. 0: not by share.'), kind: 'number', unit: '%' },
          { key: 'disk_alert_gb', label: t('Disk almost full'), hint: t('In red, with a notification, under this much free space. 0: not by size.'), kind: 'number', unit: t('GB') },
          { key: 'disk_alert_percent', label: t('Disk almost full (share)'), hint: t('Or under this share of the disk free. 0: not by share.'), kind: 'number', unit: '%' },
          { key: 'poll_seconds', label: t('Read every'), hint: t('CPU, memory and processes read again this often. 2 at least.'), kind: 'number', unit: t('seconds') },
        ],
      },
    ],
  },
  badge: (s) => {
    const n = (s.memory.pressure === 'critical' ? 1 : 0) + (s.cpu.hot_seconds >= HOT_SECONDS ? 1 : 0) + (s.network.loss > LOSS ? 1 : 0) + (s.disks?.[0] && diskTone(s.disks[0], s.disk_limits) === 'ko' ? 1 : 0)
    return { count: n, title: tn(n, '{n} system alert', '{n} system alerts') }
  },
} satisfies BlockDef<SystemState>
