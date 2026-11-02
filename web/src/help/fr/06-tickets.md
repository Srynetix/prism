---
title: Sprint, refinement et tickets
group: Everyday work
blocks: sprint, retro, wip, refinement, pickup
related: mrs, agents, time, todo
---
Suivez les tickets du sprint avec leurs merge requests (pull requests sur GitHub), préparez les suivants et prenez du travail, avec les modifications de votre tracker faites sur place et Claude à un clic. Le tracker est ClickUp par défaut, Jira ou des tableaux Kanban Obsidian sinon (**Réglages › Travail › Tracker › Fournisseur**) ; ce qui suit décrit ClickUp, puis ce qui change avec les deux autres.

## Le bloc Sprint
![Le bloc Sprint, groupé par statut](images/sprint.webp){width=563}


Le bloc **Sprint** liste les tickets du sprint en cours. Sous le titre, une barre montre l’avancement du sprint et son jour. Les onglets **En cours** et **Suivant** passent au sprint suivant : ses dates, dans combien de jours il commence, et tous ses tickets.

Chaque ticket tient sur deux lignes : le titre complet, puis l’identifiant, les assignés, le parent et l’ancienneté (orange au-delà de 14 jours sans mise à jour). À droite : la MR du ticket (numéro et pipeline d’un coup d’œil, un clic ouvre son panneau), le statut, la prochaine étape et son action.

- Le **drapeau de priorité** ouvre le titre : *Urgente* en rouge, *Haute* en orange, un simple drapeau pour Normale et Basse, vide sans priorité. Cliquez dessus pour changer la priorité dans ClickUp.
- Le **statut** est une liste déroulante avec les statuts de la liste du ticket.
- Un clic sur les **assignés** vous assigne le ticket, ou vous retire s’il est déjà à vous ; les autres assignés ne changent pas.
- **Estimations** : un clic sur l’estimation d’un ticket (sur sa ligne, ou dans son aperçu) la transforme en champ : tapez les heures (`3.5`), Entrée enregistre, Échap annule, un champ vide l’efface. Par défaut, l’estimation propre au tracker (le temps estimé de ClickUp, l’estimation d’origine de Jira ; **Saisir l’estimation**, `[tracker] estimate_builtin`). Une équipe qui estime dans des champs personnalisés numériques les liste dans **Champs d’estimation** (`[clickup] estimate_fields`) et désactive celle intégrée : ces champs s’affichent sur chaque ticket, même vides, pour être remplis. Un champ formule est affiché, jamais écrit.
- **Champs personnalisés** : ceux nommés dans **Champs des tickets** (`[clickup] fields`, Réglages › Travail › Tracker › ClickUp, par ex. `["Type"]`) s’affichent sous le statut, une pastille par valeur dans la couleur de son option ClickUp ; l’aperçu les liste aussi. La barre d’outils filtre sur leurs valeurs (comme les priorités : une puce par valeur avec son compte, **Aucune** pour les tickets sans), et **Grouper par** propose chacun d’eux : un groupe par valeur, dans l’ordre des options de ClickUp, les tickets sans valeur après. **Champs de filtre** (`[clickup] filter_fields`) limite ces filtres et regroupements aux champs listés (une liste déroulante comme Type) ; les autres sont seulement affichés. Chaque champ peut trier la liste : **Trier par** un champ, les nombres comme des nombres et une liste déroulante dans l’ordre de ClickUp, ↑ ou ↓, les tickets sans valeur en dernier ; puis par priorité, comme par défaut. Seuls les champs avec une valeur, dans l’ordre donné ; un champ liste déroulante, labels, texte, nombre, date, case à cocher, personnes ou note se lit.

Les tickets sont groupés par statut (les terminés dans un groupe *terminé* replié), par assigné, ou pas du tout. La barre d’outils filtre **Ouverts** / **Non assignés** / **Terminés** / **Tous** et par priorité (**Terminés**, une fois un ticket fini : ceux-là seulement, leur groupe ouvert) ; `m` ne garde que vos tickets. **Personne** montre les tickets d’une personne : vous, n’importe quel assigné du sprint, ou les non assignés, avec leurs tickets ouverts comptés (pas avec Obsidian, qui n’a pas d’assignés) ; retenu, chaque onglet le sien. Les sous-tâches sont indentées sous leur parent. L’interrupteur **Hors sprint** ajoute les tickets en cours qui ne sont pas dans le sprint, dans un groupe à part, avec leur tableau et *Ajouter au sprint* dans leur menu.

