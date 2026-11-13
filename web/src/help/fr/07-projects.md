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
À gauche, les projets, groupés par dossier de travail (`[[workspaces]]`). Chacun montre ses signaux : un chantier qui vous attend, la pipeline de sa branche par défaut en échec, ses chantiers et ses MRs, le retard de son clone. Un projet sans rien en cours est grisé. L’entrée **Projets** du rail compte les chantiers qui vous attendent, et `⌘` avec le chiffre qui suit celui de la vue Agents l’ouvre.

## Un projet
- **L’en-tête** : sa forge, la branche de son clone et son retard, la pipeline de sa branche par défaut, ses MRs ouvertes, ses notes (un clic les ouvre, filtrées sur lui) et les ports où ses agents écoutent ; **Éditeur**, **Terminal**, le gestionnaire de fichiers et la forge.
- **Nouveau chantier** : décrivez le travail, ou choisissez un ticket (son **Implémenter**, qui prend le ticket) ; **Démarrer** crée un worktree (dans le clone quand le dépôt n’a pas de worktrees) et lance l’agent tout de suite, **Plus d’options…** ouvre le lanceur prérempli. Son prochain ticket et ses quick actions sont à côté.
- **Chantiers**, par étape : **En cours** (un agent y tourne ; celui qui vous attend est souligné), **En revue** (sa MR ouverte : pipeline, fils ouverts, approbations), **Terminé · à nettoyer** (sa MR mergée, sa branche disparue ou son ticket terminé). **Par activité** les liste tous, le plus récent d’abord.
- **À reprendre** : le dernier agent fermé dans le projet (**Rouvrir** ramène sa conversation), et les chantiers où rien ne tourne et aucune MR n’est ouverte : un worktree fait à la main, un agent arrêté (**Reprendre**, ou **Agent ici**).
- **Le dépôt** : son clone, **Tirer** (en avance rapide seulement), et un lien vers ses réglages.

## Nettoyer
Un chantier terminé montre le même verdict que le bloc Worktrees : **Rien à perdre**, ou ce qu’une suppression perdrait. **Supprimer le worktree** se fait en deux clics ; quand du travail serait perdu, le second dit lequel. **Archiver** le retire de la liste jusqu’à un nouveau commit ou changement.

L’onglet **Nettoyage**, en haut de la liste, est le bloc Worktrees sur tous les projets : tailles, verdicts, suppression et archivage en masse. La page Worktrees a quitté le rail pour lui (une page qui ne contient que le bloc Worktrees n’est plus affichée) ; le bloc Worktrees lui-même reste sur les pages qui l’utilisent.

[Ouvrir l’onglet Nettoyage](prism:projects/cleanup)

## Depuis ailleurs
- Le menu **Repos** de la barre d’état : **Ouvrir le projet** en tête de chaque dépôt.
- La vue Agents **par projet** : le bouton dossier à côté du **+** d’un projet ouvre sa page.
- Sur le téléphone, **Par projet** dans Agents : le nom d’un projet, ou le dépôt sur la carte d’un agent, ouvre sa page, ses chantiers en cartes et **Nouveau chantier** en bas. **Par statut** reste la vue par défaut.
