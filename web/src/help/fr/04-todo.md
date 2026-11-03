---
title: Alertes : ce qui a besoin de vous maintenant
group: Everyday work
blocks: todo
related: mrs, ci, agents, settings
---
Le bloc Alertes transforme tout ce que Prism lit en une seule liste de problèmes qui méritent d’agir, le plus grave en premier. L’en-tête, la barre des menus (ou la zone de notification) et les notifications portent la même liste quand vous regardez ailleurs.

## Ce qui arrive dans les Alertes
![Le bloc Alertes par urgence : ce pour quoi quelqu’un vous attend d’abord](images/alerts.webp){width=565}


Chaque ligne a une étiquette colorée qui nomme son type, puis une phrase qui dit ce qui ne va pas. Prism vérifie, entre autres :

- **Infra** : un runner hors service, des jobs en attente d’un runner depuis trop longtemps (avec le runner qui l’explique, s’il y en a un), aucun runner manager du tout.
- **CI** : un pipeline en échec sur `main` ou une branche `release/*`.
- **ArgoCD** : une synchronisation en échec, une application dégradée ou absente, ou désynchronisée depuis trop longtemps.
- **Data** : un pipeline de données GCP cassé ou qui dérive, quand Data est activé.
- **Sync** : une MR et son ticket qui ne concordent pas : MR mergée mais ticket pas avancé, MR ouverte mais ticket encore avant le développement, MR sorties de brouillon mais ticket encore en cours (il devrait être en revue), MR toutes approuvées mais ticket encore en revue (une MR avec des modifications demandées ne compte pas comme approuvée : son ticket reste en revue), ticket à merger mais sa MR pas dans la file de merge (`[tracker] merge_queue_statuses`), ticket fermé avec une MR ouverte, ticket en revue sans MR, ticket en cours sans branche ni MR depuis des jours.
- **Revue** : une MR qui attend votre revue depuis un moment.
- **MR** : vos propres MR en échec de CI, en conflit, prêtes mais à rebaser, ou sans identifiant de ticket.
- **Claude** : une session bloquée sur une demande d’autorisation, ou des sessions qui attendent votre réponse.
- **Worktrees** : des worktrees périmés qu’on peut supprimer sans risque, ou dont le dossier a disparu.
- **Source** : une source qui n’a pas pu être lue.

Un ticket dans un statut bloqué (`[clickup] blocked_statuses`) est exclu de toutes les règles Sync.

## Par urgence
Les lignes se rangent en trois bandes, selon ce qu’elles retiennent :

- **Maintenant** : quelqu’un vous attend, une personne ou un agent (une question, une permission, une review demandée), ou quelque chose est cassé.
- **Aujourd’hui** : votre propre travail est bloqué : pipeline de MR en échec, conflits, discussions non résolues, ticket désynchronisé.
- **Quand vous pouvez** : rien ne presse : un rebase à faire, un worktree périmé, le tour d’un agent terminé.

Chaque règle a sa bande, réglée dans **Réglages › Alertes** (`band_now`, `band_later`, `band_hidden`) : une règle nommée nulle part suit sa gravité. Une ligne Aujourd’hui qui dure plus de `escalate_hours` (24 par défaut) passe dans Maintenant. Son âge s’affiche à droite. L’étiquette garde sa couleur : **rouge** cassé, **orange** à traiter, **bleu** bon à savoir.

Les alertes d’une même MR ou d’un même ticket partagent une ligne (`group`) : son identifiant et son titre, chaque alerte en petite pastille, et les correctifs de celle à traiter d’abord — les conflits avant tout, les discussions avant la CI (les corriger pousse, la CI repart). Le chevron les déplie, chacune en entier.

En mode **Court**, le bloc garde les lignes Maintenant et Aujourd’hui, trois au plus, ou indique **Rien d’urgent.** Replié, son titre montre encore combien d’éléments restent.

L’élément **Ce qui vous attend** de la barre d’état montre un point et un compte par bande ; un clic ouvre la même liste depuis n’importe quelle page.

