import type { BlockDef } from '../block'
import type { TrailState } from './trail'
import { t } from '../../i18n.svelte'

/** Day trail: your day rebuilt from its traces, to log time and to prepare the standup. */
export const trail = {
  id: 'trail' as const,
  label: t('Day trail'),
  category: 'Overview',
  help: {
    purpose: t('What did you do today, or on the day picked in the header, without writing it down?'),
    points: [
      t('Timeline: your traces in order (commits, MRs opened, merged, reviewed, tickets moved, agents, meetings, time logged), alike ones grouped; a stretch of the working day without any trace is flagged ([trail] gap_minutes).'),
      t('By ticket: the time Prism estimates from the traces against the time logged; Log adds the difference to the ticket, Log all does every ticket. Nothing is logged without your click.'),
      t('Daily: the standup from the working day before (merged, opened, worked on, reviewed) and what waits today (MRs to get green or reviewed, reviews for you, tickets in progress, meetings), blockers from the tickets’ dependencies. Edit it, then Copy.'),
    ],
    source: t('Your commits in the local clones (git log, your user.email), your own events on the forge, your tickets’ status changes as Prism sees them, your meetings once over; kept [trail] keep_days in the profile’s data folder. Agents and logged time come from Prism itself.'),
  },
  settings: {
    parent: 'work',
    node: { id: 'trail', label: t('Day trail'), sections: ['trail'], toggle: 'trail' },
    sections: [
      {
        id: 'trail', source: 'trail', label: t('Day trail'), table: '[trail]', path: ['trail'],
        intro: t('The Day trail block: your day rebuilt from its traces, to log time and to prepare the standup. Its journal stays on this machine (trail.json in the profile’s data folder). Read at launch.'),
        fields: [
          { key: 'enabled', label: t('Day trail'), hint: t('On by default: reads your commits locally and your own events on the forge every few minutes.'), kind: 'toggle' },
          { key: 'commits', label: t('Commits'), hint: t('Your commits in the local clones of the repositories (git log, by your user.email).'), kind: 'toggle' },
          { key: 'forge', label: t('Forge events'), hint: t('Your MRs opened, merged, approved and commented, from the forge’s events (GitLab events, GitHub users/<you>/events).'), kind: 'toggle' },
          { key: 'tickets', label: t('Ticket moves'), hint: t('Your tickets changing status, as Prism sees them change (while it runs).'), kind: 'toggle' },
          { key: 'meetings', label: t('Meetings'), hint: t('Your meetings once over; declined, canceled and free ones left out.'), kind: 'toggle' },
          { key: 'gap_minutes', label: t('Gap'), hint: t('A stretch of the working day (9:00 to 18:30) this long without any trace is flagged. 0: never.'), kind: 'number', unit: t('minutes') },
          { key: 'keep_days', label: t('Keep'), hint: t('Days of traces kept.'), kind: 'number', unit: t('days') },
          { key: 'poll_seconds', label: t('Poll every'), hint: t('The sources are read again this often (↻ in the block: now).'), kind: 'number', unit: t('seconds') },
        ],
      },
    ],
  },
} satisfies BlockDef<TrailState>
