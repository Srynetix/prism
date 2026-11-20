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
3. Cliquez sur **+ Bloc** pour ajouter un bloc. Le menu les regroupe par sujet (Vue d’ensemble, Tickets, Code et agents, CI et livraison, Sécurité, Infra, Data, Sur mesure, Fun) ; survoler un bloc montre son aide et un aperçu en direct ; un bloc peut apparaître sur plusieurs pages.
4. Cliquez sur **+ Colonne** pour ajouter une colonne (quatre au plus), ou sur **×** sur un bloc pour le retirer de cette page.
5. Cliquez sur **Terminé**, ou appuyez sur `Esc`. Les colonnes vides sont supprimées.

Les blocs sont écrits dans `config.toml` au fur et à mesure, dans les **Blocs par colonne** de la page (Réglages › Interface › Pages), avec l’Id de la page : renommer ou déplacer une page conserve donc sa disposition. **Réinitialiser** ramène la page à des tailles égales et des blocs normaux ; ses blocs restent. **Pages…** mène à Réglages › Interface › Pages, la table entière. Colonnes et blocs se redimensionnent à la souris ; double-cliquez sur une poignée pour revenir aux proportions par défaut.

> [!TIP]
> Renommer une page dans Réglages › Interface › Pages conserve sa disposition tant que son **Id** reste le même.

## Gérer les pages depuis le rail
Un clic droit sur une page du rail (ou son **⋯** en mode disposition) ouvre son menu :

- **Renommer** (`F2` pour la page affichée) : le nom se tape sur place, `Entrée` le garde, `Échap` le laisse comme avant.
- **Icône et couleur…** : une grille d’icônes et de couleurs, visibles dans le rail et l’en-tête à mesure ; `Entrée`, **Terminé** ou un clic ailleurs les gardent, `Échap` remet les précédentes.
- **Dupliquer** : une copie juste après, avec ses tailles et l’état de ses blocs.
- **Monter** et **Descendre** ; ou faites glisser la page le long du rail.
- **Modifier la disposition**, sur cette page.
- **Masquer** : la page quitte le rail sans être supprimée. **Pages masquées**, en bas du rail, les liste ; un clic en remet une.
- **Supprimer** (deux clics) : **Annuler**, dans le message qui suit, la remet pendant quelques secondes. La dernière page ne peut pas être supprimée.

En mode disposition, **+ Page** sous les pages du rail en ajoute une après la page affichée : une page vide, une copie de celle-ci, ou une page des modèles de vos packs. Elle s’ouvre en mode disposition, son nom prêt à être tapé. Dans l’en-tête aussi, l’icône et le nom de la page se changent sur place.

Tout s’écrit dans `[[views]]` de `config.toml`, comme le fait Réglages › Interface › Pages ; une page renommée garde son **Id**, et avec lui sa disposition.

### Les pages d’un pack
Un pack peut apporter des pages. Elles portent la marque **pack** en mode disposition et dans leur menu, et restent à jour avec le pack : dans `config.toml`, une telle page est une ligne avec son seul `id` (et `hidden` quand elle est masquée). La renommer, changer son icône, sa couleur ou ses blocs l’écrit en entier : elle ne suit plus le pack, et Prism le dit, avec **Annuler**. Son menu propose alors **Remettre au modèle du pack**. Une page de pack masquée reste à jour.

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
La ligne en bas de la fenêtre est commune à toutes les pages. Par défaut, elle est équilibrée en trois zones, chaque élément affiché quand il a quelque chose à dire : à gauche ce qui vous attend — les agents en attente (un clic ouvre la vue Agents sur le premier), votre pipeline en cours ou en échec (en échec, les logs du job qui échoue ; en cours, sa MR), la prochaine réunion —, légèrement teinté ; puis votre travail, le menu **Dépôts** et le jour du sprint ; à droite l’ordinateur : l’usage du forfait, **Prism** et l’horloge (l’heure, pratique en plein écran ; la date au survol). La plupart des éléments ouvrent ce qu’ils décrivent en un clic.

**Prism** tient en un point et quelques mots pour Prism lui-même : vert quand tout va bien, orange pour un avertissement ou une mise à jour à faire, rouge pour une erreur. Un clic liste ce qu’il y a : un nouveau Prism (Réglages › Général › Mises à jour), des packs ou modules modifiés à leur source (Réglages › Général › Packs), et ses problèmes ([Réglages et config.toml](prism:help/settings) dit ce qu’ils sont).

Tous les autres éléments sont masqués tant que vous ne les affichez pas : les rappels, le résumé, les routines à relire, Prism MCP, Ce qui vous attend, les autres profils et ordinateurs, les shells ouverts, Au passage, le lecteur audio, les retours, le point de connexion, et ceux des blocs (le Pet, CPU et RAM, Son, incidents de service, blocs sur mesure). Quand la fenêtre est trop étroite, les éléments les moins utiles passent dans un **⋯** au bout de la barre ; ce qui vous attend, jamais.

> [!NOTE]
> Faites un clic droit sur la barre d’état pour choisir les éléments qu’elle affiche, ou **Personnaliser…** pour les ranger : glissez-les le long de la barre, le trait orange séparant la gauche de la droite, ou dans le tiroir au-dessus pour les masquer. L’ordre est enregistré dans `[appearance] statusbar`, les éléments masqués dans `statusbar_hidden` (Réglages › Interface › Apparence) ; **Ordre par défaut** revient à la barre par défaut. Une config qui règle déjà l’un des deux garde sa barre telle qu’écrite ; les anciens `update`, `packs` et `problems` se lisent comme **Prism**.

