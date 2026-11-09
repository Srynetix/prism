import type { BlockDef } from '../block'
import type { StatusState } from './types'
import { t, tn } from '../../i18n.svelte'

/** Status pages: incidents under way at the services Prism leans on. */
export const status = {
  id: 'status' as const,
  label: t('Status pages'),
  category: 'Overview',
  help: {
    purpose: t('Is it you, or is GitLab (GitHub, ClickUp, Jira, Claude, Google Cloud) having an incident?'),
    points: [
      t('One line per service: its state from its public status page, green when all is operational.'),
      t('An incident under way shows below its service: its name, impact and status, how long ago it started, its latest update and the components it affects; a click opens it.'),
      t('In the status bar, while a service has an incident: its name, red for a major one; a click shows this block.'),
      t('In Alerts ([status] alerts): each incident under way, major and critical ones with a notification.'),
    ],
    source: t('The public status pages, without any token, every [status] poll_seconds: Atlassian Statuspage (GitHub, ClickUp, Jira, Claude and [status] pages), status.io (GitLab.com), Google Cloud’s incidents feed. By default the services Prism reads.'),
  },
  status: { label: t('Service incidents'), side: 'right', after: 'usage' },
  settings: {
    parent: 'general',
    node: { id: 'status', label: t('Status pages'), sections: ['status'], toggle: 'status' },
    sections: [
      {
        id: 'status', source: 'status', label: t('Status pages'), table: '[status]', path: ['status'],
        intro: t('The Status pages block: incidents under way at the services Prism leans on, from their public status pages (no token). Read at launch.'),
        fields: [
          { key: 'enabled', label: t('Status pages'), hint: t('On by default: a few public pages read every few minutes.'), kind: 'toggle' },
          { key: 'services', label: t('Services'), hint: t('github, gitlab (GitLab.com), clickup, jira, claude, gcp (Google Cloud). Empty: those Prism reads — its forge on gitlab.com or GitHub, its tracker, Claude when it runs the agents.'), kind: 'list', optional: true },
          {
            key: 'pages', label: t('Other pages'), kind: 'table', blank: { name: '', url: '' },
            hint: t('Any Atlassian Statuspage page (it answers /api/v2/summary.json): Sentry, Slack, Vercel…'),
            columns: [
              { key: 'name', label: t('Name'), kind: 'text', width: '140px' },
              { key: 'url', label: t('Page'), kind: 'text', placeholder: 'https://status.example.com' },
            ],
          },
          { key: 'alerts', label: t('In Alerts'), hint: t('Each incident under way: major and critical ones with a notification, minor ones without.'), kind: 'toggle' },
          { key: 'poll_seconds', label: t('Poll every'), hint: t('The pages are read again this often (↻ in the block: now). 60 at least.'), kind: 'number', unit: t('seconds') },
        ],
      },
    ],
  },
  badge: (s) => {
    const n = s.services.filter((x) => x.level === 'major' || x.level === 'critical').length
    return { count: n, title: tn(n, '{n} service with a major incident', '{n} services with a major incident') }
  },
} satisfies BlockDef<StatusState>
