---
title: Environnements, vulnérabilités et Docker
group: Watch the system
blocks: envs, argocd, vulns, docker
related: ci, agents, settings, todo
---
Les applications déployées sont-elles en bonne santé et tournent-elles avec la version attendue, et quelles vulnérabilités connues partent avec ce que vous construisez ? Depuis ces deux blocs, une session Claude peut prendre en charge le diagnostic ou le correctif.

## Le bloc Environnements
![Le bloc Environnements : une application désynchronisée, avec Enquêter](images/environments.webp)


Le bloc **Environnements** lit vos clusters Kubernetes, en lecture seule, toutes les 30 secondes. Ses onglets, un par environnement (`[[environments]]` : **prod**, **staging**…), portent chacun un compteur rouge quand des workloads ont un problème.

En haut d’un onglet, trois chiffres :

- **Prêts** : les workloads dont tous les pods sont prêts, sur le total. Il signale aussi les namespaces que Prism n’a pas pu lire.
- **Redémarrages** : les redémarrages des pods actuels.
- **Images** : là où un bloc de votre équipe donne les versions attendues (`[clusters] stages`), combien de workloads sont en retard sur le dernier tag de leur stage. Le stage de branche (staging) fait tourner les builds d’une branche, il n’y a donc rien à comparer.

En dessous, un en-tête nomme les colonnes (faites glisser le bord droit d’un en-tête pour redimensionner sa colonne), puis les workloads sont groupés par cluster, avec sa charge CPU et RAM, puis par application. Une ligne d’application additionne ses composants (pods prêts, ressources, dernier déploiement, redémarrages) et indique combien ont un problème ; cliquez dessus pour les afficher ou les masquer. Un clic sur la ligne d’un cluster replie ses applications de la même façon : replié, il indique toujours combien de composants ont un problème. Les deux sont retenus.

Chaque composant montre ses pods prêts, ses redémarrages, la raison d’attente ou la dernière sortie (`OOMKilled`, `CrashLoopBackOff`…), et le tag de l’image qu’il fait tourner. Un composant en retard sur le tag attendu affiche `≠` et le tag qu’il devrait faire tourner.

## ArgoCD
Quand ArgoCD gère une application, un badge sur sa ligne donne l’état de synchronisation et de santé : **Synced**, **Syncing**, **Sync failed**, ou la santé quand elle n’est pas bonne. Le survol montre la révision et la dernière synchronisation ; un clic ouvre l’application dans ArgoCD quand son URL web est configurée. Chaque ligne de cluster résume aussi ses applications (`3 synced`, ou `1/3 not synced`).

Une application désynchronisée depuis plus de `[alerts] argo_out_of_sync_minutes` apparaît dans les Alertes.

Un clic sur une pastille ArgoCD ouvre la **vue de synchronisation** ; le résumé ArgoCD d’un cluster aussi, ou bien il désigne le bloc ArgoCD quand la page l’affiche : les Applications des instances de ce cluster, celles en échec ou en cours de synchronisation d’abord, et pour celle choisie sa dernière opération (phase, quand elle a commencé et combien de temps elle a duré, auto-sync ou lancée à la main, message), les ressources de cette synchronisation avec leur résultat (un hook en échec apparaît), les ressources désynchronisées ou en mauvaise santé maintenant, et ses derniers déploiements. Pendant une synchronisation, ArgoCD est lu toutes les 5 secondes au lieu de 30. ⌘-clic sur une pastille ouvre toujours l’Application dans ArgoCD.

## Le bloc ArgoCD
Là où Environnements montre ArgoCD à côté de chaque workload, le bloc **ArgoCD** (+ Bloc › Infra) regarde ArgoCD lui-même, pour chaque Application que ses instances gèrent : celles de la plateforme et des add-ons qu’aucun pipeline ne livre, et les instances clients dans un onglet à part (les groupes des chemins Helm, `[argocd_layout]`).

- En haut, **Synced**, **Healthy**, **Synchronisations en échec** et **Dérive** (Applications qui s’écartent de Git, et combien de ressources).
- **À regarder** : une synchronisation en échec, une Application dégradée ou absente, une désynchronisée ou en cours de synchronisation, la pire d’abord. Sous chacune, pourquoi : les ressources de sa dernière synchronisation qui ont échoué (un hook, une ressource refusée), et celles modifiées hors de Git ou en mauvaise santé maintenant. **Enquêter** ouvre une session Claude quand l’Application aboutit dans un environnement de `[[environments]]` (ArgoCD ne dit pas sur quel cluster il déploie).
- **Synced et healthy** : les autres, en mur de noms par environnement (production d’abord).
- **Opérations de synchronisation** : la dernière opération de chaque Application sur 24 heures, quel que soit son déclencheur (un auto-sync, un changement de config, une synchronisation à la main), avec son résultat et sa durée.

