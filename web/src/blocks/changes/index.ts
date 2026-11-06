import type { BlockDef } from '../block'
import type { ChangesState } from './changes'
import { t } from '../../i18n.svelte'

/** Changes: what touched an environment, on one timeline; the first screen to open on a CrashLoop. */
export const changes = {
  id: 'changes' as const,
  label: t('Changes'),
  category: 'Infra',
  help: {
    purpose: t('Since yesterday, what changed on staging? Argo CD syncs, MRs merged, pods restarting, alerts and vulnerabilities of an environment, on one timeline.'),
    points: [
      t('The band: a lane per source, a mark per change, the ranges Prism was closed hatched; drag on it to zoom on a range. Below, the list by hour, newest first, repeats folded (a pod restarting 6 times in 4 min is one line).'),
      t('A crash (CrashLoop, OOMKilled, restarts, an application turned Degraded) shows the sync or rollout of the same application just before it ([changes] correlate_minutes) as “probably linked”: a guess on time and name, not a certain cause. Diagnose sends an agent in plan mode with the timeline, the logs and the diff of that change.'),
      t('“What changed” in a pod’s logs, on a crash alert and in the Argo CD panel of a Degraded application opens the timeline around the first crash, the linked change highlighted. The palette (⌘K) opens it on an environment.'),
      t('Times are the source’s when it gives one (Argo CD’s history, merged_at, a container’s termination): right even for what happened while Prism was closed. Alerts and vulnerabilities only exist from when Prism saw them (“seen”). Copy gives the range as Markdown.'),
    ],
    source: t('What Prism already reads (Argo CD, the clusters, the forge, the alerts, the scans) compared from one read to the next, plus the Kubernetes events with [changes] kube_events and what the blocks of a pack add. Kept [changes] retention_days in the profile’s data folder (changes.jsonl).'),
  },
  settings: {
    parent: 'infra',
    node: { id: 'changes', label: t('Changes'), sections: ['changes'], toggle: 'changes' },
    sections: [
      {
        id: 'changes', source: 'changes', label: t('Changes'), table: '[changes]', path: ['changes'],
        intro: t('The Changes block: what touched an environment on one timeline. Its journal stays on this machine (changes.jsonl in the profile’s data folder).'),
        fields: [
          { key: 'enabled', label: t('Changes'), hint: t('On by default: it reads only what Prism already reads, and compares.'), kind: 'toggle' },
          { key: 'sources', label: t('Sources'), hint: t('What goes on the timeline. packs: the changes the blocks of a pack add (a block’s id names one alone).'), kind: 'checks', options: ['argocd', 'mrs', 'pods', 'alerts', 'vulnerabilities', 'packs'] },
          {
            key: 'branches', label: t('Branch per environment'), hint: t('The branch that feeds each environment (release/* for any release branch): the MRs merged into it go on its timeline. None: every merged MR, on every environment.'), kind: 'table', blank: { env: '', branch: '' },
            columns: [
              { key: 'env', label: t('Environment'), kind: 'text', options: 'envs', width: '140px' },
              { key: 'branch', label: t('Branch'), kind: 'text', placeholder: 'main' },
            ],
          },
          { key: 'correlate_minutes', label: t('Probably linked'), hint: t('A crash is linked to a sync or a rollout of the same application this long before it.'), kind: 'number', unit: t('minutes') },
          { key: 'default_period', label: t('Opens on'), hint: t('The period the block shows first: 1h, since_yesterday, 24h, 7d.'), kind: 'select', options: ['1h', 'since_yesterday', '24h', '7d'] },
          { key: 'kube_events', label: t('Kubernetes events'), hint: t('Also reads the warnings of each environment’s namespace (BackOff, OOMKilling, FailedScheduling): an hour kept by the cluster, and the right to list events. Restarts and OOMKilled come from the pods without it.'), kind: 'toggle' },
          { key: 'retention_days', label: t('Keep'), hint: t('Days of changes kept.'), kind: 'number', unit: t('days') },
        ],
      },
    ],
  },
} satisfies BlockDef<ChangesState>
