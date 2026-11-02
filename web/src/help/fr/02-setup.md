---
title: Configurer vos sources
group: Start
related: welcome, settings, pages, agents
---
Prism réutilise les accès dont votre ordinateur dispose déjà : `glab` pour GitLab ou `gh` pour GitHub, un jeton ClickUp dans le trousseau du système, votre kubeconfig, vos connexions Claude Code et gcloud. Ce guide montre comment connecter chaque source, comment les Réglages les vérifient, et ce qui se passe quand l’une d’elles est désactivée.

## Le premier lancement
Sans configuration, Prism vous accueille et vous demande votre premier profil : son nom, sa couleur, et ce dont il part : **Vide** (toutes les sources désactivées, avec le pack de démarrage de Prism : l’assistant de pages propose alors son modèle Developer), **Un pack** (celui de votre équipe, un fichier ou un fichier d’un dépôt), ou **La démo**. **Démarrer Prism** écrit `config.toml` et relance Prism dessus. **Plus tard** laisse Prism sans configuration, jusqu’à ce que vous la fassiez dans les Réglages.

## Partir du pack de votre équipe
Si votre équipe partage un **pack** (sa configuration Prism : dépôts, listes et statuts ClickUp, environnements, statuts de validation…), importez-le d’abord : **Réglages › Général › Packs**, puis un chemin de fichier ou un fichier d’un dépôt : `glab:group/project:path/pack.toml@ref` sur GitLab, `gh:owner/repo:path/pack.toml@ref` sur GitHub, la ref étant une branche, un tag ou un commit. Ses valeurs remplissent les Réglages, marquées *du pack* ; tout ce que vous modifiez est enregistré dans votre propre `config.toml`, par-dessus le pack, et **Revenir à la valeur du pack** l’annule. **Mettre à jour** relit le pack, vos modifications restent ; pour le pack d’un dépôt, le champ à côté change la branche, le tag ou le commit auquel il est lu (pour essayer un commit avant qu’il n’arrive sur main), et le commit lu s’affiche sous son nom. Comme Prism lui-même, les packs sont vérifiés au lancement et toutes les 6 heures (`[updates] packs`) : un pack modifié à sa source apparaît dans la barre d’état, et Réglages › Packs indique ce qui a changé et le met à jour. Un pack peut exiger une version de Prism assez récente (`[pack] requires`) : une version plus ancienne le refuse, et demande de mettre Prism à jour d’abord. L’emplacement de vos clones vous appartient, jamais au pack : Prism liste alors les dépôts du pack qui n’ont pas de clone ici, et les trouve dans un dossier que vous choisissez (uniquement là), vous laisse choisir chacun (vérifié par rapport à son projet), ou le clone ; **Réglages › Code › Dépôts** fait de même à tout moment. Le panneau **TOML** des Réglages montre le fichier du pack, ainsi que le pack et votre fichier fusionnés. [Ouvrir Réglages › Général › Packs](prism:settings/packs)

Chaque source est facultative : désactivez celles que vous n’utilisez pas (**GitLab** ou **GitHub**, **Runners**, **ClickUp**, **Calendrier**, **Data**) et leurs blocs l’indiquent au lieu d’échouer.

## Profils
Prism peut héberger plusieurs configurations sur la même machine, par exemple celle de votre équipe et une personnelle : chaque **profil** a son propre config.toml et ses packs, ses propres agents, notes et disposition, et ses propres entrées de trousseau. Le badge en bas du rail montre le profil actif ; cliquez dessus pour changer (`⌥⌘1`, `⌥⌘2`… dans le menu Prism › Profil). Changer de profil relance Prism sur l’autre profil ; les agents de celui que vous quittez continuent de tourner et reviennent quand vous y revenez. Pour garder les deux, **⧉** (ou **Ouvrir à côté** dans Réglages › Général › Profils) ouvre un profil dans son propre Prism, à côté de celui-ci : le titre de la fenêtre, l’icône de la barre des menus et celle du Dock portent alors le nom ou la lettre de chaque profil. Un profil s’ouvre dans un seul Prism au plus : l’ouvrir à nouveau ramène celui-ci au premier plan. La barre d’état de chacun montre les autres ouverts : leur lettre et leur nom, combien de leurs agents vous attendent (teinté alors), un clic amenant ce Prism au premier plan.