Un clic sur n’importe laquelle ouvre la vue de synchronisation. Le bloc ne fait que lire : il ne synchronise ni ne rafraîchit jamais. Il ne montre ni les versions ni ce qui attend d’être livré : c’est le rôle d’un bloc de livraison.

## Logs des pods
Cliquez sur le nom d’un composant, ou sur son bouton **Logs**, pour ouvrir ses logs dans un panneau latéral : ses pods à gauche, le log du conteneur choisi à droite, suivi en direct.

1. Choisissez un pod, et un conteneur s’il en a plusieurs.
2. Après un plantage, passez de **Actuel** à **Précédent** pour lire le log d’avant le dernier redémarrage.
3. Le panneau montre les 2 000 dernières lignes du log. Remontez tout en haut (ou cliquez sur **Lignes plus anciennes**) pour lire plus loin, 10 000 puis 50 000 lignes, puis jusqu’au démarrage du conteneur (200 000 lignes ou 48 Mo au plus).
4. Tapez dans **Filtrer les lignes…** pour ne garder que les lignes correspondantes ; **Première erreur** saute à la première ligne d’erreur, **Fin** suit la fin.
5. Appuyez sur `f` pour le plein écran, `Esc` pour fermer.

### Un shell dans le pod
**Shell** dans l’en-tête du panneau ouvre un terminal dans le conteneur choisi (`kubectl exec`, bash si l’image l’a, sh sinon), avec les droits de votre kubeconfig. Comme les shells d’un worktree, tmux le garde : fermer le tiroir ne fait que le détacher, `exit` ou × le termine, **+** en ouvre un autre. Si kubectl échoue (pas de droits, pas de shell dans l’image), son message reste à l’écran jusqu’à Entrée. Pour y revenir : **N shells** dans la barre d’état liste les shells encore ouverts, et `⌘⇧T` rouvre le dernier tiroir fermé (voir Clavier). Désactivez-le avec **Shell dans un pod** dans Réglages › Infra › Clusters ; il n’est jamais proposé sur un téléphone.

### CPU et mémoire dans le temps
**Usage** dans l’en-tête du panneau affiche, au-dessus du log, le CPU et la mémoire du pod choisi sur 1 h, 6 h, 24 h ou 7 j : un graphe chacun, sa limite (la somme des limites de ses conteneurs, ou de leurs requests) en pointillés quand elle est proche, écrite dessous dans tous les cas. Survolez un graphe pour lire un moment dans les deux. Sur un cluster GKE (son contexte `gke_<projet>_<zone>_<cluster>`), l’historique vient de Google Cloud Monitoring (Google Managed Prometheus, les métriques de cAdvisor), pour tout pod, celui d’un environnement aussi ; il utilise la connexion de gcloud et demande le rôle `monitoring.viewer` sur le projet. Ailleurs, Prism garde lui-même les mesures des pods du bloc Pods, toutes les 15 s pendant 6 heures, depuis son lancement. Un téléphone l’affiche aussi, pour un pod du bloc Pods ou Environnements. Désactivez-les avec **Usage d’un pod dans le temps** et **Google Managed Prometheus** dans Réglages › Infra › Clusters.

## Enquêter avec Claude
Un composant en difficulté a un bouton **Enquêter**. Il ouvre une session Claude dans le dépôt, avec le workload, son namespace, son contexte et le problème constaté. Claude diagnostique avec `kubectl` (describe, logs précédents, événements du namespace) sans rien modifier dans le cluster, puis explique la cause et propose un correctif dans le code ou la config. `⇧`-clic la lance tout de suite.

> [!NOTE]
> Prism n’écrit jamais dans un cluster. Il n’a besoin que d’un accès en lecture aux pods et aux deployments des namespaces listés.