## Agir sur une ligne
Beaucoup de lignes portent des correctifs rapides, sous forme de petits boutons avant **Ouvrir** :

- **→ statut** passe le ticket au statut qu’il devrait avoir.
- **Rebase** demande à la forge (GitLab ou GitHub) de rebaser la MR ; **File de merge** la met dans la file de merge ; **Relancer** relance les jobs en échec ; **Logs** ouvre les logs du pipeline.
- Le numéro de la MR ouvre son panneau ; **Logs du pod** ouvre les logs d’une charge de travail en échec.
- Un bouton d’agent lance l’action d’agent correspondante, dans le lanceur (`⇧`-clic : immédiatement) : **Revue**, **Corriger le pipeline**, **Rebaser et corriger les conflits** sur une MR, dans son worktree ; **Corriger le pipeline** sur le pipeline en échec d’une branche protégée, dans un nouveau worktree ; **Enquêter** sur un pod en échec ou une application ArgoCD dégradée, en lecture seule sur le cluster. Pour corriger un pipeline, Prism lit les logs des jobs en échec et met leurs dernières lignes dans le prompt (`[agents] fix_log_lines`) : l’agent part de l’erreur.
- **Dire à l’agent**, sur un serveur de dev arrêté sur une erreur, envoie l’erreur à l’agent dont le shell l’a lancé ; quand cet agent est occupé, le message est seulement tapé dans son prompt, pour que vous l’envoyiez.
- Dès qu’un agent travaille sur la ligne (sur la branche de la MR, ou lancé par son bouton), le bouton cède la place à l’agent et à son état (**s’en occupe**, **demande une autorisation**, **prêt pour la revue**) : un clic l’ouvre.

Au clavier, allez sur la ligne avec `j` / `k` et appuyez sur `a` : cela lance son premier correctif rapide.

Un correctif qui a fonctionné affiche une coche. La ligne elle-même disparaît quand les données rattrapent, à la lecture suivante.

> [!TIP]
> Le filtre et **Moi** s’appliquent aussi aux Alertes : tapez un identifiant de ticket pour ne voir que les problèmes qui l’entourent.

## Marquer comme vu
Certaines lignes ne demandent aucune action de votre part. Pour en mettre une de côté :

1. Survolez la ligne et cliquez sur sa coche, ou allez-y avec `j` / `k` et appuyez sur `x`.
2. La ligne passe dans la section **Vues** en bas, repliée. Elle ne compte plus dans l’en-tête ni dans la barre des menus.
3. Pour la faire revenir, dépliez **Vues** et cliquez sur le bouton d’annulation, ou appuyez à nouveau sur `x`.

Une ligne vue revient d’elle-même quand son texte change (un nouveau compte, un nouvel état), et une marque expire au bout de sept jours : un problème récurrent ne reste jamais caché longtemps.

## Reporter
Une ligne que vous traiterez plus tard : son bouton lune (⏾) la reporte d’**1 heure**, à **cet après-midi**, au **lendemain matin**, **après ma réunion** (celle en cours ou la prochaine du jour, d’après le calendrier) ou **quand ça change**. Elle passe dans la section **Reportées**, repliée, avec l’heure de son retour ; le bouton d’annulation la fait revenir tout de suite. Reportée sur l’ordinateur, elle l’est aussi sur le téléphone, et inversement. **Bande de ce type d’alerte…** ouvre les réglages où se choisit la bande de sa règle.

## Pour moi et le voyant de CI
![Pour moi, ouvert depuis l’en-tête : une revue en attente, une session Claude en attente, une MR et son ticket désynchronisés](images/for-me.webp){width=520}


L’en-tête résume ce qui vous concerne, en deux groupes :

