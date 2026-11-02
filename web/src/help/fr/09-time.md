---
title: Suivi du temps et daily
group: Everyday work
blocks: trail
related: tickets, notes, mrs, ref-settings
---
Saisissez votre temps dans ClickUp (ou en worklogs Jira, Réglages › Travail › Tracker › Jira › Suivi du temps) sans quitter Prism, voyez d’un coup d’œil si votre journée et votre semaine sont complètes, et laissez la Trace du jour vous dire ce que vous avez fait.

## La pastille de temps dans l’en-tête
L’en-tête affiche une pastille avec le logo ClickUp et le temps saisi le jour affiché par rapport à l’objectif, par exemple `6h00/7h today`. Une fine barre en dessous se remplit à mesure que vous saisissez ; elle passe au vert quand la journée atteint son objectif.

L’objectif est de 7 heures par jour ouvré et 35 heures par semaine. Il est fixé dans l’application, ce n’est pas un réglage.

Un clic sur la pastille ouvre le tiroir **Suivi du temps**. Son en-tête reprend le total du jour et ajoute celui de la semaine (`week 21h00/35h`). Un clic en dehors du tiroir le ferme, et le ticket et la description en cours de saisie sont gardés pour la prochaine fois.

## Saisir du temps
![Le panneau de temps de l’en-tête : une nouvelle saisie, la semaine, les saisies du jour](images/time.webp){width=402}


Le tiroir a trois champs : **Ticket**, **Durée** et **Description** (facultative).

1. Ouvrez le tiroir depuis la pastille de l’en-tête.
2. Tapez un identifiant de ticket, ou choisissez-en un dans les suggestions : les tickets de votre sprint, ceux de vos MR ouvertes, ceux de vos sessions Claude et les tâches des tableaux de temps.
3. Tapez une durée : `1h30`, `1h`, `45m`, `45` (minutes) ou `1.5` (heures) fonctionnent tous.
4. Ajoutez une description si vous le souhaitez, puis appuyez sur `Enter` ou cliquez sur **Saisir le temps**. Le formulaire se vide alors, ticket compris, prêt pour la saisie suivante.

La saisie part dans ClickUp le jour affiché dans l’en-tête, juste après la dernière saisie de ce jour, ou à 9:00 si le jour est encore vide. La ligne au-dessus du bouton indique où elle tombe, par exemple `Tue 7 Oct, from 14:30`.

> [!TIP]
> Vous avez rarement besoin de taper le ticket. **Saisir le temps** dans le menu ⋯ d’une MR ou d’un ticket (Sprint, À prendre, Affinage), ou dans l’en-tête du panneau d’un ticket, ouvre le tiroir pré-rempli ; et quand le filtre global contient un identifiant de ticket, le tiroir démarre avec lui.

## Tâches récurrentes
Réunions, support, rituels : le temps qui n’appartient à aucun ticket du sprint va sur des tâches récurrentes, lues dans les tableaux utilisés pour le temps (Réglages › Travail › Tracker › ClickUp › Tableaux, `uses = ["time"]`).

Elles se trouvent dans une section repliée du tiroir, au nom du tableau, avec leur nombre. Dépliez-la pour trouver :

- les cinq utilisées le plus récemment, en raccourcis en haut ;
- un champ de filtre, puis la liste complète, chaque sous-tâche sous sa tâche parente ;
- un clic sur un nom choisit la tâche et place le curseur dans **Durée** ; la flèche l’ouvre dans ClickUp ; le survol montre sa description.

Ces tâches ne sont pas listées dans le bloc Sprint, seulement ici.

## La semaine, et modifier les saisies
Sous le formulaire, le tiroir montre la semaine du jour affiché, du lundi au dimanche, chaque jour avec une petite barre et son total. Un jour qui a atteint 7 heures est vert ; un jour ouvré passé qui ne les a pas atteintes est orange, pour qu’un après-midi oublié saute aux yeux. Un clic sur un jour le sélectionne dans toute l’application. Les jours futurs ne peuvent pas être sélectionnés.

Sous la semaine viennent les saisies du jour sélectionné. Pour en modifier une :

1. Cliquez sur son crayon, ou double-cliquez sur la ligne.
2. Modifiez l’heure de début, sur 24 h (`09:30`, `9:30` ou `930` ; le jour lui-même ne change pas), le ticket, la description ou la durée.
3. Appuyez sur `Enter` pour enregistrer dans ClickUp, ou `Esc` pour annuler.

Laisser le ticket vide conserve la tâche de la saisie. Pour supprimer une saisie, cliquez sur son ✕, puis à nouveau sur **supprimer ?** dans les secondes qui suivent. Seules les saisies affichées par Prism peuvent être modifiées ou supprimées.