## Configurer les clusters
Les environnements sont listés dans Réglages › Infra › Kubernetes › Environnements (`[[environments]]`) : une ligne par environnement et par cluster, avec **Env** (prod, demo, staging, ou un nom à vous comme `ops` : chaque nom est un onglet du bloc, ceux d’aucun stage en dernier), **Contexte kube** et **Namespaces** (`*` remplace n’importe quels caractères : `*` lit tous les namespaces du cluster, `app-*` ceux qui commencent par `app-` ; aucun : rien n’est lu). Les instances ArgoCD vont dans Réglages › Infra › Kubernetes › ArgoCD (`[[argocd]]`) : namespace, contexte kube, URL web, et les environnements dont chacune fait autorité. Sans aucune, Prism cherche dans les namespaces `argocd*` du cluster du contexte par défaut (Réglages › Infra › Kubernetes › Clusters).

> [!TIP]
> Pas de droits sur les clusters ? Désactivez **Accès au cluster** (`[clusters] enabled`) dans Réglages › Infra › Kubernetes › Clusters : pods, environnements et ArgoCD ne sont alors plus lus du tout, et les Alertes ne montrent aucune erreur d’accès.

[Ouvrir Réglages › Infra › Kubernetes › Environnements](prism:settings/envs)

## Le bloc Vulnérabilités
![Le bloc Vulnérabilités : ce qu’il faut corriger, par bibliothèque et par image](images/vulnerabilities.webp)


Le bloc **Vulnérabilités** (désactivé par défaut : Réglages › Sécurité › Vulnérabilités ; il demande `trivy`) lance un scan Trivy local, tel que la CI le verrait aujourd’hui : les résultats HIGH et CRITICAL qui ont un correctif, avec le `.trivyignore` de chaque projet appliqué. Son titre montre la branche et le commit scannés, et quand. Il scanne une fois par jour ; le bouton ↻ relance un scan tout de suite. Un badge **+N nouvelles** compte les résultats apparus depuis le scan précédent.

Trois chiffres les résument : **À corriger** (critiques et hautes), **Acceptées** (dans un `.trivyignore`) et **Sains** (projets et images sans rien à signaler).

Trois groupes sont scannés, chacun avec sa case sous **Afficher** :

- **Fichiers** : les dépendances de la branche principale (lockfiles, manifestes), le dépôt d’un seul tenant, ou chaque projet `project.json` de `[vulnerabilities] components` à part (un monorepo Nx). `[vulnerabilities] branches = ["release", "main"]` scanne aussi la dernière branche de release : chaque projet a alors une ligne par branche (`cart-api · release`, `cart-api · main`), et le titre nomme les deux branches.
- **Images** : les images qu’un bloc de votre équipe nomme (`[vulnerabilities] images`), ses propres réglages disant lesquelles et où elles sont.
- **Base** : les images de base que ce bloc nomme aussi, construites dans un autre dépôt (un Dockerfile par image), scannées avec leur `.trivyignore`.

**Masquer les ignorées** masque partout les résultats acceptés ; **Masquer les sains** masque les projets et images sans rien à signaler. Cliquez sur un projet pour voir ses propres résultats. Chaque résultat donne le paquet, la CVE (avec son lien), la version installée et la version qui la corrige.

## Corriger une CVE
**Corriger la CVE**, sur un résultat, ouvre une session Claude qui la corrige (avec le skill de votre équipe s’il en a un) : elle trouve la dépendance dans chaque projet, la met à jour et vérifie à nouveau, dans un nouveau worktree. Quand chaque cible touchée est une image de base d’un autre dépôt, la session démarre plutôt dans ce dépôt, pour corriger son Dockerfile.

1. Trouvez le paquet sous **À corriger**.
2. Cliquez sur **Corriger la CVE** (ou `⇧`-clic pour la lancer tout de suite).
3. Relisez la modification que propose Claude, puis utilisez ↻ pour scanner à nouveau.

Sur GitHub, les alertes **Dependabot** ouvertes du dépôt, hautes et critiques, forment une cible à part, à côté de celles de Trivy : la plage de versions concernée tient lieu de version installée, et chaque résultat renvoie vers son alerte.

[Ouvrir le bloc Vulnérabilités](prism:block/vulns)
[Ouvrir Réglages › Sécurité › Vulnérabilités](prism:settings/vulns)

## Docker
![Le bloc Docker : l’usage disque avec ses boutons de nettoyage, puis les conteneurs](images/docker.webp)

Le bloc **Docker** montre le démon Docker local (Docker Desktop, OrbStack, colima…), lu via la CLI `docker` du contexte courant, ou de `[docker] context`. Activez-le dans Réglages › Infra › Docker, puis ajoutez le bloc à une page.

