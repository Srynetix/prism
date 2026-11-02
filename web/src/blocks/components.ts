import { EXTRA_COMPONENTS } from '$extra-components'
import type { BlockComponent } from './block'
import BigQueryCost from './data/BigQueryCost.svelte'
import DataPipelines from './data/DataPipelines.svelte'
import DevServers from './dev_servers/DevServers.svelte'
import Docker from './docker/Docker.svelte'
import Pet from './fun/Pet.svelte'
import PetItem from './fun/PetItem.svelte'
import Quotes from './fun/Quotes.svelte'
import ShipIt from './fun/ShipIt.svelte'
import Sound from './sound/Sound.svelte'
import SoundItem from './sound/SoundItem.svelte'
import StatusItem from './status/StatusItem.svelte'
import StatusPages from './status/StatusPages.svelte'
import System from './system/System.svelte'
import SystemItem from './system/SystemItem.svelte'
import Tokens from './tokens/Tokens.svelte'
import Trail from './trail/Trail.svelte'

/** The component of each block of `BLOCK_DEFS`, for the window's grid. */
export const BLOCK_COMPONENTS: Record<string, BlockComponent<never>> = {
  trail: Trail as BlockComponent<never>,
  dev_servers: DevServers as BlockComponent<never>,
  docker: Docker as BlockComponent<never>,
  data: DataPipelines as BlockComponent<never>,
  bqcost: BigQueryCost as BlockComponent<never>,
  shipit: ShipIt as BlockComponent<never>,
  quotes: Quotes as BlockComponent<never>,
  pet: Pet as BlockComponent<never>,
  status: StatusPages as BlockComponent<never>,
  tokens: Tokens as BlockComponent<never>,
  system: System as BlockComponent<never>,
  sound: Sound as BlockComponent<never>,
  'status:status': StatusItem as unknown as BlockComponent<never>,
  'system:status': SystemItem as unknown as BlockComponent<never>,
  'sound:status': SoundItem as unknown as BlockComponent<never>,
  'pet:status': PetItem as unknown as BlockComponent<never>,
  ...EXTRA_COMPONENTS,
}
