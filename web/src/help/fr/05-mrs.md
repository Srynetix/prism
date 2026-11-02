---
title: Merge requests et revue
group: Everyday work
blocks: mrs, queue
related: ci, agents, tickets, ref-keys
---
Suivez les merge requests de l’équipe, voyez d’un coup d’œil ce dont chacune a besoin ensuite, et relisez-les sans quitter Prism.

> [!NOTE]
> Sur un profil GitHub, les merge requests sont des pull requests : le bloc s’appelle **Pull requests**, et l’une d’elles s’écrit `#12` plutôt que `!12`, dans le filtre aussi.

> [!TIP]
> Un seul bloc pour les deux forges : un dépôt sur l’autre forge (Réglages › Dépôts, *Forge* : un projet personnel sur GitHub dans un profil GitLab, par exemple) a ses MR dans la même liste, sa puce marquée du logo de cette forge et ses numéros écrits à sa façon (`toolkit#14`). Elles sont lues avec l’outil et la connexion propres à cette forge (`gh` ou `glab`), si bien que *les miennes* et *à relire* suivent votre compte là-bas ; revue, approbation, merge, rebase et auto-merge fonctionnent comme sur les autres. Les pipelines de ses branches et la file de merge restent ceux de la forge du profil.

## Lire la liste
![Le bloc Merge requests : les pipelines des branches, les auteurs, puis une ligne par MR avec sa prochaine étape](images/mrs.webp){width=563}


Le bloc **Merge requests** a trois onglets : **Équipe**, **Les miennes** et **À relire**. Les lignes sont triées par dernière mise à jour, la plus récente en premier. Chaque MR tient sur deux lignes :

- d’abord, la pastille de l’auteur (sa photo de profil GitLab / GitHub, ou ses initiales sur une couleur qui lui est propre ; `[forge] avatars` désactivé : toujours les initiales), puis le titre complet, avec un bouclier rouge pour une MR de sécurité et une étiquette **Brouillon** ;
- en dessous, le contexte : branche → cible, auteur, ticket (un clic ouvre son aperçu), approbations, fils de discussion ouverts, ancienneté et taille ;
- dans une colonne fixe, le pipeline d’un coup d’œil : un segment par stage (vert réussi, orange en cours, rouge en échec, contour pour un stage manuel) et une courte étiquette comme `Verify 3/7 · 4m`.

Dans l’onglet **Équipe**, une rangée d’auteurs avec leur nombre de MR ouvertes permet de n’afficher que les MR d’une personne. Quand d’autres dépôts sont configurés avec leurs MR, une rangée **Dépôts** filtre par dépôt, et leurs MR sont nommées comme `helm!214`.

La taille d’une MR ouverte, ce sont ses lignes ajoutées et supprimées, `+48 −9`. Au-delà d’un seuil, elle ressort en orange : la MR est grosse et sera plus difficile à relire.

> [!TIP]
> Le seuil des grosses MR est de 400 lignes modifiées par défaut. Changez-le dans Réglages › Interface › Alertes et son › **Grosse MR** (`[alerts] large_mr_lines`).

[Ouvrir le bloc Merge requests](prism:block/mrs)

## La prochaine étape
Prism lit chaque MR et dit en clair ce qui la débloque, avec le bouton qui le fait. La couleur ne sert qu’à cela :

| Couleur | Signification | Étapes |
| --- | --- | --- |
| Rouge | bloquée | *Conflits*, *CI en échec* |
| Orange | à vous de jouer | *À relire*, *Rebase nécessaire*, *Fils ouverts* |
| Bleu | en attente de quelqu’un d’autre | *En attente de revue*, *En revue*, *CI en cours* |
| Vert | prête | *Prête à merger* |
| Violet | automatique | *Dans la file de merge*, *Merge après la CI*, *Merge en cours* |

Le bouton de la ligne est l’action qui débloque : Claude **Corriger** pour un pipeline en échec sur votre MR, Claude **Relire** quand quelqu’un attend votre revue, Claude **Résoudre** pour les conflits, **Rebase**, **Merger** (votre MR prête à merger, quand il n’y a pas de file de merge : un second clic confirme) ou **File de merge**, **Auto-merge** (votre MR approuvée, sans fil ouvert, dont la CI tourne encore : la forge la merge une fois au vert), ou **Logs**. Tout le reste est dans le menu ⋯, en toutes lettres : merger maintenant (toute MR que la forge peut merger), file de merge (GitLab), auto-merge, rebase, logs, relance, annulation et nouveau lancement du pipeline (GitLab), actions Claude, suivi du temps, copie de la branche, GitLab.

## Le panneau de la MR
![Le panneau de la MR : vue d’ensemble, description, pipeline](images/mr-drawer.webp)


Cliquez sur une MR (ou appuyez sur `Enter` sur sa ligne) pour ouvrir son panneau à droite. Il a trois onglets :

