import type { BlockDef } from '../block'
import type { DiskUsageState } from './types'
import { t } from '../../i18n.svelte'

/** Disk usage: where the space of a folder or a disk goes, scanned on demand. */
export const diskUsage = {
  id: 'disk_usage' as const,
  label: t('Disk usage'),
  category: 'Infra',
  help: {
    purpose: t('Where does the space go? A folder or the whole disk scanned on demand, its folders by size, and what can be rebuilt freed in two clicks.'),
    points: [
      t('Scan: a folder of [disk_usage] roots (the home folder, the whole disk…) or any other, chosen from the block; Rescan reads it again. Nothing is watched between two scans.'),
      t('The list of its folders by size, opened one level at a time, with their share, and Δ: how much each grew or shrank since the scan before. Beside it, the treemap: each folder a rectangle of its size, three levels deep, colored by type (build, cache, VMs, media, models…) or by folder.'),
      t('A click selects, a double click opens a folder (the path above goes back up), a right click gives its actions: show it in the file manager, a terminal there, copy its path, scan it alone, move it to the Trash (two clicks).'),
      t('Reclaimable: what can be rebuilt, found by its name (node_modules, Rust target/, .venv, .next, DerivedData, the caches, the iOS simulators, Docker’s image, the Trash), with how it is freed; each group goes in two clicks.'),
      t('The bar at the top: the disk, the folder scanned in it, the rest used, and the free space.'),
    ],
    source: t('Local: the folder read once per scan, on several threads, what the files take on the disk (a sparse disk image counts what it holds, a hard link once). Other disks mounted inside it and folders kept in the cloud only (iCloud Drive, OneDrive…) are not entered. The last scan of each folder is kept in Prism’s data folder, for Δ and to show it again after a restart. macOS, Linux and Windows.'),
  },
  settings: {
    parent: 'infra',
    node: { id: 'disk_usage', label: t('Disk usage'), sections: ['disk_usage'], toggle: 'disk_usage' },
    sections: [
      {
        id: 'disk_usage', source: 'disk_usage', label: t('Disk usage'), table: '[disk_usage]', path: ['disk_usage'],
        intro: t('The Disk usage block (macOS, Linux): a folder or a disk scanned on demand, where its space goes, and what can be rebuilt freed from the block. Nothing is read until a scan is asked for.'),
        fields: [
          { key: 'enabled', label: t('Disk usage'), hint: t('On by default: nothing is read until you scan.'), kind: 'toggle' },
          { key: 'roots', label: t('Folders offered'), hint: t('Offered to scan, one per line: ~ for the home folder, / for the whole disk (C: on Windows). Any other can be chosen from the block.'), kind: 'list' },
          { key: 'exclude', label: t('Never entered'), hint: t('Folders the scan leaves out, one per line (~ allowed).'), kind: 'list' },
          { key: 'one_volume', label: t('One disk'), hint: t('Stay on the scanned folder’s disk: other disks and network shares mounted inside it are left out.'), kind: 'toggle' },
          { key: 'delta', label: t('Growth (Δ)'), hint: t('How much each folder grew since the scan before: the last scan of each folder is kept.'), kind: 'toggle' },
          { key: 'cleanup', label: t('Cleanup'), hint: t('Reclaimable and its buttons, and the Trash from the block. Off: the block only reads.'), kind: 'toggle' },
        ],
      },
    ],
  },
} satisfies BlockDef<DiskUsageState>