[Ouvrir le bloc Sprint](prism:block/sprint)

## Mon sprint
Le bloc **Mon sprint** est le sprint en cours vu de votre côté, pour préparer la rétro. En haut : les tickets qui vous sont assignés et terminés, la part de votre temps sur les tickets du sprint, votre plus longue plage de concentration (médiane des jours), les revues que vous avez faites, et les agents que vous avez lancés avec leur coût.

En dessous : où est passé votre temps (tickets du sprint, réunions, autres tickets, sans ticket) et, jour par jour, votre plus longue plage sans réunion et combien de fois vous avez basculé entre tickets et MR. Cela vient des traces du **Fil de la journée** : une estimation, pas une feuille de temps ; avec le Fil de la journée désactivé, ces chiffres restent vides. Puis vos tickets : livrés, ou reportés et pourquoi (bloqués, jamais commencés, CI rouge), l’estimation du tracker face au temps saisi ou tracé, et le cycle d’en cours à terminé tel que Prism a vu les changements. **Vu par Prism** liste ce qui ressort, comparé uniquement à votre propre sprint précédent ; l’équipe tient en une ligne, sans classement.

**Pour la rétro** contient vos points pour la rétro du sprint, à part de vos notes : choisissez à garder (▲), à changer (▼) ou une idée (✦), tapez le point et appuyez sur `Enter`. Ils sont conservés par sprint, dans le dossier de données de Prism (`retro.json`) : une fois le sprint suivant commencé, le sélecteur de sprint dans l’en-tête de la cellule fait revenir les points de celui qui est passé en revue. Un clic sur la marque d’un point change son type, un double-clic sur son texte le modifie, × le supprime. Les notes étiquetées `#retro` pendant le sprint reviennent en dessous, sous *De mes notes*. **Copier en Markdown** et **Rédiger mes points** incluent les points du sprint en cours, par type.

**Copier en Markdown** copie le tout pour la rétro de l’équipe ; **Rédiger mes points** demande à un agent deux ou trois points à garder et à changer, à partir de ces chiffres (il ne modifie aucun fichier). Les chiffres de chaque sprint sont conservés sur cette machine, pour y comparer le suivant.

## La prochaine étape d’un ticket
Comme une MR, chaque ticket dit en clair ce qui le fait avancer, avec le bouton qui le fait :

- *À prendre* : personne ne l’a encore, le bouton est **M’assigner**.
- *Démarrer* ou *Pas encore de MR* : il est à vous et n’a pas de MR, le bouton est **Implémenter** (Claude).
- Quand le ticket a une MR ouverte, l’étape et l’action propres à la MR : *CI en échec* → Corriger, *À relire* → Relire…
- *!… mergée, avancer* : la MR est mergée, le bouton passe le ticket au statut suivant.

Tout le reste est dans le menu ⋯ (ou `.` sur la ligne) : statut, priorité, m’assigner, saisir du temps, les actions Claude, filtrer le tableau de bord, ouvrir dans ClickUp.

## Le parcours d’un ticket
Un ticket passe par sept étapes : **Affiné**, **Dév**, **MR**, **Revue**, **Mergé**, **Recette**, **Terminé**. Prism le situe à partir de ce qu’il lit déjà : le statut du tracker, les agents liés au ticket, les MR qui le nomment (brouillon, CI, approbations, threads, merge). Une étape en échec — CI rouge, conflits, rebase raté — passe en rouge.

En haut de l’aperçu du ticket, une carte dit où il en est et pourquoi (*2 threads non résolus*, *CI en échec : test*, *Sam attend votre revue*), avec le seul bouton qui le fait avancer : **Review-fix** sur votre MR qui a des retours, **Corriger** sur une CI rouge, **Revue** quand on vous demande une revue, **Valider en local** une fois mergé, **Implémenter** avant toute MR, et les actions propres à la MR (Rebase, Merge, Auto-merge…).

