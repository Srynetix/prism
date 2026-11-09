import type { BlockDef } from '../block'
import type { DockerContainer, DockerState } from './types'
import { t } from '../../i18n.svelte'

/**
 * A container in trouble: unhealthy, restarting in a loop, or exited with an error (137 and 143,
 * a stop's kill and term, are no error). A container stopped by hand (exit 0) is not one.
 */
export const containerFailing = (c: DockerContainer) =>
  c.health === 'unhealthy' || c.state === 'restarting' || c.state === 'dead' || (c.state === 'exited' && !!c.exit_code && c.exit_code !== 137 && c.exit_code !== 143)

export const docker = {
  id: 'docker' as const,
  label: 'Docker',
  category: 'Infra',
  help: {
    purpose: t('What runs in your local Docker, and what its disk is spent on?'),
    points: [
      t('The disk per kind (images, containers, volumes, build cache): the reclaimable part in orange, and a prune per kind (two clicks; the volume prune removes every volume no container uses, named ones included).'),
      t('Containers by Compose project: state and health, ports, CPU and memory; start, stop, restart, log, and remove once stopped.'),
      t('Images (their own size, unused ones removable) and volumes (unused ones flagged).'),
      t('In Alerts: a container unhealthy, restarting or exited with an error, Docker not answering, and reclaimable disk beyond [docker] reclaimable_gb.'),
    ],
    source: t('The docker CLI of the current context (or [docker] context): containers every [docker] poll_seconds, the disk every 3 minutes and after an action.'),
  },
  settings: {
    parent: 'infra',
    node: { id: 'docker', label: 'Docker', sections: ['docker'], toggle: 'docker' },
    sections: [
      {
        id: 'docker', source: 'docker', label: 'Docker', table: '[docker]', path: ['docker'],
        intro: t('The Docker block: containers, images, volumes and their disk, through the docker CLI (Docker Desktop, OrbStack, colima…). It starts, stops, restarts and removes containers, removes images and prunes unused data. Read at launch.'),
        fields: [
          { key: 'enabled', label: 'Docker', hint: t('Off by default. On: the docker CLI must reach a daemon; add the Docker block to a page.'), kind: 'toggle' },
          { key: 'context', label: t('Context'), hint: t('The docker context read. Empty: the current one (docker context use).'), kind: 'text', optional: true, placeholder: t('current') },
          { key: 'poll_seconds', label: t('Poll every'), hint: t('Containers and their usage; the disk is measured every 3 minutes.'), kind: 'number', unit: t('seconds') },
          { key: 'reclaimable_gb', label: t('Reclaimable'), hint: t('Docker’s unused images, volumes and build cache beyond this are flagged in Alerts (formerly [alerts] docker_reclaimable_gb). 0: never.'), kind: 'number', unit: t('GB') },
        ],
      },
    ],
  },
} satisfies BlockDef<DockerState>
