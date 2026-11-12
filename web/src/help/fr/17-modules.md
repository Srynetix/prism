---
title: Écrire un module
group: Make it yours
related: settings, fun
---
Un **module** apporte les blocs d'une équipe à Prism sans le reconstruire : un dossier que Prism charge à l'exécution, avec une partie web (les blocs que la fenêtre affiche) et, s'il lit quelque chose, un back end (leur collecteur, que Prism exécute isolé). Ce guide en écrit un avec le SDK des modules ; `examples/modules/hello`, dans le dépôt de Prism, en est un complet.

## Ce qu'est un module

Un dossier de `modules/` à côté de `config.toml` (chaque profil le sien), nommé d'après l'identifiant du module :

```text
modules/hello/
  module.toml        ce qu'il est, ce qu'il peut atteindre
  web/defs.js        les déclarations et les mots de ses blocs
  web/components.js  leurs composants
  backend.wasm       son back end (facultatif)
```

Prism les liste dans **Réglages › Packs › Modules** : chargé, ou refusé avec la raison (un autre contrat de module, un autre Svelte, un Prism plus récent requis, un back end qui a arrêté Prism). Un module ajouté ou remplacé apparaît une fois la fenêtre rechargée (**Recharger la fenêtre**, au même endroit).

## Le manifeste

```toml
[module]
id = "hello"
version = "1.0.0"
contract = 1                  # le contrat de module pour lequel il est construit
requires = "0.12"             # le plus ancien Prism avec lequel il marche

[web]
dir = "web"

[wasm]
component = "backend.wasm"
tables = ["hello"]            # les seules tables de config qu'il reçoit
capabilities = { git = true } # git, gitlab, http = ["hôte"], exec = ["programme"]

[[wasm.blocks]]
id = "hello"
parts = ["hello_reads"]       # ses sources lues à part (l'une peut porter le nom du bloc)
repos = "one"                 # none, one ou many : [hello] repo / repos
reloads = true
provides = ["stages", "images"]
remote = { ping = "read" }    # ses commandes depuis un téléphone : read, act ou critical
```

Le back end n'atteint rien qu'il ne déclare pas : GitLab via glab (`gitlab`), git dans vos clones, en lecture seulement (`git` : les dépôts de ses blocs et les autres de `[[repos]]`), un GET vers les hôtes qu'il nomme (`http`), les programmes qu'il nomme (`exec`). À part ça, ni fichier, ni réseau, ni environnement.

## La partie web

Son projet dépend de `@prism/sdk`, de Svelte (la mineure de Prism : Prism en refuse une autre) et de Vite ; sa build tient en une ligne :

```ts
// vite.config.mts
import { moduleConfig } from '@prism/sdk/vite'
export default moduleConfig('hello', import.meta.dirname)
```

- `src/defs.ts` exporte `prism` et `svelte` (donnés par la build : `__PRISM_CONTRACT__`, `__SVELTE__`), `words` (son français) et `blocks()`, les déclarations de ses blocs : leur aide, leur élément de barre d'état, un guide de l'Aide (`guide`) et une page de Réglages pour leurs tables de config (`settings`) s'ils en ont. Il n'importe que `@prism/sdk/words`.
- `src/components.ts` exporte `components`, par identifiant de bloc (`hello:status` pour son élément de barre d'état). Ils n'atteignent Prism que par `@prism/sdk` : les mots, le snapshot, les commandes d'un bloc (`call`), la mise en page, les actions de Prism, les formats, ses icônes et ses contrôles.

Ce que Prism ne donne pas échoue à la build, pas dans Prism : un autre module de Svelte (Prism donne `svelte` et ses `animate`, `easing`, `motion`, `reactivity`, `store`, `transition`…), les sources de Prism, un fichier hors du dossier du module.

## Le back end

Un crate Rust construit pour WebAssembly, ses blocs écrits comme ceux de Prism : `impl Block`, avec les mêmes noms que le `blocks::sdk` de Prism.

```toml
# backend/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
prism-module = { git = "https://github.com/Srynetix/prism" }
```

```rust
// backend/src/lib.rs
pub mod blocks { pub use prism_module::sdk; }   // le bloc lit crate::blocks::sdk
mod hello;                                       // impl Block for Hello { … }

static BLOCKS: &[&dyn prism_module::Block] = &[&hello::HELLO];
prism_module::module!(BLOCKS);
```

Son collecteur est une boucle lancée dans `start`, comme dans Prism : `loop { lire; ctx.publish(&état); ctx.pause(délai).await }`. Chaque `pause` rend la main à Prism, qui reprend la boucle quand cette partie est de nouveau due ; git, GitLab et les programmes répondent tout de suite. Une panique, ou un appel de plus de deux minutes, arrête le back end : ses blocs le disent, et Prism le relance 30 s plus tard. Ses lignes `tracing` (`tracing::warn!`, avec `tracing` dans ses dépendances) vont au journal de Prism, comme celles d'un bloc de Prism.

Build : `rustup target add wasm32-wasip2` une fois, puis `cargo build --release --target wasm32-wasip2`. `vite build` copie le composant dans `dist/<id>/` avec `module.toml`.

## L'apporter avec un pack

Le pack d'une équipe installe ses modules : chaque `[[modules]]` en nomme un et l'endroit où le lire, avec les mêmes sources que le pack lui-même (la connexion glab ou gh de l'utilisateur).

```toml
[[modules]]
id = "hello"
source = "glab:groupe/prism-pack:modules/hello@main"   # un dossier d'un dépôt
# "gh:owner/repo:dist/hello.zip@v2" (une archive), ou un dossier ou un .zip à côté du pack
```

Importer ou mettre à jour le pack place chaque module dans `modules/<id>/`, en entier ; retirer le pack, ou un module du pack, retire le dossier. L'alerte de mise à jour des packs signale aussi qu'un de ses modules a changé à sa source.

## Lui faire confiance

Rien d'un module ne tourne avant que vous lui fassiez confiance : sa partie web n'est pas chargée, son back end ne démarre pas, ses blocs disent qu'ils attendent. **Réglages › Packs › Modules** liste ce que son back end peut atteindre (GitLab, git, les hôtes et programmes qu'il nomme) : **Faire confiance** (deux clics), et son back end démarre aussitôt, sa partie web au prochain chargement de la fenêtre. La confiance va à l'empreinte de son dossier : une nouvelle version attend de nouveau, avec ce qu'elle change (une capacité ajoutée, un bloc, une table de config).

## L'installer et l'essayer

Sans pack : copiez `dist/<id>` dans `modules/`, faites-lui confiance dans Réglages › Packs › Modules et rechargez la fenêtre, puis ajoutez ses blocs à une page (**+ Bloc**). Les lignes et les erreurs de son back end sont dans le journal de Prism (`module <id>: …`). Dans le profil de démo, ses blocs montrent l'état de `Block::demo`.
