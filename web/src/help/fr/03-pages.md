---
title: Pages, blocs et disposition
group: Start
related: welcome, settings, keyboard, ref-blocks
---
Prism affiche tout sous forme de blocs disposés sur des pages. Ce guide explique comment choisir vos pages, réorganiser leurs blocs, et utiliser les contrôles communs à toutes les pages : le filtre, Moi, le sélecteur de date et la barre d’état.

## Les pages du rail
Chaque icône du rail à gauche est une page ; `⌘1` à `⌘9` (ou `1` à `9`, et `gt` / `gT`) passent de l’une à l’autre. Les pages sont empilées dans l’ordre du rail : aller sur une autre fait glisser l’écran vers le haut ou le bas jusqu’à elle (instantanément quand les animations sont désactivées ou que macOS réduit les animations), et Agents, Réglages et Aide arrivent par la droite. Sur un trackpad, deux doigts vers le haut ou le bas au-dessus du rail passent à la page suivante ou précédente ; au-dessus de la page elle-même, ils ne font que défiler. Réglages › Interface › Apparence › **Balayer entre les pages** (`[appearance] page_swipe`, activé par défaut) désactive le geste. Sans aucun réglage, Prism affiche un seul **Dashboard** vide : remplissez-le avec **+ Bloc** dans Modifier la disposition, ou choisissez un modèle dans l’assistant de pages.

L’icône d’une page peut porter un badge : un point rouge quand un runner est hors service, le nombre de worktrees périmés, ou le nombre de pipelines de données en échec, quelle que soit la page qui héberge le bloc correspondant. L’icône **Agents** indique combien d’agents vous attendent.

## Choisir vos pages
![L’assistant de pages : le modèle Developer du pack de démarrage et les pages qu’il crée](images/wizard.webp)

L’assistant de pages s’ouvre de lui-même la première fois, quand aucune page n’est configurée. Ses modèles viennent de vos packs : le pack de votre équipe apporte ceux de ses rôles (Developer, Product, Data…). Sans aucun, l’assistant propose d’importer le **pack de démarrage de Prism**, qui apporte le modèle **Developer** : tickets, merge requests, worktrees et agents. C’est ensuite un pack comme un autre, dans **Réglages › Général › Packs** (ou `prism pack import prism:starter` depuis un terminal).

L’assistant procède en cinq étapes, chacune partant de ce que cette machine a déjà : **Forge** (glab ou gh installés, et en tant que qui ils sont connectés : *connecté en tant que …*, ou la commande de connexion à lancer ; ou pas de forge), **Agents** (les CLI d’agents trouvées dans le PATH, avec leur chemin et leur version, la première cochée étant celle par défaut ; un lien pour installer celle qui manque), **Dépôts** (les dépôts git de vos dossiers de travail et de `~/Development`, `~/src`, `~/code`…, ceux de la forge choisie cochés : écrits en `[[repos]]` avec leurs MR et leurs worktrees, et leur dossier comme dossier de travail quand vous n’en avez aucun), **Tickets** (ClickUp, Jira, Obsidian, ou pas de tracker : les tickets viennent alors des titres et des branches de vos merge requests) et **Votre page** (le modèle, et **Lire mon Calendrier macOS**, macOS uniquement). Rien n’est cherché avant que l’assistant s’ouvre, aucun jeton n’est lu, et seul *connecté en tant que …* arrive jusqu’à la fenêtre. **Vérifier à nouveau** regarde encore, après un `gh auth login` par exemple. **Passer** saute une étape : la config garde ce qu’elle en dit. Sous chaque étape, **Votre première page** montre les blocs au fur et à mesure ; la dernière montre les pages, sans les blocs dont la source manque (pas de forge connectée, pas de dépôt, pas de CLI d’agent ou de tmux, pas de tracker, pas de contexte kubectl), listés à part dans **Laissés de côté** : aucun bloc vide et en erreur le premier jour, **+ Bloc** en ajoute un une fois sa source réglée. **Commencer** écrit les pages dans `config.toml` avec les choix des étapes ; **Ouvrir la démo** ouvre le profil de démo dans son propre Prism, à côté de celui-ci, pour voir Prism rempli avant de brancher quoi que ce soit.

[Ouvrir l’assistant de pages](prism:wizard)

Vous pouvez aussi modifier les pages une à une dans **Réglages › Interface › Pages** (la table `[[views]]`) : un nom, une icône, une couleur, et les blocs de chaque colonne, écrits sous forme d’une chaîne par colonne, comme `todo sessions notes`. **Partir d’un modèle…** ouvre l’assistant de pages. Les pages s’appliquent dès l’enregistrement.

## Réorganiser une page
![Modifier la disposition : chaque bloc reçoit sa poignée et ses contrôles ; + Bloc, + Colonne, + Bande et Réinitialiser dans l’en-tête](images/edit-layout.webp)


1. Cliquez sur **Modifier la disposition** dans l’en-tête.
2. Faites glisser un bloc par sa poignée vers un autre emplacement ou une autre colonne.
3. Cliquez sur **+ Bloc** pour ajouter un bloc. Le menu les regroupe par sujet (Vue d’ensemble, Tickets, Code et agents, CI et livraison, Sécurité, Infra, Data, Fun) ; survoler un bloc montre son aide et un aperçu en direct ; un bloc peut apparaître sur plusieurs pages.
4. Cliquez sur **+ Colonne** pour ajouter une colonne (quatre au plus), ou sur **×** sur un bloc pour le retirer de cette page.
5. Cliquez sur **Terminé**, ou appuyez sur `Esc`. Les colonnes vides sont supprimées.