- **Vue d’ensemble** : le résumé (branche, auteur, assignés, relecteurs et approbations, ticket, fils, labels, dates), le pipeline avec ses stages et ses jobs en échec (*Relancer le job*, logs), la section de merge, et la description de la MR rendue en Markdown avec ses images.
- **Modifications** : le diff, pour la revue.
- **Discussion** : les fils et commentaires, avec *Répondre* et *Résoudre le fil* / *Rouvrir le fil* sur chacun, et un nouveau commentaire en bas (`⌘↵` envoie).

**Approuver** se trouve dans l’en-tête du panneau quand la forge vous permet d’approuver ; une fois approuvée, il affiche **✓ Approuvée** et, sur GitLab, un clic retire votre approbation (GitHub ne permet pas de la reprendre : rejetez votre revue sur GitHub). La ligne suit immédiatement. À côté, **Demander des modifications** ouvre un commentaire qui dit ce qui doit changer : `⌘↵` le publie et passe votre revue à *modifications demandées*, ce qui retire aussi votre approbation. L’en-tête affiche alors **⚠ Modifications demandées**, et parmi les *Relecteurs*, ceux qui ont demandé des modifications sont marqués ⚠. Quel que soit le demandeur, une MR avec des modifications demandées le dit partout : sa prochaine étape est **Modifications demandées** (à vous de jouer sur votre MR), sa ligne affiche *⚠ modifications demandées*, une notification vous prévient quand quelqu’un demande des modifications sur l’une des vôtres, et son ticket n’est jamais proposé « à merger », même approuvé. Sur GitLab, un clic dessus reprend votre demande ; GitHub ne le peut pas, rejetez votre revue là-bas. Depuis le panneau, vous pouvez aussi modifier les *Assignés* et les *Relecteurs*, utiliser *M’assigner*, et *Marquer comme brouillon* / *Marquer comme prête*. *Étiquettes* › **Modifier** (ou *Étiquettes…* dans le menu ⋯) coche ou décoche les étiquettes du projet ; un nom qu’il n’a pas encore est proposé en *Ajouter « … »*, et la forge crée l’étiquette. Seules les étiquettes que vous avez changées sont envoyées : une posée entre-temps ailleurs (le job de la file de merge, la page de la forge) reste.

Quand quelqu’un approuve l’une de vos MR, une notification dit qui, avec le ticket de la MR, et si la MR est désormais entièrement approuvée.

## Relire les modifications
![Modifications : les fichiers à gauche, le diff à droite, unifié ou côte à côte](images/mr-changes.webp)

1. Ouvrez la MR et allez dans **Modifications**. Appuyez sur `f` pour le plein écran si vous voulez plus de place.
2. Choisissez le rendu **Unifié** ou **Côte à côte** ; votre choix est retenu.
3. Lisez fichier par fichier. Quand le panneau est large, la liste des fichiers à gauche mène à un fichier ; **Tout déplier** / **Tout replier** agissent sur tous les fichiers. Au clavier (`Tab` pour donner le focus au panneau), `]` / `[` passent au fichier suivant ou précédent.
4. Cochez **Vu** sur un fichier (ou appuyez sur `v`) : il se replie et la progression s’affiche. La case se décoche d’elle-même si le fichier change ensuite.
5. Survolez une ligne et cliquez sur le **+** dans la marge pour la commenter. Sur plusieurs lignes : glissez d’un **+** le long des autres, ou Maj-cliquez sur le **+** d’une seconde ligne ; les lignes choisies s’éclairent, et le commentaire se place sous la dernière (sur GitLab comme sur GitHub, un fil sur ces lignes). Elles restent dans un même bloc de modifications : une forge refuse une plage sur deux. Les fils existants s’affichent sous leur ligne, avec réponse et résolution ; celui sur plusieurs lignes dit lesquelles.

Les gros fichiers (plus de 400 lignes) démarrent repliés. Dans une ligne modifiée, la partie qui a changé est surlignée.

## Les fichiers Markdown
Un fichier `.md` peut aussi se lire rendu : **Diff**, **Les deux** ou **Aperçu** dans son en-tête (**Les deux** par défaut, retenu). L’aperçu montre le fichier entier sur la branche source de la MR, à côté de son diff. Les blocs que la MR modifie ont une barre dans la marge, verte pour un ajout et orange pour une modification ; les lignes supprimées ont un repère rouge. Survolez un bloc pour éclairer les lignes correspondantes du diff (et inversement) ; cliquez pour y défiler.