Sous ses champs, la section **Parcours** dessine les sept étapes, puis un journal par étape, daté quand Prism connaît la date : *Affinage* (créé, estimé, dans quel sprint), *Développement* (chaque agent avec sa CLI, sa durée et son worktree ; les MR ouvertes et leur CI), *Revue* (relecteurs, threads, changements demandés, approbations) et *Livraison* (le merge, la file de merge, les environnements déployés depuis le merge, la recette, terminé). Les étapes à venir sont en pointillé ; une étape sans rien à montrer est omise. Un environnement compte comme déployé quand ses workloads ont changé après le merge, comme dans le fil : une heure, pas la preuve du commit. Rien n’est stocké pour cela : le journal est reconstruit à partir de ce que Prism lit. La section est repliée, son étape actuelle à côté de son titre ; son chevron l’ouvre, et elle reste ouverte pour tous les tickets, comme **Réel vs estimé**. La carte de prochaine action en haut peut être masquée : `[tracker] next_action`, dans Réglages › Travail › Tracker.

## Le bloc En cours
**En cours** (+ Bloc › Tickets) est votre travail en cours, une ligne par ticket : les tickets du sprint qui vous sont assignés et pas terminés, ceux avec une MR ouverte ou un agent à vous qui tourne, et les MR qui demandent votre revue. Chaque ligne a les sept étapes en petites barres, pourquoi il en est là, votre rôle (*auteur*, *relecteur*, *assigné*) et le même bouton unique que l’aperçu du ticket. Un clic sur son id ou son titre ouvre le ticket (ou la MR, sans ticket que Prism lit).

L’ordre vide la journée par le haut : d’abord ce qui attend après vous (une revue demandée, des retours sur votre MR, la question d’un agent), puis ce qui a échoué, puis le cours normal, puis ce qui n’a pas commencé ; dans chaque groupe, ce qui attend depuis le plus longtemps en premier. **À moi d’agir** ne montre que ce sur quoi vous pouvez agir, **J’attends** ce qui attend quelqu’un d’autre (une revue, la CI, la file de merge, un agent au travail) avec depuis combien de temps, **Tout** les deux ; le choix est gardé, et partagé avec le téléphone, où le bouton de chaque ligne prend toute la largeur. En mode court, seulement ce qui est à vous, une ligne chacun.

## Les tickets bloqués
L’aperçu liste les **Commits** du ticket, lus dans vos clones locaux tels qu’à leur dernier fetch, du plus récent au plus ancien : ceux dont le message le nomme (`ABC-123`, pas `ABC-1234`), sur n’importe quelle branche, et ceux des branches nommées d’après lui (`feat/abc-123-…`) pas encore mergées. Un point plein : dans la branche principale ou une branche de release ; un point creux : non mergé, avec sa branche. Le hash court ouvre le commit sur la forge ; avec plusieurs dépôts, chaque commit nomme le sien. Rien n’est lu sur la forge, et un dépôt sans clone n’en a aucun.

Un ticket qui attend d’autres tickets dans ClickUp (ses dépendances, **Waiting on**) les montre sur une troisième ligne : l’identifiant, le statut et le nom de chaque bloqueur, les terminés barrés. Tant que l’un n’est pas terminé, la ligne indique **attend**, en rouge, et la ligne est teintée ; une fois qu’ils sont tous terminés, elle indique **débloqué**, en vert : il est temps de commencer. Un ticket que d’autres attendent indique **bloque …** sur sa deuxième ligne : mieux vaut le finir d’abord. Cliquez sur un bloqueur pour le prévisualiser, même depuis une autre liste. Trop chargé ? Cliquez sur **attend** pour replier la ligne de chaque ticket en une puce sur sa deuxième ligne (`attend 2`, les bloqueurs dans son infobulle) ; cliquez sur la puce pour les déplier. Les en-têtes de Sprint et À prendre comptent les tickets bloqués, et l’aperçu liste les deux sens sous **Attend** et **Bloque**. Les dépendances se règlent dans ClickUp ; Jira et Obsidian n’en ont pas ici.