**Réglages › Général › Profils** les liste avec leur dossier, leurs packs et leurs agents en cours ; **Modifier** en renomme un ou change sa couleur. On y crée aussi un nouveau profil : vide (toutes les sources désactivées, avec le pack de démarrage de Prism : l’assistant de pages propose alors son modèle Developer), à partir du pack d’une équipe, ou **la démo** : le travail d’une équipe fictive dans chaque bloc, sans rien lire nulle part, pour essayer Prism ou le montrer. Le jeton ClickUp d’un profil va dans le trousseau sous `prism-clickup-<profile>` (le profil par défaut garde `prism-clickup`), son jeton Jira sous `prism-jira-<profile>`. Les sessions Claude Desktop apparaissent dans le profil dont elles travaillent sur les dépôts, et dans tous les profils quand elles ne travaillent sur aucun.

## Avant de commencer
Certaines choses se font hors de Prism, une seule fois, dans un terminal :

- **GitLab** : `glab auth login` ; **GitHub** : `gh auth login`. Prism les appelle et ne voit jamais le jeton, qui reste dans le trousseau du système.
- **ClickUp** : un jeton d’API personnel (voir **Créer votre jeton ClickUp** ci-dessous), collé dans **Réglages › Travail › Tracker › ClickUp › Jeton d’API**, qui le range dans le trousseau du système. Sur macOS, le service de trousseau `prism-clickup` (`security add-generic-password -s prism-clickup -a "$USER" -w`) ; sous Linux et Windows, `prism clickup-token` lit le jeton sur son entrée standard et le range dans le trousseau du système (Secret Service, Gestionnaire d’identification). Une variable d’environnement `CLICKUP_TOKEN` fonctionne aussi et a la priorité.
- **Jira** : un jeton d’API (id.atlassian.com › Security › API tokens), collé dans **Réglages › Travail › Tracker › Jira › Jeton d’API**, qui le range dans le trousseau du système sous `prism-jira` (jamais dans config.toml) ; `prism jira-token` le lit aussi sur son entrée standard. Avec `security add-generic-password` sur macOS, passez-le sous la forme `-w "$(pbpaste)"` : l’invite de `-w` ne garde que 128 caractères, et les jetons Atlassian sont plus longs ; une variable d’environnement `JIRA_TOKEN` a la priorité. **Réglages › Travail › Tracker › Jira** indique le site et l’e-mail du compte.
- **Mail Microsoft 365** (bloc Mail) : une application que votre organisation a enregistrée dans Entra ID, avec l’autorisation déléguée Mail.ReadBasic ; vous vous connectez une fois dans **Réglages › Travail › Mail › Microsoft 365**, et le jeton d’actualisation va dans le trousseau du système.
- **Gmail et Google Agenda** (blocs Mail et Aujourd’hui, sur tout système) : un client OAuth de votre propre projet Google Cloud, puis une connexion, dans **Réglages › Travail › Mail › Gmail** ou **Calendrier › Google Agenda** (le même compte Google). Voir [Se connecter à Google](prism:help/google).
- **Obsidian** : aucune connexion ; Prism lit et écrit les fichiers du coffre.
- **Kubernetes** : un `~/.kube/config` fonctionnel avec un accès en lecture aux clusters à surveiller.
- **Agents Claude** : `tmux` et `claude` dans le PATH de votre shell de connexion (voir [Travailler avec les agents Claude](prism:help/agents)).
- **Données GCP** : `gcloud auth login`, avec un accès en lecture aux projets de données.

Les jetons ne vont jamais dans `config.toml`.

**Vérifier un accès.** Les étapes **Forge** et **Tickets** de l’assistant de pages configurent les accès dont les pages ont besoin : la commande de connexion de la forge (copiée en un clic), le jeton du tracker collé là, avec un lien vers la page qui le crée. Chaque accès est ensuite demandé au service lui-même : *connecté en tant que …*, ou ce qu’il faut faire (*pas encore de jeton*, *a refusé ce jeton*, *non connecté : lancez `glab auth login`*). La même vérification se trouve sous le jeton dans **Réglages › Travail › Tracker › ClickUp** ou **Jira**, et sous **Connexion** dans **Réglages › Code › Forge** ; **Tester** redemande, après un `glab auth login` par exemple. Un jeton est vérifié dès qu’il est enregistré, avant même le redémarrage qui le met en service. Plus tard, un jeton manquant ou refusé apparaît comme tel dans les Alertes, avec un bouton vers sa section des Réglages.