- **Pour moi** : les revues qui vous attendent, les sessions Claude qui vous attendent, et les lignes Sync non marquées comme vues. Cliquez dessus, ou appuyez sur `i`, pour voir chaque élément et l’ouvrir. Quand rien n’attend, il indique **Rien en attente**.
- **Santé de la CI** : un voyant qui reste vert tant que tout va bien. Il passe au rouge et pulse quand un pipeline a échoué sur `main` ou `release/*` (`main ✕` et un compte) ou quand un runner est hors service. À côté : les runners en bonne santé sur le total, et les jobs de CI actifs. Cliquez dessus pour les détails.

## Notifications et barre des menus
Prism continue de tourner dans la barre des menus quand vous fermez la fenêtre. Son compteur additionne les sessions Claude qui vous attendent et les lignes rouges et orange non marquées comme vues ; survolez-le pour les détails.

Une notification système est envoyée quand quelque chose de nouveau apparaît : un pipeline en échec sur une branche protégée, un runner hors service, un déploiement en échec, un pipeline de données cassé, une revue qui vous attend, ou Claude qui demande une réponse ou une autorisation. Les jobs en attente, ArgoCD désynchronisé et les données qui dérivent restent dans les Alertes sans vous interrompre, et un agent Prism qui termine son tour ne notifie pas. Au-delà de trois nouveaux éléments à la fois, vous recevez un seul récapitulatif.

## Récapitulatif, heures calmes et concentration
Par défaut, chaque nouvel élément notifie dès qu’il apparaît. Activez **Récapitulatif** (Réglages › Interface › Alertes et son › Notifications, `[alerts] digest`) et Prism les retient à la place, puis envoie un seul récapitulatif à chaque heure de **Récapitulatif à** (`digest_times`, 9:00, 12:30 et 17:30 par défaut) : ce qui a encore besoin de vous, et ce qui s’est résolu entre-temps (un pipeline redevenu vert). Les types dans **Toujours immédiatement** (`digest_urgent` : infra, déploiements et un agent qui demande une autorisation par défaut) continuent de notifier immédiatement.

Les **heures calmes** (`quiet_hours`, comme `19:00-08:00`) font taire tout, l’urgent compris, récapitulatif ou non. La **concentration** aussi : un événement de calendrier dont le titre contient **Événements de concentration** (`focus_events`, `Focus` par défaut), ou le mode zen. Un récapitulatif dû entre-temps attend leur fin ; en mode direct, ce qu’elles ont retenu part en un seul récapitulatif une fois terminées. Les rappels de réunion et vos propres rappels gardent leur heure.

La barre d’état indique ce qui est retenu et quand part le prochain récapitulatif, ou que les heures calmes ou la concentration sont actives. Cliquez dessus pour le dernier récapitulatif et ce qui attend le suivant ; **Envoyer maintenant** envoie le récapitulatif tout de suite, **Marquer comme lu** efface le dernier.

## Régler les seuils
Chaque règle fondée sur le temps a son seuil dans **Réglages › Interface › Alertes et son**, la section `[alerts]`, appliqué dès l’enregistrement :

| Réglage | Clé | Par défaut |
|---|---|---|
| Job en attente | `pending_job_minutes` | 5 minutes |
| Revue en attente | `review_wait_hours` | 4 heures |
| Ticket inactif | `doing_idle_days` | 3 jours |
| Session en attente | `waiting_session_hours` | 2 heures |
| ArgoCD désynchronisé | `argo_out_of_sync_minutes` | 15 minutes |

La même section contient les seuils des badges et des données, comme `large_mr_lines`. Les statuts de ticket sur lesquels s’appuient les règles Sync sont dans **Réglages › Travail › Tracker › ClickUp**.

[Ouvrir Réglages › Interface › Alertes et son](prism:settings/alerts)

## Sur le téléphone
L’onglet **Alertes** de l’accès à distance les montre par bande — **Maintenant**, **Aujourd’hui**, **Quand vous pouvez** — chacune en entier, son type et son âge au-dessus. Ses correctifs (une action d’agent, un statut, un rebase…) et **Ouvrir** sont des boutons sous le texte ; **⏾** affiche dessous les moments où la reporter, **✓** la marque comme vue, comme sur l’ordinateur : les vues et les reportées se replient en bas, un appui les réaffiche.