> [!NOTE]
> Saisir, modifier et supprimer ne fonctionnent que dans la fenêtre de l’application Prism, pas dans un aperçu navigateur.

## Vues Semaine et Mois
Le sélecteur en haut du tiroir, **Jour · Semaine · Mois**, change ce qu’il affiche ; Prism retient votre choix. **Jour** est le formulaire et les saisies du jour décrits plus haut. Le tiroir s’élargit pour les deux autres.

- **Semaine** : une feuille de temps de la semaine du jour affiché, une ligne par ticket (ou tâche) et une colonne par jour, avec le total de chaque ligne et, en bas, le total de chaque jour coloré comme la bande de la semaine.
- **Mois** : un calendrier du mois, chaque jour avec son total et une petite barre, les totaux de semaine à droite, puis le temps par ticket sur le mois, du plus saisi au moins saisi.

Les deux ont ‹ › pour passer à la semaine ou au mois précédent ou suivant, aussi loin que l’historique lu, et affichent le total de la période par rapport à son objectif (jours ouvrés × objectif quotidien), avec l’objectif à date pour la période en cours. Les jours ouvrés passés qui n’ont pas atteint leur objectif sont listés en orange : un clic sur l’un d’eux, ou sur n’importe quel jour, ouvre la vue **Jour** sur lui, prête à combler le manque.

## Le sélecteur de date
Le sélecteur ‹ jour › de l’en-tête, avec son bouton **Aujourd’hui**, fixe le jour que vous regardez. Il pilote le tiroir de temps, la Trace du jour, la section des MR mergées, l’agenda et les notes du jour. Tout le reste montre l’état actuel.

Prism relit `[clickup] time_history_days` jours de temps saisi (30 par défaut), et au moins jusqu’au 1er du mois précédent pour que la vue Mois montre le mois dernier en entier : vous pouvez remonter jusque-là pour combler un manque.

## La Trace du jour
![La chronologie de la Trace du jour : les traces dans l’ordre, les plages sans trace signalées](images/trail-timeline.webp)

Le bloc **Trace du jour** reconstitue le jour choisi dans l’en-tête à partir de ses traces : vos commits dans les clones locaux, vos MR ouvertes, mergées, approuvées et commentées, vos tickets qui changent de statut, vos agents, vos réunions une fois terminées, et le temps que vous avez saisi. Rien à noter : Prism tient lui-même le journal, sur `[trail] keep_days` jours (30 par défaut), sur cette machine. Ajoutez le bloc à une page avec Modifier la mise en page, puis + Bloc (Vue d’ensemble).

Ses trois onglets :

- **Chronologie** : les traces dans l’ordre, les semblables regroupées (trois commits sur une branche, deux commentaires sur une MR). Un clic ouvre la MR ou le ticket. Une plage de la journée de travail (9:00 à 18:30) sans aucune trace pendant `[trail] gap_minutes` (60 par défaut) est signalée : souvent du temps travaillé et non saisi.
- **Par ticket** : pour chaque ticket, le temps que Prism estime à partir de ses traces face au temps déjà saisi. Chaque trace compte le temps depuis la précédente, 45 minutes au plus ; un agent compte sa durée d’exécution ; les réunions sont à part. **Saisir** ajoute la différence au ticket, à partir de sa première trace ; **Tout saisir** le fait pour chaque ticket. C’est une estimation : rien n’est saisi sans votre clic.
- **Daily** : votre part du standup. *Hier* est le jour ouvré précédent : MR mergées et ouvertes, tickets travaillés (commits, agents), MR relues. *Aujourd’hui* : vos MR à faire passer au vert, à faire relire ou à merger, les revues qui vous attendent, vos tickets en cours, les réunions du jour. *Blocages* : vos tickets qui attendent d’autres personnes. Modifiez librement le texte (votre version est gardée pour ce jour), **Régénérer** le reconstruit, **Copier** le met dans le presse-papiers.

![Par ticket : le temps tracé face au temps saisi, un bouton Saisir par ticket](images/trail-ticket.webp)

![Daily : hier, aujourd’hui et blocages, prêts à coller](images/trail-daily.webp)

Le bloc s’ouvre sur le daily le matin et sur le temps à saisir ensuite, puis sur l’onglet que vous avez choisi en dernier. ↻ relit vos traces tout de suite.

Chaque source a son interrupteur dans Réglages › Travail › Trace du jour : commits (`git log` dans les clones, d’après votre `user.email`), événements de la forge (les événements de GitLab, `users/<you>/events` de GitHub : les vôtres uniquement), changements de statut des tickets (vus pendant que Prism tourne) et réunions.

[Ouvrir Réglages › Travail › Trace du jour](prism:settings/trail)
