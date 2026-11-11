import Hello from './Hello.svelte'
import HelloItem from './HelloItem.svelte'

/** Its components, by block id; `<id>:status` its status-bar item. */
export const components = { hello: Hello, 'hello:status': HelloItem }