## Sur une moitié d’écran
Prism tient sur une moitié d’écran, à côté d’un éditeur ou d’un navigateur : la fenêtre descend à 640 × 480 px. Sous 1 200 px de large, il s’organise pour la place qu’il a plutôt que de se tasser :

- le rail ne montre que ses icônes, leur nom dans leur infobulle ;
- les blocs d’une page passent les uns sous les autres, la page défile ;
- un en-tête ou une barre garde ses actions principales sur une ligne et range les autres dans son **⋯**, mesuré au fur et à mesure : élargissez la fenêtre et elles reviennent. La barre d’état fait de même, ses éléments les moins utiles d’abord ;
- la vue Agents montre un agent à la fois, son terminal ou son Chat sur toute la largeur : sa liste est un tiroir (le bouton au début de son en-tête, ou `⌘B`), refermé dès qu’un agent est choisi ; son panneau latéral s’ouvre par-dessus le terminal (**Terminal** ou **Chat** y revient), ses onglets en sélecteur dans le **⋯** quand ils ne tiennent plus ; les agents côte à côte attendent une fenêtre plus large.

Sous 720 px, les tiroirs et les fenêtres de dialogue prennent toute la largeur. Un bloc suit sa propre largeur plutôt que celle de la fenêtre : étroit dans une page large, il se comporte comme sur une moitié d’écran. Le zoom de la fenêtre (`⌘+` / `⌘-`) change aussi la place : à 125 %, une fenêtre de 1 440 px se dispose comme une de 1 150 px.

## Les blocs sur mesure
Un bloc à vous, sans Rust ni Svelte : une commande, ou un script écrit dans `config.toml`, qui affiche du JSON. Déclarez-le dans **Réglages › Interface › Blocs sur mesure** (ou `[[custom_blocks]]`) : **Commande** lance un programme avec ses arguments tels quels, jamais par un shell ; **Script** contient le programme lui-même, dans l’éditeur de l’onglet Fichiers, lancé par son shebang (bash, python3, node, `uv run --script` avec ses dépendances ; `/bin/sh` sans shebang), avec des modèles de départ qui affichent déjà un JSON valide. **Lancer maintenant** lance ce que vous éditez, enregistré ou non, et montre sa sortie ; **Écrire avec un agent** ouvre le lanceur avec le contrat et votre description, et **Charger le script de l’agent** amène ce qu’il a écrit dans l’éditeur.

Sa sortie est un objet JSON dont le `kind` est `list` (titre, sous-titre, état `ok`/`warn`/`ko`, badge, lien), `gauge` ou `stats` (des chiffres avec seuils `warn`/`ko`, et une sparkline de leurs dernières valeurs), `markdown`, ou `table` (un clic sur une colonne la trie) ; tout le reste s’affiche tel quel. Ses `alerts` vont dans Alertes, une par `key`, et son `status` dans la barre d’état ; un échec, un timeout ou un JSON hors contrat s’affiche dans **Problèmes** avec la fin de sa sortie d’erreur. Chaque bloc est un bloc de page à part entière, `custom:<id>`, sous **Sur mesure** dans **+ Bloc** ; sur le téléphone, ils sont tous dans la vue **Blocs sur mesure** (sauf ceux en `remote = false`).

Les actions (`[[custom_blocks.actions]]`) mettent des boutons sur les lignes concernées (`when = "state != 'ok'"`) : une commande ou un script lancé avec la ligne dans `PRISM_ITEM`, le bloc relu juste après, ou un agent ouvert dans le lanceur avec la ligne comme donnée. Elles viennent de la config uniquement : la sortie d’un script ne dit jamais quoi lancer.

> [!WARNING]
> Un bloc sur mesure tourne avec vos droits, à intervalle régulier, sans que personne ne regarde. Enregistré depuis les Réglages, il a votre confiance ; ajouté ou modifié ailleurs (un éditeur, un agent, une mise à jour de pack), il l’attend : le bloc montre ce qui a changé et d’où ça vient, et **Faire confiance et lancer** (deux clics, sur l’ordinateur uniquement) le laisse tourner. Le bloc d’un pack garde sa version de confiance en attendant ; un téléphone peut supprimer un bloc en attente, jamais lui faire confiance. Les secrets passent par référence (`env = { TOKEN = "keychain:<service>" }`), jamais en clair.

## Sur le téléphone

L’accès depuis le téléphone montre un bloc à la fois, quel que soit le contenu de vos pages : chaque bloc de Prism, et ceux d’un pack, est une vue. Quatre sont des onglets en bas (`[remote] tabs`, par défaut home, mrs, tickets, agents) ; les autres sont sous **Plus**, par groupe comme dans **+ Bloc**. Il s’ouvre sur **Accueil** : les agents qui attendent une réponse, les MR qui ont besoin de vous, vos tickets en cours, la prochaine réunion et les alertes les plus graves, chacun ouvrant sa propre vue. Quelques vues rassemblent des blocs : **Tickets** (sprint et à prendre), **CI · Livraison** (santé de la CI et blocs de livraison), **Infra** (pods, environnements, vulnérabilités). Un bloc désactivé sur l’ordinateur est omis. Les filtres, tris et regroupements des blocs (un filtre de MR, le statut, la personne, le regroupement et le tri d’une liste de tickets, le filtre des agents…) sont les mêmes sur l’ordinateur et sur le téléphone : modifiés sur l’un, l’autre les affiche à la prochaine ouverture du bloc. Les onglets, replis et largeurs restent propres à chaque écran. Depuis un téléphone, il faut que ses actions soient activées (Réglages › Accès à distance).

[Lire : Les Réglages et config.toml](prism:help/settings)