## Le fil d’un travail
Un ticket, sa branche, sa MR, son pipeline, ses agents et ses déploiements forment un seul travail. Appuyez sur `t` sur une ligne (un ticket, une MR, un agent, un worktree, l’agent d’un serveur de dev), ou choisissez **Fil** dans son menu ⋯, et un panneau montre ce fil du ticket jusqu’à la production : **Ticket**, **Worktree**, **Agent**, **MR**, **Pipeline**, puis chaque environnement. Cliquez sur un lien pour l’ouvrir (l’aperçu du ticket, le panneau de la MR, les logs du pipeline, l’agent, le worktree dans votre éditeur).

Prism le construit à partir de ce qu’il relie déjà : l’identifiant du ticket dans un nom de branche, la branche d’une MR, le ticket, la branche ou la MR liée d’un agent, la branche d’un worktree. Un lien qui devrait déjà exister est en pointillés orange : un ticket en cours sans worktree, branche ni agent ; une branche avec des commits et sans MR ; une MR mergée il y a des jours et dans le premier environnement, mais pas dans le suivant. Un environnement compte comme **mis à jour** quand ses charges de travail ont changé après le merge : Prism compare les heures, pas les commits. En dessous viennent les alertes qui le concernent (le statut du ticket en décalage avec sa MR, un pipeline rouge), puis ses chiffres : le temps saisi, le coût des agents, les réunions qui nomment le ticket, ce qu’il attend et ce qu’il bloque.

Sur chaque ligne de ticket, le même fil est replié en petits carrés à côté de son identifiant (worktree, agent, MR, pipeline, environnements) : verts quand ils sont là, en pointillés orange quand ils manquent, rouges quand ils sont cassés. Un ticket qu’aucun travail n’a encore touché n’en a aucun. Cliquez dessus pour ouvrir le panneau. Ils sont désactivés par défaut : Réglages › Interface › Apparence › **Fils des tickets** (`[appearance] ticket_threads`) les affiche.

## L’aperçu du ticket
![L’aperçu du ticket au-dessus de la page](images/ticket.webp)

Cliquez sur une ligne de ticket, une carte de Refinement ou le ticket d’une MR : son aperçu s’ouvre dans un panneau latéral, avec la description, les checklists, les sous-tâches et les commentaires, lus depuis ClickUp à l’ouverture. Les images s’affichent dans le texte et les pièces jointes dans une section **Pièces jointes**. Sous ses champs, les actions d’agent de son menu ⋯ sous forme de boutons (**Affiner**, **Implémenter**, **Valider en local** quand c’est pertinent, **Ouvrir un agent**) : un clic ouvre le lanceur au-dessus du panneau, un `⇧`-clic lance immédiatement.

1. Changez le statut avec la liste déroulante ; le tableau de bord suit aussitôt.
2. Cliquez sur un ticket cité ou une sous-tâche pour l’ouvrir dans le même panneau ; ← revient en arrière.
3. Ajoutez un commentaire avec *Commenter* (`⌘↵` envoie), ou **Modifiez** l’un des vôtres.
4. `Esc` ferme le panneau. Un `⌘`-clic sur un ticket l’ouvre plutôt dans ClickUp.

> [!NOTE]
> Modifier un commentaire remplace son texte dans ClickUp : la mise en forme riche d’origine (mentions, gras) est perdue. Prism refuse de modifier le commentaire de quelqu’un d’autre.

## Refinement et À prendre
![La page Tickets : Refinement à gauche, À prendre à droite](images/tickets.webp)


Le bloc **Refinement** est la file de préparation : les tickets dans les statuts de refinement, issus des tableaux utilisés pour le refinement (Réglages › Travail › Tracker › ClickUp › Tableaux) : les listes trimestrielles d’un dossier, ou la liste d’un tableau. Il est groupé par EPIC : une ligne par EPIC avec l’avancement de ses tâches, sa charge estimée, ses signaux (non découpé, non assigné, en sommeil) et la prochaine action, qui se déplie en ses tâches. En haut, jusqu’à trois choses à faire : un EPIC à découper, le ticket endormi depuis le plus longtemps, un EPIC à qui il ne manque que des estimations pour être prêt. Triez par statut (l’ordre du flux) ou par dernière mise à jour, et filtrez par trimestre, dossier et statut : avec des statuts choisis, un EPIC se déplie seulement en ses tâches dans ces statuts, son avancement les comptant toujours toutes.

