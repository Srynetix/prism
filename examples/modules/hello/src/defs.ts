import { t } from '@prism/sdk/words'

declare const __SVELTE__: string

/** The module contract it is built for, and the Svelte it is compiled with (Prism checks both). */
export const prism = 1
export const svelte = __SVELTE__

/** Its words in Prism's other languages, under Prism's own. */
export const words = {
  fr: {
    Hello: 'Bonjour',
    'Hello from a module': 'Bonjour depuis un module',
    'A block loaded at run time, from modules/hello.': 'Un bloc chargé à l’exécution, depuis modules/hello.',
    'It reads the snapshot as Prism does, and keeps its own time.': 'Il lit le snapshot comme Prism, et tient sa propre heure.',
    'Fail fails on purpose: the block says so, with Retry, and the rest of the window goes on.': 'Échouer échoue exprès : le bloc le dit, avec Relancer, et le reste de la fenêtre continue.',
    'Built with examples/modules/build.mts.': 'Construit avec examples/modules/build.mts.',
    '{n} agents, {m} sources failing': '{n} agents, {m} sources en échec',
    'It is {time}.': 'Il est {time}.',
    Fail: 'Échouer',
    'This block failed on purpose.': 'Ce bloc a échoué exprès.',
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
      points: [t('It reads the snapshot as Prism does, and keeps its own time.'), t('Fail fails on purpose: the block says so, with Retry, and the rest of the window goes on.')],
      source: t('Built with examples/modules/build.mts.'),
    },
    status: { label: t('Hello'), side: 'right' as const },
  },
]