Prism récupère le PATH de votre shell de connexion au démarrage, il trouve donc `glab`, `gh`, `git`, `kubectl` et `claude` même lancé depuis le Finder. **Réglages › Général › Dépendances** montre ceux qu’il a trouvés, et leurs versions. [Ouvrir Réglages › Général › Dépendances](prism:settings/dependencies)

## Créer votre jeton ClickUp
Prism lit ClickUp avec un jeton d’API personnel : il agit en votre nom, avec vos accès, et rien de plus.

1. Dans ClickUp, cliquez sur votre avatar (en bas à gauche), puis **Settings**.
2. Dans vos propres réglages, ouvrez **Apps**.
3. Dans **API Token**, **Generate** (ou **Copy** si vous en avez déjà un). Le jeton commence par `pk_`.
4. Dans Prism, collez-le dans [Réglages › Travail › Tracker › ClickUp](prism:settings/clickup) › **Jeton d’API** (son lien ouvre la page Apps de ClickUp), puis **Enregistrer dans le trousseau** : la ligne en dessous indique *connecté en tant que …* une fois que ClickUp l’accepte. **Redémarrez Prism** pour le mettre en service.

Le jeton va dans le trousseau du système (`prism-clickup`, `prism-clickup-<profile>` pour un autre profil), jamais dans `config.toml`. **Regenerate** dans ClickUp révoque l’ancien jeton : tout ce qui l’utilise cesse de fonctionner, Prism compris jusqu’à ce que vous colliez le nouveau. Prism affiche alors *ClickUp a refusé le jeton* dans les Alertes, avec un bouton vers les réglages ClickUp.

## ClickUp, Obsidian ou Jira
Un profil lit un seul tracker, ClickUp par défaut. Avec **Obsidian Kanban** (**Réglages › Travail › Tracker**, `[tracker] provider = "obsidian"`), **Réglages › Travail › Tracker › Obsidian** indique le coffre et ses tableaux, un par dépôt : l’identifiant d’un ticket se résout alors dans le dépôt de son tableau, si bien que deux projets peuvent avoir chacun un `T-12`, et le **Préfixe d’identifiant** d’un tableau est un motif (`[ABCLM]` pour `A-07` à `M-23`). Le statut d’une carte est sa colonne ; une carte qui renvoie à une note (`[[T-12]]`) prend l’identifiant, le titre et la priorité de la note, une carte intitulée `**C-68 — …**` cet identifiant, et ses lignes indentées sont sa description. Prism déplace les cartes entre colonnes (le `status` de la note suit), règle les priorités et les commentaires, et crée des cartes. Obsidian n’a ni assignés, ni saisies de temps, ni file de refinement, ni sprint suivant : ces blocs et actions s’effacent. Avec **Jira Cloud** (`[tracker] provider = "jira"`), **Réglages › Travail › Tracker › Jira** indique le site, le compte et le tableau : Prism lit son sprint actif (les tickets ouverts d’un tableau Kanban), les statuts dans l’ordre de ses colonnes et en minuscules, et prévisualise les tickets. Prism fait avancer un ticket dans son workflow (la transition menant au statut choisi doit exister depuis là où il se trouve), règle sa priorité (la première priorité de Jira se lisant comme urgente, haute, normale ou basse ; un ticket en a toujours une) et ajoute ou modifie des commentaires en texte brut, vous l’assigne (Jira a un seul assigné : il remplace le précédent) et l’ajoute au sprint actif ; le **+** du bloc Sprint crée un ticket du **Type de nouveau ticket** dans le projet du tableau, dans le sprint actif. Avec le **Suivi du temps** activé, le bloc Temps lit et écrit vos worklogs. À prendre montre le sprint suivant du tableau et les **Files** de Réglages › Travail › Tracker › Jira, chacune une requête JQL utilisée pour la prise, le refinement (par epic) ou la synchronisation ; les tickets du tableau en cours hors du sprint actif apparaissent dans Sprint. Le préfixe des tickets et les statuts du flux se règlent dans **Réglages › Travail › Tracker**, quel que soit le tracker.

