import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { moduleConfig } from '../build.mts'

export default moduleConfig('hello', dirname(fileURLToPath(import.meta.url)))