Les blocs sont écrits dans `config.toml` au fur et à mesure, dans les **Blocs par colonne** de la page (Réglages › Interface › Pages), avec l’Id de la page : renommer ou déplacer une page conserve donc sa disposition. **Réinitialiser** ramène la page à des tailles égales et des blocs normaux ; ses blocs restent. **Pages…** mène à Réglages › Interface › Pages. Colonnes et blocs se redimensionnent à la souris ; double-cliquez sur une poignée pour revenir aux proportions par défaut.

> [!TIP]
> Renommer une page dans Réglages › Interface › Pages conserve sa disposition tant que son **Id** reste le même.

## Les contrôles d’un bloc
Survolez le titre d’un bloc pour faire apparaître ses contrôles :

- **?** explique le bloc : à quoi il sert, ce que vous voyez, d’où viennent ses données, et **Lire le guide →** pour tout savoir.
- Trois boutons de taille : **Normal**, **Court** (quelques lignes : juste vos MR et revues, la réunion en cours et la suivante…) et **Replié** (le titre seul). Les blocs normaux se partagent la hauteur restante. Au clavier : `za` replie, `zs` passe en court.
- **Zoom** agrandit le bloc à toute la page (`zz`) ; `Esc` le restaure.

## Les colonnes des tableaux
Dans un tableau (Environnements, Worktrees, la semaine du Temps saisi), faites glisser le bord droit de l’en-tête d’une colonne pour l’élargir ou la rétrécir ; les autres colonnes gardent leur largeur. Double-cliquez sur le bord pour rendre au tableau ses largeurs automatiques. Chaque tableau retient ses largeurs, sur cet ordinateur.

## Filtre et Moi
Le filtre de l’en-tête (`/` pour y aller, `Esc` pour l’effacer) accepte un identifiant de ticket, un numéro de MR comme `!1957`, ou n’importe quel texte. Il suit les liens entre tickets, MR, branches et sessions, et s’applique à tous les blocs, Alertes comprises. Il reste tant que la fenêtre est ouverte.

**Moi** (`m`) ne garde que vos MR (en tant qu’auteur) et vos tickets (en tant qu’assigné), avec leurs pipelines et jobs. Les sessions Claude, les pipelines de `main` et `release/*`, et les alertes d’infra, de CI, de Claude et de revue restent visibles. Moi se combine avec le filtre.

## Le sélecteur de date
Le sélecteur ‹ jour › de l’en-tête choisit le jour que vous regardez : il pilote les saisies de temps, la section des MR mergées et l’agenda. ‹ et › avancent d’un jour ; un clic sur le jour ouvre un calendrier du mois (`←` `→` `↑` `↓` s’y déplacent, `Enter` choisit, `Esc` ferme). **Aujourd’hui** revient au jour courant. Le reste du tableau de bord montre toujours l’état actuel.

## La barre d’état
La ligne en bas de la fenêtre est commune à toutes les pages. Elle affiche, quand il y a quelque chose à dire : l’état de la connexion, le menu **Dépôts**, votre pipeline en cours ou en échec, les déploiements en cours, la prochaine réunion, les rappels dus, les sessions Claude en attente, l’usage de Claude, le jour du sprint, le lecteur audio et l’horloge (l’heure, pratique en plein écran ; la date au survol). La plupart des éléments ouvrent ce qu’ils décrivent en un clic.

Ses éléments sont groupés selon ce qu’ils vous demandent, entre de fins traits : à gauche ce qui vous attend (agents en attente, votre pipeline, la prochaine réunion, les rappels, le résumé), légèrement teinté, puis votre travail (dépôts, sprint, autres profils, shells) ; à droite l’ordinateur et Prism, **Problèmes** parmi eux ([Réglages et config.toml](prism:help/settings) dit ce qu’il liste). Quand la fenêtre est trop étroite, les éléments les moins utiles passent dans un **⋯** au bout de la barre ; ce qui vous attend, jamais.

> [!NOTE]
> Faites un clic droit sur la barre d’état pour choisir les éléments qu’elle affiche, ou **Personnaliser…** pour les ranger : glissez-les le long de la barre, le trait orange séparant la gauche de la droite, ou dans le tiroir au-dessus pour les masquer. L’ordre est enregistré dans `[appearance] statusbar` (Réglages › Interface › Apparence) ; **Ordre par défaut** revient aux zones.

## Sur le téléphone

L’accès depuis le téléphone montre un bloc à la fois, quel que soit le contenu de vos pages : chaque bloc de Prism, et ceux d’un pack, est une vue. Quatre sont des onglets en bas (`[remote] tabs`, par défaut home, mrs, tickets, agents) ; les autres sont sous **Plus**, par groupe comme dans **+ Bloc**. Il s’ouvre sur **Accueil** : les agents qui attendent une réponse, les MR qui ont besoin de vous, vos tickets en cours, la prochaine réunion et les alertes les plus graves, chacun ouvrant sa propre vue. Quelques vues rassemblent des blocs : **Tickets** (sprint et à prendre), **CI · Livraison** (santé de la CI et blocs de livraison), **Infra** (pods, environnements, vulnérabilités). Un bloc désactivé sur l’ordinateur est omis. Les filtres, tris et regroupements des blocs (un filtre de MR, le statut, la personne, le regroupement et le tri d’une liste de tickets, le filtre des agents…) sont les mêmes sur l’ordinateur et sur le téléphone : modifiés sur l’un, l’autre les affiche à la prochaine ouverture du bloc. Les onglets, replis et largeurs restent propres à chaque écran. Depuis un téléphone, il faut que ses actions soient activées (Réglages › Accès à distance).

[Lire : Les Réglages et config.toml](prism:help/settings)
