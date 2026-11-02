import type { BlockDef } from '../block'
import type { TokensState } from './types'
import { t } from '../../i18n.svelte'

/** Tokens: what the coding tools consumed, by week and by month. */
export const tokens = {
  id: 'tokens' as const,
  label: t('Tokens'),
  category: 'Code & agents',
  help: {
    purpose: t('How many tokens did your coding tools use this week, this month — and where did they go?'),
    points: [
      t('Week (Monday to Sunday) or Month: the period’s total, against the one before (the same days, for the period under way), and its cost: Claude Code’s and Codex’s at the API prices (a figure to compare on a subscription, not a bill), Command Code’s and OpenCode’s as they count it.'),
      t('A bar a day; ‹ › go back in time, as far as the history goes. Hover a bar for its day.'),
      t('Where they went: output, input, cache writes and reads (most of the volume, and cheap); the tools, when there are several; the models; the projects (the folder each session ran in).'),
      t('With several tools, All or one of them alone (a Tool row too): the total, the bars, the cost and where they went, that tool’s only. The choice is the same on the phone.'),
      t('Every session of this computer, Prism’s agents or not: Claude Code, Codex, Command Code, OpenCode ([tokens] sources).'),
    ],
    source: t('The tools’ usage figures, never what was said: Claude Code’s transcripts (~/.claude/projects), Codex’s rollouts (~/.codex/sessions), Command Code’s sessions (~/.commandcode/projects), OpenCode’s database (~/.local/share/opencode, read-only). Read at launch, then what was added every [tokens] poll_seconds. Daily totals are kept in Prism’s data folder for [tokens] keep_days: the tools delete their sessions after a while, the history stays.'),
  },
  settings: {
    parent: 'agents-root',
    node: { id: 'tokens', label: t('Tokens'), sections: ['tokens'], toggle: 'tokens' },
    sections: [
      {
        id: 'tokens', source: 'tokens', label: t('Tokens'), table: '[tokens]', path: ['tokens'],
        intro: t('The Tokens block: what the coding tools consumed by week and by month, from their usage figures (never what was said). Read at launch.'),
        fields: [
          { key: 'enabled', label: t('Tokens'), hint: t('On by default: the sessions are local, only their usage figures are read.'), kind: 'toggle' },
          { key: 'sources', label: t('Tools'), hint: t('claude (Claude Code), codex, command-code, opencode. Empty: all of them; a tool not installed has nothing to read.'), kind: 'list', optional: true, placeholder: t('all the tools') },
          { key: 'keep_days', label: t('History'), hint: t('Days of daily totals kept, beyond what the tools keep of their sessions (a month for Claude Code).'), kind: 'number', unit: t('days') },
          { key: 'poll_seconds', label: t('Read every'), hint: t('What the sessions gained since (↻ in the block: now). 60 at least.'), kind: 'number', unit: t('seconds') },
        ],
      },
    ],
  },
} satisfies BlockDef<TokensState>
