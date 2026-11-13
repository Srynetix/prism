# Prism's words in other languages

The English text in the code is the key (`i18n.svelte.ts`):

```ts
t('Merge requests')
t('Move {ticket} to “{status}”', { ticket: f.ticket, status: f.status })
tn(n, '{n} review waiting', '{n} reviews waiting')   // each form its own key
tc('state', 'Open')                                     // key `state|Open`, falls back to `Open`
```

Each language has a folder (`fr/`) of dictionaries, one file per part of the app, each a default
export `{ 'English text': 'translation' } satisfies Dictionary`. A key the dictionary lacks shows
in English. A language's dictionaries are one chunk (`fr-all.ts`), read before the app's modules
(`loadLanguage`, in `main.ts` and `remote/main.ts`): a window in English never loads them, and a
word asked before that (a constant of a module the entry imports itself) stays in English, which
the dev build warns about once in the console. The help guides are translated whole: `help/<lang>/NN-id.md`, the same header with
`group` kept in English (Help shows it translated); a block's guide gives `guide.translations`.

A module brings its words in its `defs.js` (`words`, by language), and its back end's messages with
`Block::messages` (`sdk::tr!`); Prism's own translations win over theirs.

What stays in English: config keys and values, ids, what is compared in code (tracker statuses,
saved names), the prompts sent to agents (they follow `[agents] prompt_language`), logs, tests.

## French

- **Vous**, never tu. Actions in the infinitive (`Ouvrir`, `Relancer le pipeline`), states as
  adjectives or past participles (`Ouverte`, `Fusionnée`).
- Typography: `«  »` for quotes (with no-break spaces inside), a no-break space (U+00A0) before
  `:` `;` `?` `!` `»` and after `«`; `…` for the ellipsis; capital only at the start of a label.
- Units and numbers: `5 min`, `2 h`, `3 j` (jours), `Mo`, `Go`.

| English | Français |
| --- | --- |
| merge request, MR | merge request (f.), MR (f.) — « la MR » |
| pull request, PR | pull request (f.), PR (f.) |
| pipeline, job, stage | pipeline (m.), job (m.), stage (m.) |
| branch, commit, tag, rebase, merge | branche, commit, tag, rebase, merge (« rebaser », « merger ») |
| auto-merge, merge queue | auto-merge, file de merge |
| draft | brouillon |
| review, reviewer, approve | revue, relecteur, approuver |
| worktree | worktree (m.) |
| ticket, sprint, board | ticket, sprint, tableau |
| assignee, assign | assigné, assigner |
| estimate | estimation |
| agent, prompt, plan mode | agent, prompt, mode plan |
| permission (an agent asks) | autorisation |
| waits for you / your turn | vous attend / à vous |
| session | session |
| block, page, rail | bloc, page, rail |
| Settings | Réglages |
| Alerts, For me | Alertes, Pour moi |
| pod, cluster, namespace, deployment | pod, cluster, namespace, déploiement |
| release, deploy, rollout | release, déployer, déploiement |
| vulnerability | vulnérabilité |
| note, tag | note, étiquette |
| shell, terminal | shell, terminal |
| phone, remote | téléphone, accès à distance |
| pack, profile | pack, profil |
| theme, density | thème, densité |
| on / off | activé / désactivé |
| loading… | chargement… |
| retry | relancer |
| logs | logs |
