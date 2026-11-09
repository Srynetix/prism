import type { BlockDef } from '../block'
import type { CustomBlocksState } from './types'
import { t, tn } from '../../i18n.svelte'

/**
 * Custom blocks (`[[custom_blocks]]`): a command or a script that prints JSON, made a block. Each
 * one is a page block of its own, `custom:<id>` (views.ts); this one shows them all, one after the
 * other (a phone's view), and holds their alerts, status bar item and help.
 */
export const customBlocks = {
  id: 'custom_blocks' as const,
  label: t('Custom blocks'),
  category: 'Custom',
  help: {
    purpose: t('Your own blocks, without Rust nor Svelte: a command or a script of config.toml prints JSON, Prism makes a block of it.'),
    points: [
      t('[[custom_blocks]] in config.toml (or Settings › Interface › Custom blocks): a command (a list of arguments, never a shell) or a script written in the config and run by its shebang (bash, python, node, uv run --script…), every every_seconds or on demand (↻).'),
      t('Its output, a JSON object: kind list (title, sub, state ok/warn/ko, badge, url), gauge or stats (numbers with warn/ko thresholds, a sparkline of their last values), markdown, or table (columns and rows, sorted by a click on a column). Anything that is not JSON shows as it is.'),
      t('alerts in the output go to Alerts (one per key), status to the status bar; a failure, a timeout or JSON off the contract shows in Problems with the end of its error output.'),
      t('Actions ([[custom_blocks.actions]], from the config only): a button on the lines they apply to (when), running a command, a script or an agent, the line given as PRISM_ITEM; the block is read again after.'),
      t('Each one is a block of its own on a page: custom:<id> (+ Block lists them). A block added or changed outside Settings (an editor, an agent, a pack) waits for your trust before it runs.'),
    ],
    source: t('Local: the program you declared, run with your rights, at the pace you set, its output capped (1 MB) and killed with its children past its timeout. Nothing runs in the demo nor before you trust it.'),
  },
  status: { label: t('Custom blocks'), side: 'right', after: 'problems' },
  badge: (s) => {
    const n = s.blocks.filter((b) => b.pending).length
    return { count: n, title: tn(n, '{n} custom block waits for your trust', '{n} custom blocks wait for your trust') }
  },
} satisfies BlockDef<CustomBlocksState>