En haut, le disque par type : images, conteneurs, volumes et cache de build, chacun avec son nombre d’actifs et son total, et sa taille ; la partie orange d’une barre est ce que Docker pourrait libérer. Chaque type qui a quelque chose à libérer a son bouton de nettoyage, en deux clics : **Nettoyer les orphelines** supprime les images sans tag qu’aucun conteneur n’utilise, **Nettoyer les arrêtés** les conteneurs arrêtés, **Nettoyer les inutilisés** tous les volumes qu’aucun conteneur n’utilise (y compris les volumes nommés : leurs données sont perdues), **Nettoyer** le cache de build. Le disque est mesuré toutes les 3 minutes et juste après une action.

En dessous, trois onglets :
- **Conteneurs**, par projet Compose : état et santé, ports, CPU et mémoire. **Log** ouvre le tiroir de logs avec les conteneurs du projet à gauche ; un conteneur en cours redémarre ou s’arrête (deux clics), un conteneur arrêté démarre ou est supprimé (deux clics).
- **Images**, les plus grosses d’abord : la taille propre à l’image (ce que sa suppression libère) et les conteneurs qui l’utilisent. Une image inutilisée peut être supprimée.
- **Volumes**, les plus gros d’abord, les inutilisés signalés.

Les Alertes signalent un conteneur en mauvaise santé, qui redémarre en boucle ou sorti en erreur (les codes 137 et 143 d’un arrêt n’en sont pas), Docker qui ne répond pas, et des données récupérables au-delà de `[docker] reclaimable_gb` (10 Go par défaut ; 0 désactive l’alerte).

[Ouvrir le bloc Docker](prism:block/docker)
[Ouvrir Réglages › Infra › Docker](prism:settings/docker)

## Système
Le bloc **Système** montre ce que l’ordinateur a sous le pied pendant que tournent les agents, les serveurs de dev et Docker. Il est activé par défaut (Réglages › Infra › Système) ; ajoutez-le à une page.

- **CPU**, **Mémoire** et **Réseau**, chacun avec ses 10 dernières minutes. La couleur de la mémoire est la pression mémoire de macOS : vert, orange quand il compresse, rouge quand il swappe ; la barre répartit ce que tiennent les apps, le système (wired) et le compresseur.
- **Disque** : l’espace libre du disque qui contient votre dossier personnel — orange sous 10 % ou 25 Go, rouge sous 5 % ou 10 Go, limites réglées dans Réglages › Infra › Système (`[system] disk_warn_gb`, `disk_warn_percent`, `disk_alert_gb`, `disk_alert_percent` ; 0 en désactive une) — et, sous **Disques**, les volumes externes branchés. Sur macOS, c’est ce qu’un fichier peut encore occuper : le Finder ajoute l’espace que le système pourrait purger.
- **Qui consomme**, groupé selon ce que Prism connaît : ses agents (leurs processus sous tmux), Docker (sa machine virtuelle), les shells et serveurs de dev, Prism lui-même, puis les trois autres applications les plus lourdes, leurs processus auxiliaires comptés avec elles. Le CPU est en % d’un cœur, additionné.
- **Réseau** : la latence d’une poignée de main TCP vers les services qu’appelle Prism (sa forge, son tracker, l’API de ses agents, et `[system] hosts`), toutes les 15 s, avec les dernières poignées de main de chaque hôte ; la perte et la gigue sur 5 minutes, la passerelle par défaut, le trafic, et le signal et le débit du lien Wi-Fi (jamais le nom du réseau).

Dans la barre d’état : le CPU avec ses dernières minutes, la RAM et la latence ; un clic affiche le bloc. Les Alertes signalent une pression mémoire rouge et un disque presque plein (tous deux avec une notification), un disque qui se remplit, un CPU à 90 % ou plus pendant 5 minutes, et plus de 5 % de poignées de main perdues (`[system] alerts`).

Sur le téléphone, **Système** est l’une des vues de l’accès à distance (sous **Plus**, ou un onglet du bas avec `[remote] tabs`) : les jauges deux par deux, puis qui consomme, les disques et le réseau. Le bloc prend aussi cette disposition dans une colonne étroite sur l’ordinateur.

[Ouvrir le bloc Système](prism:block/system)
[Ouvrir Réglages › Infra › Système](prism:settings/system)