Le bloc **À prendre** liste les tickets prêts à être pris (`ready to dev`, `todo`) : les tableaux où l’on prend du travail (Réglages › Travail › Tracker › ClickUp › Tableaux) et le sprint suivant, groupés par tableau dans leur couleur. Les lignes sont les mêmes que dans le sprint, avec **M’assigner**. *Ajouter au sprint* dans le menu ⋯ ajoute le ticket au sprint en cours après un second clic, en plus de sa propre liste.

> [!WARNING]
> *Ajouter au sprint* ne vérifie pas que le ticket est spécifié et estimé. Assurez-vous qu’il est prêt avant de l’ajouter.

[Ouvrir le bloc À prendre](prism:block/pickup)

## Avec Jira
Avec Jira Cloud comme tracker, le bloc **Sprint** montre le sprint actif du tableau indiqué dans **Réglages › Travail › Tracker › Jira** (un tableau Scrum), ou ses tickets ouverts (un tableau Kanban), les colonnes du tableau ordonnant les statuts ; l’onglet **Suivant** montre le sprint suivant. Les tickets du tableau en cours hors du sprint actif s’y ajoutent aussi (**Statuts en cours**, la catégorie « In Progress » de Jira par défaut). **Refinement** et **À prendre** lisent les **Files** du tableau, chacune une requête JQL utilisée pour la prise, le refinement ou la synchronisation, comme les tableaux de ClickUp ; À prendre ajoute le sprint suivant.

Un changement de statut passe par une transition du workflow du ticket : il doit en exister une qui mène à ce statut. La priorité va à la première priorité Jira qui se lit comme urgente, haute, normale ou basse. Les commentaires sont ajoutés ou modifiés en texte brut ; un ticket vous est assigné, ajouté au sprint actif, ou créé dans le projet du tableau. Le temps est saisi sous forme de worklogs Jira (voir [Suivi du temps](prism:help/time)).

## Avec Obsidian
Sur des tableaux Kanban Obsidian, le bloc **Sprint** montre les cartes ouvertes de tous les tableaux, groupées par tableau, et **À prendre** celles d’une colonne des statuts Prêt (**Réglages › Travail › Tracker**) : déplacer une carte dans une telle colonne l’y envoie. Le sélecteur de statut propose les colonnes de son tableau ; un changement déplace la carte dans le fichier, et le `status` de sa note prend la valeur qu’ont les notes de cette colonne. La priorité va dans le `priority` de la note (`p0` à `p3`) ou l’étiquette `#p0`-`#p3` de la carte ; un commentaire sous le `## Comments` de la note. Le **+** du bloc Sprint crée une carte, et sa note quand les cartes du tableau ont des notes. Prism relit les fichiers dès qu’ils changent.

## Les actions Claude sur les tickets
La section **Claude** du menu ⋯ d’un ticket confie le travail à un agent :

- **Affiner** demande à Claude d’affiner le ticket dans le dépôt (la compétence de refinement de votre équipe si elle en a une), en lecture seule : faisabilité, solution technique, estimation. Dans Refinement, l’action s’appelle **Estimer** pour un ticket à estimer, et **Découper** pour un EPIC pas encore découpé.
- **Implémenter** demande à Claude d’implémenter le ticket (la compétence de votre équipe si elle en a une) dans le worktree du ticket, ou dans un nouveau worktree nommé d’après le ticket.
- **Valider en local** apparaît sur un ticket en *validation* ou *à valider* : un agent valide le ticket par rapport à ce qu’il demande, pas ses MR, qui sont en principe mergées (le prompt les cite avec leur branche cible). Une MR encore ouverte est nommée pour que l’agent valide sur sa branche ; quand c’est la seule, l’agent travaille sur cette branche, sinon dans un nouveau worktree nommé d’après le ticket. Depuis *à valider*, une fois l’agent lancé, le ticket passe en *validation* et vous est assigné.
- **Ouvrir un agent** ouvre le lanceur nommé d’après le ticket, sans prompt, dans son worktree s’il en a un.

Chaque action ouvre le lanceur d’agent, prérempli ; un `⇧`-clic lance immédiatement.

[Lire : Travailler avec les agents Claude](prism:help/agents)