## GitLab ou GitHub
Un profil lit une seule forge, GitLab par défaut. Sur GitHub (**Réglages › Code › Forge**, `[forge] provider = "github"`), Prism lit les pull requests de vos dépôts et leurs checks (GitHub Actions et statuts de commit), les merge, les rebase, les approuve et les commente, et affiche les jobs Actions et leurs logs. Une pull request s’écrit alors `#12`. La santé de la CI lit les exécutions de workflows GitHub Actions, Vulnérabilités ajoute les alertes Dependabot, Renovate suit son workflow ou son app ; ce qui lit la CI propre à GitLab (la file de merge, un bloc qui lit les jobs GitLab) reste désactivé. L’interrupteur d’accès, les branches, la fenêtre des merges, l’intervalle de lecture et la santé de la CI se règlent dans **Réglages › Code › Forge** (`[forge]`), quelle que soit la forge ; **Réglages › Code › Forge › GitLab** contient ce que seul GitLab a.

## Connecter chaque source
![Réglages › Code › Forge : la dernière lecture de la source, puis chaque champ avec son aide](images/settings.webp)


Ouvrez les Réglages (`⌘,`). Les sources sont regroupées sous **Sources** et **Infra** dans la navigation.

1. **Forge** : GitLab par défaut ; pour GitHub, réglez le **Fournisseur** sur `github`. [Ouvrir Réglages › Code › Forge](prism:settings/forge) Donnez ensuite à chaque dépôt son **Projet** : `group/project` sur GitLab, `owner/repo` sur GitHub. La vérification à côté indique `found: …`, ou que `glab` ou `gh` ne le voit pas. [Ouvrir Réglages › Code › Dépôts](prism:settings/repos) Sur GitLab, réglez aussi l’**Hôte** (gitlab.com ou votre instance). [Ouvrir Réglages › Code › Forge › GitLab](prism:settings/gitlab)
2. **Dépôts** : chaque dépôt est une carte, regroupée par dossier de travail (là où est son clone). Ouvrez-en un pour lui donner son clone (**Choisir un dossier…** ou **Cloner**), activer ce que Prism en lit (sur cette machine : worktrees, images de base ; sur la forge : MR, pipelines, file de merge, tickets) et, au besoin, ses propres branches. Sous la liste, **Blocs qui lisent un dépôt** : Santé de la CI, Renovate, Vulnérabilités et les blocs de votre équipe lisent chacun le dépôt choisi là, et **Références de MR sans dépôt** désigne celui que `!1294` veut dire (aucun : chaque MR s’écrit `shop!1294`). Avec un seul dépôt, ou un pack qui le nomme, il n’y a rien à choisir. [Ouvrir Réglages › Code › Dépôts](prism:settings/repos)
3. **ClickUp** : les listes et dossiers sont trouvés par leur nom : le **Dossier des sprints** et les **Tableaux** au-delà du sprint — bugs à prendre, file de refinement, tâches sur lesquelles saisir du temps… (chacun une liste, ou les listes d’un dossier, et son usage) ; un nom introuvable apparaît dans les Alertes (*Config ClickUp : introuvable dans l’espace de travail*). Le **Préfixe des tickets** (ABC pour ABC-123) relie les tickets aux branches et aux MR ; vide, Prism n’a pas d’identifiants de tickets. Désactivé, aucun jeton n’est nécessaire. [Ouvrir Réglages › Travail › Tracker › ClickUp](prism:settings/clickup)
4. **Clusters** et **Pods** : choisissez le **Contexte par défaut** dans votre kubeconfig (la vérification indique `reachable`), puis ajoutez les **Cibles** du bloc Pods : un contexte (celui par défaut s’il est vide) et ses namespaces, `*` correspondant à n’importe quels caractères. Les runners GitLab et leurs jobs de CI y sont reconnus.
5. **Environnements** et **ArgoCD** : une ligne par environnement et cluster, en lecture seule.
6. **Sessions Claude** : les dossiers où Claude Desktop garde les métadonnées et transcriptions de ses sessions. Prism regarde seulement si une session travaille ou attend, jamais ce qui s’y dit.
7. **Agents** : l’interrupteur **Vue Agents** vérifie que `tmux` et `claude` sont trouvés. [Ouvrir Réglages › Agents › Agent](prism:settings/agents)
8. **Calendrier** : avec le Calendrier de macOS (par défaut), macOS demande l’accès à vos calendriers au premier lancement ; si vous avez refusé, autorisez-le à nouveau dans Réglages Système › Confidentialité et sécurité › Calendriers. Un compte Exchange ou Microsoft 365 : [Exchange dans le Calendrier macOS](prism:help/exchange). Sous Linux ou Windows, ou pour lire Google Agenda directement, réglez **Fournisseur** sur `google` et connectez-vous à Google ([Se connecter à Google](prism:help/google)).
9. **Data (GCP)** : désactivé par défaut. Activez **Accès GCP**, listez les **Projets** à lire, et la vérification indique `gcloud logged in`.