## Merger, rebaser et Claude
- **Merger maintenant** merge la MR tout de suite quand la forge dit qu’elle le peut (pipeline, approbations, fils, ni brouillon ni conflit) : avec ses propres réglages de squash et de branche source sur GitLab, votre méthode de merge par défaut pour le dépôt sur GitHub. C’est irréversible : un second clic confirme.
- **File de merge** (GitLab) ajoute le label que surveille votre bot ou job de merge (`[gitlab] merge_queue_label`) : il rebase la MR, puis la met en auto-merge.
- **Auto-merge** laisse la forge merger dès que pipeline, approbations et fils le permettent, tout de suite si c’est déjà le cas. Un clic l’active ou le désactive. Sur GitHub, le dépôt doit autoriser l’auto-merge.
- **Rebase** demande à la forge de rebaser la branche sur sa cible.

La section **Claude** du menu ⋯ lance un agent sur la MR : **Corriger le pipeline** (quand la CI a échoué), **Revue**, **Appliquer la revue** pour appliquer les commentaires de revue, **Rebaser sur** sa branche cible pour rebaser et résoudre les conflits (aussi proposé comme **Rebaser en local** quand le rebase de GitLab lui-même a échoué : sa raison s’affiche sur la MR), **Valider en local** pour vérifier la MR sur sa branche (quand son ticket attend dans le premier statut de validation, il passe au dernier et vous est assigné, comme depuis le ticket), et un backport pour une MR qui ne cible pas `main`.

> [!NOTE]
> Un `⇧`-clic sur une action Claude la lance immédiatement, sans le lanceur. Les actions Claude sont celles des dépôts dont **Worktrees** est activé (Réglages › Code › Dépôts) : aucune sur la MR d’un autre dépôt.

[Lire : Travailler avec les agents Claude](prism:help/agents)

## Le bloc File de merge
**File de merge** (+ Bloc › Code et agents) montre ce qui attend d’être mergé dans un dépôt, choisi au-dessus de la liste (le choix est le même sur le téléphone) :
- **Dans la file**, pour un dépôt avec une file de merge (Réglages › Dépôts, *File*) : les MR qui portent son label, la plus ancienne en premier, dans l’ordre où son job de CI les prend. Avec des labels de priorité (`[gitlab] merge_priority_prefix = "merge-priority::"`, puis `merge-priority::0`, `merge-priority::1`… sur les MR), le plus petit nombre passe d’abord, puis les MR sans, la plus ancienne d’abord dans chaque groupe ; la ligne affiche **P0**, **P1**…, et le menu ⋯ d’une MR la règle (*Priorité de merge* : P0 à P3, — pour aucune ; l’autre label de priorité de la MR part). **▶** marque la suivante. Une MR retenue (conflits, rebase en échec, pipeline en échec) est comptée comme *bloquée* dans l’en-tête : le job ne la mergera pas en l’état.
- **Auto-merge** : les MR que la forge merge d’elle-même une fois au vert, hors de la file.
- **Approuvées, pas dans la file** (ou *auto-merge désactivé*, sans file) : des MR approuvées que rien ne mergera. Leur ligne propose **File de merge**, **Auto-merge** ou **Merger**.

Quand les MR visent plusieurs branches cibles, la liste est séparée par branche (la branche principale d’abord, puis celles de release) : chacune a son propre ordre et son propre **▶**.

En mode court, l’icône d’auto-merge sur une ligne indique que la forge mergera cette MR d’elle-même une fois au vert (déjà réglé par le job de la file, ou activé à la main).

Les brouillons et les MR encore en revue sont laissés au bloc Merge requests. Il ne lit rien par lui-même : les mêmes MR, les mêmes lignes et le même panneau.

## Sur le téléphone
L’onglet **MR** de l’accès à distance est fait pour le pouce. **Équipe** / **Les miennes** changent la liste ; une rangée défilante filtre par dépôt et par auteur ; les pipelines des branches protégées sont dans un bandeau. Ce qui **a besoin de vous** vient en premier — une MR qui attend votre revue, les vôtres bloquées ou prêtes pour leur prochaine étape — puis les autres, les plus récemment mises à jour en premier. Chaque ligne montre qui et son état, le titre sur deux lignes, son pipeline, son ticket, ses approbations et ses fils, et sa prochaine étape sous forme de bouton (**Corriger**, **Relire**, **Merger**… ; un merge demande un second appui).

Un appui ouvre la MR en plein écran, **‹ MR** revient. Les faits sont des tuiles, puis la description, les approbations et le pipeline. En bas, toujours à portée : l’**action principale du moment** (la prochaine étape de la ligne ; à défaut, **Approuver**) et **Actions**, une feuille avec tout le reste — approuver, demander des modifications, merger, rebaser, auto-merge, brouillon, les actions d’agent, saisir du temps, les liens. Une action d’agent démarre l’agent immédiatement sur le téléphone, et l’ouvre. **Modifications** montre un fichier à la fois, ses lignes renvoyées à la ligne : **‹ ›** ou la liste des fichiers passent de l’un à l’autre, **Vu** passe au fichier suivant pas encore vu. Dans **Discussion**, la zone de commentaire reste en bas.
