import { t } from '@prism/sdk/words'

/** Given by `@prism/sdk/vite` as it builds the module. */
declare const __SVELTE__: string
declare const __PRISM_CONTRACT__: number

/** The module contract it is built for, and the Svelte it is compiled with (Prism checks both). */
export const prism = __PRISM_CONTRACT__
export const svelte = __SVELTE__

/** Its words in Prism's other languages, under Prism's own. */
export const words = {
  fr: {
    Hello: 'Bonjour',
    'Hello from a module': 'Bonjour depuis un module',
    'A block loaded at run time, from modules/hello.': 'Un bloc chargé à l’exécution, depuis modules/hello.',
    'It reads the snapshot as Prism does, and keeps its own time.': 'Il lit le snapshot comme Prism, et tient sa propre heure.',
    'Fail fails on purpose: the block says so, with Retry, and the rest of the window goes on.': 'Échouer échoue exprès : le bloc le dit, avec Relancer, et le reste de la fenêtre continue.',
    'Built with the module SDK (@prism/sdk, prism-module).': 'Construit avec le SDK des modules (@prism/sdk, prism-module).',
    '{n} agents, {m} sources failing': '{n} agents, {m} sources en échec',
    'It is {time}.': 'Il est {time}.',
    Fail: 'Échouer',
    'This block failed on purpose.': 'Ce bloc a échoué exprès.',
    'Its back end read {n} times, {when}.': 'Son back end a lu {n} fois, {when}.',
    'Last commit of {project}: {commit}': 'Dernier commit de {project} : {commit}',
    'No back end: build it with just module-example.': 'Pas de back end : construis-le avec just module-example.',
    'Pong: {n} reads': 'Pong : {n} lectures',
    'Crash the back end': 'Planter le back end',
    'Its back end, a WebAssembly component, reads its repository’s last commit: Crash the back end panics it on purpose, and Prism starts it again.': 'Son back end, un composant WebAssembly, lit le dernier commit de son dépôt : Planter le back end le fait paniquer exprès, et Prism le relance.',
  },
}

/** Its blocks: a function, called once its words are in Prism's dictionaries. */
export const blocks = () => [
  {
    id: 'hello',
    label: t('Hello'),
    category: 'Fun',
    help: {
      purpose: t('A block loaded at run time, from modules/hello.'),
      points: [t('It reads the snapshot as Prism does, and keeps its own time.'), t('Fail fails on purpose: the block says so, with Retry, and the rest of the window goes on.'), t('Its back end, a WebAssembly component, reads its repository’s last commit: Crash the back end panics it on purpose, and Prism starts it again.')],
      source: t('Built with the module SDK (@prism/sdk, prism-module).'),
    },
    status: { label: t('Hello'), side: 'right' as const },
  },
]
