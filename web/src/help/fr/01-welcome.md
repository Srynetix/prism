---
title: Bienvenue dans Prism
group: Start
related: setup, pages, todo, keyboard
---
Prism rassemble ce qu’un développeur de l’équipe surveille toute la journée (merge requests, CI, tickets, agents Claude, agenda) et vous dit ce qui a besoin de vous, dans une seule fenêtre.

## La logique de Prism
![Le tableau de bord : Alertes, merge requests, le sprint, les agents et l’agenda du jour](images/dashboard.webp)


Prism lit vos sources (GitLab ou GitHub, ClickUp, Kubernetes, vos sessions Claude, votre Calendrier sur macOS) et les affiche sous forme de **blocs** disposés sur des **pages** : les icônes du rail à gauche. Vous ne recopiez jamais rien à la main : un clic ouvre la MR, le ticket ou l’agent là où il se trouve.

Au-dessus de tout, une question : *qu’est-ce qui a besoin de moi maintenant ?* Le bloc **Alertes** y répond, le plus grave en premier. L’en-tête le résume dans **Pour moi** (revues en attente, sessions Claude qui demandent, tickets désynchronisés), avec à côté un voyant de santé de la CI.

> [!TIP]
> Ce que vous voyez est en lecture seule tant que vous ne cliquez pas sur une action. Les actions qui pourraient faire perdre quelque chose, comme supprimer un worktree, demandent un second clic.

## Vos dix premières minutes
1. **Vérifiez vos sources.** Ouvrez les Réglages (`⌘,`) : chaque section de source indique si sa dernière lecture a réussi, et vérifie à la saisie des valeurs comme le projet d’un dépôt ou le contexte kube. [Lire : Configurer vos sources](prism:help/setup)
2. **Choisissez vos pages.** L’assistant de pages propose un point de départ : les modèles du pack de votre équipe, ou le pack de démarrage de Prism (Developer), avec votre forge, votre tracker et la CLI de votre agent. Il montre les pages et leurs blocs avant de les créer. [Ouvrir l’assistant de pages](prism:wizard)
3. **Lisez les Alertes de haut en bas.** Le rouge d’abord, puis l’orange, puis le bleu. Appuyez sur `x` sur une ligne (ou cliquez sur sa coche) pour la marquer comme vue.
4. **Retenez trois touches.** `/` filtre tout, `m` ne garde que ce qui est à vous, `i` ouvre ce qui vous attend. Toutes les autres : `?`.

## D’où viennent les données
Chaque source est lue à son propre rythme, puis tous les blocs se mettent à jour d’un coup :

- **GitLab** ou **GitHub** (un par profil) : les merge requests (pull requests sur GitHub) et leurs pipelines, via `glab` ou `gh`, toutes les 15 s par défaut, avec la santé de la CI.
- **ClickUp** : le sprint, le refinement, les tickets à prendre et le suivi du temps, toutes les 45 s par défaut. Ou bien des tableaux Kanban **Obsidian**, lus depuis votre coffre.
- **Kubernetes** : les pods de n’importe quel cluster et namespace, runners GitLab compris (suivis en direct), les environnements et ArgoCD, en lecture seule.
- **Claude** : les agents que Prism lance et vos sessions Claude Desktop. Prism lit leur état, jamais le texte de la conversation.
- **Calendrier macOS** (macOS uniquement) : les réunions du jour, lues localement sur votre Mac.
- **GCP** (facultatif, désactivé par défaut) : les pipelines de données et le coût BigQuery, métadonnées uniquement.

Les données ne circulent qu’entre votre ordinateur et ces services. Prism réutilise les accès que vous avez déjà : `glab` et `gh` gardent leur jeton dans le trousseau du système, et le jeton ClickUp s’y trouve aussi.

> [!NOTE]
> Fermer la fenêtre ne fait que la masquer. Prism continue de tourner dans la barre des menus (la zone de notification sous Linux et Windows), avec un compteur de ce qui vous attend, et envoie une notification quand quelque chose casse. `⌘Q` quitte.

## Se repérer
- Le rail à gauche change de page (`⌘1` à `⌘9`). La vue **Agents** suit les pages ; **Thème** et **Réglages** sont en bas.
- Chaque bloc a un `?` dans son titre : à quoi sert le bloc, ce qu’il montre, d’où viennent ses données, et un lien vers le guide correspondant.
- La barre d’état en bas montre votre pipeline en cours, la prochaine réunion, l’usage de Claude et plus encore.
- `⌘?` ou `F1` ouvre cette Aide à tout moment.

[Afficher les raccourcis clavier](prism:keys)