> [!TIP]
> Vous ne savez pas où se trouve quelque chose ? Tapez dans **Rechercher dans les réglages…** en haut de la navigation des Réglages.

## Lire les vérifications
Les Réglages vous disent si chaque source fonctionne, à trois endroits :

- En haut d’une section de source, sa dernière lecture : **Lue correctement avec la configuration actuelle.**, **Dernière lecture en échec : …** avec la raison, ou **Pas encore lue.**
- À côté d’une valeur vérifiable (projet, contexte kube, namespace, dossier, gcloud, outils des agents) : une coche et ce qui a été trouvé, ou un avertissement et le problème. **Vérifier à nouveau** les relance.
- Dans la navigation, une icône d’avertissement signale une section dont la vérification a échoué ou dont la source n’a pas pu être lue.

Sur le tableau de bord, une source injoignable apparaît dans les **Alertes** sous forme d’une ligne rouge **Source**, `… injoignable : …`. Un `config.toml` invalide y apparaît aussi, au lieu d’empêcher Prism de s’ouvrir.

## Les interrupteurs d’accès
Tout le monde n’a pas accès à tout. Quatre interrupteurs disent ce que Prism a le droit de lire :

| Interrupteur | Désactivé signifie |
|---|---|
| `[forge] enabled` (**Accès GitLab**, **Accès GitHub** sur GitHub) | Ni la forge ni le dépôt local ne sont lus : pas de MR, de pipelines, de worktrees, de vulnérabilités, de dépôts ni d’agents. |
| `[clusters] enabled` (**Accès aux clusters**) | Pas de pods, d’environnements ni d’ArgoCD. |
| `[agents] enabled` (**Vue Agents**) | Pas de vue Agents ; les actions Claude ouvrent Claude Desktop à la place. |
| `[calendar] enabled` (**Agenda**) | Aucun calendrier n’est lu (Calendrier macOS ou Google Agenda), ni son accès demandé : pas de bloc Aujourd’hui, de compte à rebours de réunion ni de rappels. |

`[data] enabled` (**Accès GCP**) fonctionne de la même façon pour les blocs Data et Coût BigQuery.

Quand une source est désactivée, rien n’essaie de la joindre : vous n’avez donc aucune erreur d’accès dans les Alertes. L’assistant de pages règle ces interrupteurs pour vous selon le modèle choisi : un modèle Product, par exemple, désactive la forge (GitLab ou GitHub), les clusters et les agents.

> [!NOTE]
> Les accès forge, clusters, Calendrier et GCP sont lus au démarrage de Prism. Les Réglages marquent ces valeurs d’un badge **redémarrage** et, après l’enregistrement, proposent **Redémarrer Prism**.

## Enregistrer
Les modifications sont mises de côté jusqu’à l’enregistrement : **Enregistrer dans config.toml**, ou `⌘S`. L’enregistrement ne réécrit que les clés modifiées et conserve les commentaires et la mise en forme du fichier. Le bouton **TOML** affiche le fichier à côté des réglages, pour voir exactement ce qui sera écrit.

[Lire : Les Réglages et config.toml](prism:help/settings)
