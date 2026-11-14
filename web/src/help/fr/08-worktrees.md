---
title: Worktrees
group: Everyday work
blocks: worktrees
related: agents, mrs, todo, settings
---
Voyez tous les worktrees de vos dépôts, à quoi ils servent et qui y travaille, et supprimez ceux dont vous n’avez plus besoin sans jamais perdre de travail.

## Ce que montre le bloc
![Le bloc Worktrees : en cours d’utilisation, obsolètes, et le détail de celui sélectionné](images/worktrees.webp)


Le bloc **Worktrees** (et l’onglet **Nettoyage** de la page Projets, qui peut remplacer une page Worktrees : voir [Projets et chantiers](prism:help/projects)) liste les worktrees des dépôts marqués **Worktrees** dans Réglages › Code › Dépôts (l’ancien `[worktrees] repo` en fait partie), chacun avec son dépôt quand il y en a plusieurs : ceux des agents de Prism, ceux de Claude Desktop (dans `.claude/worktrees/`) et ceux que vous avez créés à la main. L’en-tête les compte, indique combien sont utilisés, combien sont obsolètes, et l’espace disque qu’ils occupent.

Pour chaque worktree, vous voyez :

- sa branche et son upstream, et son retard et son avance sur sa base ;
- le ticket et la MR auxquels il est lié ;
- les agents Claude et les sessions Desktop qui y travaillent, sessions Desktop archivées comprises ;
- ses modifications non commitées, les commits poussés nulle part, sa taille et son dernier signe de vie.

Cliquez sur un worktree pour voir son détail : ce à quoi il est lié, le travail de Claude qu’il contient, et un **Verdict** qui dit s’il peut être supprimé sans risque. Les filtres **Tous**, **Utilisés** et **Obsolètes** restreignent la liste.

[Ouvrir le bloc Worktrees](prism:block/worktrees)

## Les quatre états
| État | Signification |
| --- | --- |
| **Utilisé** | Un agent y tourne, une session Desktop est active, ou il y a eu de l’activité récemment (24 heures par défaut). |
| **Obsolète** | Sa MR est mergée, sa branche n’existe plus sur origin, son ticket est terminé, sa session Desktop est archivée, ou rien ne s’y est passé depuis `[worktrees] stale_days` jours (14 par défaut). |
| **Dossier disparu** | Git connaît encore le worktree, mais son dossier a disparu. |
| **Inactif** | Aucun des cas ci-dessus : pas utilisé, rien n’indique qu’il est terminé. |

Un worktree obsolète affiche **sûr** quand il peut être supprimé sans perte, **à garder** quand il contient encore du travail. Le bloc Alertes signale les worktrees obsolètes qui peuvent être supprimés sans risque.

## Supprimer des worktrees
- **Supprimer** efface le dossier du worktree et sa branche locale. Comme tout bouton destructeur de Prism, il faut deux clics : le premier l’arme (**Confirmer**), le second supprime.
- **Supprimer quand même…** apparaît quand le worktree contient encore du travail (modifications non commitées, fichiers non suivis, commits poussés nulle part). Le Verdict dit ce qui serait perdu ; le second clic, **Confirmer : le perdre**, le supprime définitivement.
- **Nettoyer** oublie les worktrees dont le dossier a déjà disparu (`git worktree prune`).

Pour en supprimer plusieurs d’un coup :

1. Cliquez sur **Nettoyer…** dans l’en-tête du bloc. Il liste les worktrees obsolètes.
2. Ceux qui n’ont rien à perdre sont déjà cochés. Ceux qui perdraient du travail sont marqués *travail non sauvegardé* et ne sont jamais cochés par défaut : cochez-les un par un si vous voulez vraiment les supprimer.
3. Vérifiez l’estimation de l’espace libéré, puis cliquez sur **Supprimer**. Si vous avez coché des worktrees avec du travail non sauvegardé, cliquez à nouveau pour confirmer.

Ou choisissez-les vous-même, obsolètes ou non : cochez la case devant chaque worktree qui peut partir (pas le checkout principal, ni un worktree utilisé ou verrouillé) ; `⇧`-clic coche une plage. Une barre au-dessus de la liste les compte, avec l’espace qu’ils occupent et ceux qui ont du travail non sauvegardé ; sa case les coche tous. **Supprimer N**, puis **Confirmer**, les supprime l’un après l’autre : ceux que Prism refuse sont listés, les autres partent.

Les icônes en haut du détail ouvrent un worktree dans votre éditeur, un shell dans Prism, l’app de terminal ou le Finder (Réglages › Interface › Ouvrir avec) ; **Agent ici** y lance un agent Claude.

Ses actions git sont sur la ligne qu’elles changent :
- **Changer ▾**, sur la ligne Branche, le passe sur une autre branche : une locale, ou une d’origin (créée alors en local, qui la suit). Origin est relu à l’ouverture de la liste ; une branche où se trouve déjà un autre worktree apparaît grisée. Refusé avec des modifications non commitées, ou tant qu’un agent y tourne.
- **Pull**, sur la ligne Upstream, apparaît quand origin a des commits de sa branche qui lui manquent (↓N, au dernier fetch). En fast-forward seulement : si les deux ont divergé, rien n’est pullé.

**Archiver** et **Supprimer** sont sous le Verdict, qui dit si la suppression est sûre.

## Règles de sécurité
Prism ne supprime un worktree que si rien ne serait perdu :

- aucune modification non commitée et aucun fichier non suivi, en dehors du dossier `.claude/` que laisse Claude ;
- aucun commit absent du dépôt distant, sauf les commits antérieurs au merge de la MR, déjà livrés par un squash.

Certains worktrees ne sont jamais supprimés, pas même avec **Supprimer quand même…** :

- celui où tourne un agent Prism, ou qu’utilise une session Claude Desktop non archivée ;
- le dépôt principal ;
- un worktree verrouillé (déverrouillez-le d’abord avec `git worktree unlock`).

> [!WARNING]
> **Supprimer quand même…** efface définitivement les modifications non commitées et les commits non poussés. Lisez le Verdict avant le second clic.

> [!TIP]
> Le worktree d’un agent peut aussi être supprimé depuis la vue Agents, avec **Supprimer le worktree** une fois l’agent arrêté. La même règle s’applique : seulement si rien ne serait perdu.

## Réglages
Dans Réglages › Code › Dépôts › Worktrees (`[worktrees]`) :

- **Dépôt** (`repo`) : l’ancienne façon de nommer le clone du dépôt que désignent les références de MR sans dépôt, toujours lue ; les dépôts listés sont ceux dont **Worktrees** est activé.
- **Branche de base** (`base_branch`) de chaque dépôt qui ne nomme pas la sienne (les **Branches** de sa carte) : `auto` prend la dernière branche de release, sinon la principale ; ou un nom de branche. Le retard et l’avance sont comptés par rapport à elle.
- **Obsolète après** (`stale_days`) : jours sans commit ni session Claude avant qu’un worktree soit obsolète, 14 par défaut.
- **Relire toutes les** (`poll_seconds`).

Deux seuils liés se trouvent dans Réglages › Interface › Alertes et son : **Worktree utilisé** (`[alerts] worktree_in_use_hours`) et **Branche en retard** (`[alerts] behind_commits`), au-delà duquel une branche est signalée en retard sur sa base.

[Ouvrir Réglages › Code › Dépôts › Worktrees](prism:settings/worktrees)
