---
title: Projets et chantiers
group: Everyday work
blocks: worktrees
related: agents, worktrees, mrs, tickets
---
Où en est mon travail ? La page **Projets** montre chaque dépôt comme un projet, et dans chacun tout travail en cours comme un chantier : son worktree, ses agents, son ticket, sa MR et sa pipeline, sur une seule carte.

## Deux mots
- Un **projet**, c’est un dépôt que Prism suit (`[[repos]]`) : son clone, sa forge, ses réglages.
- Un **chantier**, c’est une branche de travail d’un projet : son worktree (ou le clone lui-même), les agents qui y travaillent, son ticket et sa MR. Un agent lancé dans le clone sur `main` forme un chantier « sur main », visible tant qu’il a un agent ou des changements locaux.

« Worktree » reste le mot git, dans les détails techniques et les Réglages. La vue Agents répond à « qui m’attend ? » ; la page Projets à « où en est mon travail ? ».

[Ouvrir la page Projets](prism:projects)

## La liste des projets
À gauche, les projets, groupés par dossier de travail (`[[workspaces]]`). Chacun montre ses signaux : un chantier qui vous attend, la pipeline de sa branche par défaut en échec, ses chantiers et ses MRs, le retard de son clone. Un projet sans rien en cours est grisé. Le filtre du bandeau (`/`) resserre la liste : un projet reste quand son nom, une de ses branches, un de ses tickets ou une de ses MR (`!12`) contient ce qui est tapé, et le bandeau dit combien ; les chantiers du projet ouvert ne sont pas filtrés. L’entrée **Projets** du rail compte les chantiers qui vous attendent, et `⌘` avec le chiffre qui suit celui de la vue Agents l’ouvre.

## Un projet
- **L’en-tête** : sa forge, la branche de son clone et son retard, la pipeline de sa branche par défaut, ses MRs ouvertes, ses notes (un clic les ouvre, filtrées sur lui) et les ports où ses agents écoutent ; **Shell** (un shell dans Prism, dans son clone : tmux le garde une fois le panneau fermé), **Éditeur**, **Terminal**, le gestionnaire de fichiers et la forge.
- **Nouveau chantier** : décrivez le travail, ou choisissez un ticket (son **Implémenter**, qui prend le ticket) ; **Démarrer** crée un worktree (dans le clone quand le dépôt n’a pas de worktrees) et lance l’agent tout de suite, **Plus d’options…** ouvre le lanceur prérempli. Son prochain ticket et ses quick actions sont à côté.
- **Chantiers**, par étape : **En cours** (un agent y tourne ; celui qui vous attend est souligné), **En revue** (sa MR ouverte : pipeline, fils ouverts, approbations), **Terminé · à nettoyer** (sa MR mergée, sa branche disparue ou son ticket terminé). **Par activité** les liste tous, le plus récent d’abord. **Shell**, sur un chantier dans un worktree ou le dossier d’un agent, y ouvre un shell dans Prism (celui du clone est dans l’en-tête) : les mêmes shells que ceux du bloc Worktrees.
- **À reprendre** : le dernier agent fermé dans le projet (**Rouvrir** ramène sa conversation), et les chantiers où rien ne tourne et aucune MR n’est ouverte : un worktree fait à la main, un agent arrêté (**Reprendre**, ou **Agent ici**).
- **Le dépôt** : son clone, **Tirer** (en avance rapide seulement), et un lien vers ses réglages.

## Branches
Sous les chantiers, **Branches** liste les branches locales du clone, les plus récentes d’abord : dernier commit, worktree, MR, ↑↓ par rapport à sa branche d’origin, +/− par rapport à la base, et un verdict. **Sûre** (en vert) : terminée, sans rien à perdre : sa MR mergée (en squash aussi), disparue d’origin ou déjà dans la base, sans commit absent de toute branche distante ni changement dans son worktree. Une branche protégée, celle du clone ou celle où travaille un agent n’est jamais supprimée.

Sur l’ordinateur, chaque branche propose **Worktree** (un worktree à côté du clone, sans agent), **Basculer le clone**, **Agent ici** dans son worktree, et **Supprimer** au second clic, son worktree avec elle. Ce qui perdrait des commits ou des changements ne part que par **Forcer**, qui dit ce qui est perdu. Rien n’est supprimé sur la forge.

**Faire le ménage** relit origin et l’élague (`fetch --prune`, `worktree prune`), puis liste les branches sûres, cochées : **Supprimer** les retire, chacune revérifiée, et dit ce qui a été supprimé, ce qui a été gardé et pourquoi. Sur le téléphone, les branches sont en lecture, et **Faire le ménage** demande la passkey. L’onglet Nettoyage compte les branches sûres de chaque projet, avec son propre **Faire le ménage**.

## Nettoyer
Un chantier terminé montre le même verdict que le bloc Worktrees : **Rien à perdre**, ou ce qu’une suppression perdrait. **Supprimer le worktree** se fait en deux clics ; quand du travail serait perdu, le second dit lequel. **Archiver** le retire de la liste jusqu’à un nouveau commit ou changement.

L’onglet **Nettoyage**, en haut de la liste, est le bloc Worktrees sur tous les projets : tailles, verdicts, suppression et archivage en masse. Il remplace une page qui ne contient que le bloc Worktrees : une telle page peut partir (**Modifier la disposition**) ; le bloc Worktrees lui-même reste sur les pages qui l’utilisent.

[Ouvrir l’onglet Nettoyage](prism:projects/cleanup)

## Depuis ailleurs
- Le menu **Repos** de la barre d’état : **Ouvrir le projet** en tête de chaque dépôt.
- La vue Agents **par projet** : le bouton dossier à côté du **+** d’un projet ouvre sa page.
- Sur le téléphone, **Par projet** dans Agents : le nom d’un projet, ou le dépôt sur la carte d’un agent, ouvre sa page, ses chantiers en cartes et **Nouveau chantier** en bas. **Par statut** reste la vue par défaut.
