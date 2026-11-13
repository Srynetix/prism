---
title: Environnements, vulnérabilités et Docker
group: Watch the system
blocks: envs, argocd, changes, vulns, docker, disk_usage
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

## Le bloc Changements
Depuis hier, qu’est-ce qui a bougé sur staging ? Le bloc Changements met sur une seule frise ce qui a touché un environnement : les syncs Argo CD (leur révision, la MR qui l’a apportée, les images déployées), les MR mergées dans la branche qui l’alimente (`[changes] branches`), les pods qui se déploient, redémarrent, partent en CrashLoop ou en OOMKilled, les alertes qui apparaissent et disparaissent, les vulnérabilités nouvelles ou corrigées, et ce qu’ajoutent les blocs d’un pack.

- La bande du haut : une ligne par source, une marque par changement ; les plages où Prism était fermé sont hachurées. Glisser dessus zoome sur une plage.
- La liste en dessous, par heure, la plus récente en haut : les répétitions se replient (un pod qui redémarre 6 fois en 4 min tient sur une ligne). Une heure marquée *vu* est celle où Prism l’a vu : alertes et vulnérabilités n’ont pas de date à elles.
- Un crash montre le sync ou le déploiement de la même application juste avant (`[changes] correlate_minutes`) comme *probablement lié* : une supposition sur l’heure et le nom, jamais une cause certaine. **Diagnostiquer** lance un agent en plan mode avec la frise de l’application, pour lire ses logs, ses events et le diff de ce changement.
- **Comparer** met deux environnements sur le même axe, staging au-dessus de prod (`[changes] compare`, sinon celui affiché et le suivant dans l’ordre des rôles dev → staging → preprod → prod ; cliquer un environnement le compare, ⇄ inverse). La même révision ou MR sur les deux est reliée avec son délai ; à défaut, la même application avec la même image (`api:2.15.0`), en pointillé. Un trait est rouge quand le changement avait crashé sur le premier, et la ligne du second le dit. *En attente sur prod* liste ce qui est passé sur le premier et pas encore sur le second, chacun avec s’il y a crashé. Les deux listes côte à côte ; en étroit, une à la fois.
- **Ce qui a changé**, dans les logs d’un pod, sur une alerte de crash Argo CD et dans le panneau Argo CD d’une application Degraded, l’ouvre autour du premier crash. ⌘K : *Changements staging*. Copier donne la plage en Markdown, pour un post-mortem.

Prism garde les transitions dans un journal (`changes.jsonl` dans le dossier de données du profil, `retention_days`). Les Events Kubernetes (`kube_events`) sont désactivés par défaut : le cluster les garde une heure, et il faut le droit de lister les events.

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

Sous chaque image, ce qui a été analysé exactement : le début de son digest (un clic le copie en entier ; au survol, sa référence complète, sa plateforme, sa date de construction et l’heure de l’analyse) et son chemin dans le registre. Une image publiée pour arm64 en plus d’amd64 est analysée pour les deux, sa ligne arm64 à côté (`cart-api · prod · arm64`). **Vérifier sur le registre** demande au registre, sans rien télécharger, vers quoi pointe chaque tag analysé : **changée depuis l’analyse** quand c’est un autre digest, **0.2.149 publiée** quand un tag plus récent de la liste du bloc est sorti, **à jour** sinon. **Réanalyser** analyse cette image seule à nouveau, pas tout le reste. Le registre est lu par `buildx imagetools` de Docker (votre `docker login`), ou par gcloud pour une image sur GCP.

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
- **Disque** : l’espace libre du disque qui contient votre dossier personnel — orange sous 10 % ou 25 Go, rouge sous 5 % ou 10 Go, limites réglées dans Réglages › Infra › Système (`[system] disk_warn_gb`, `disk_warn_percent`, `disk_alert_gb`, `disk_alert_percent` ; 0 en désactive une) — et, sous **Disques**, les volumes externes branchés. Sur macOS, c’est ce qu’un fichier peut encore occuper : le Finder ajoute l’espace que le système pourrait purger. Pour voir où va la place, le bloc Espace disque ci-dessous scanne un dossier.
- **Qui consomme**, groupé selon ce que Prism connaît : ses agents (leurs processus sous tmux), Docker (sa machine virtuelle), les shells et serveurs de dev, Prism lui-même, puis les trois autres applications les plus lourdes, leurs processus auxiliaires comptés avec elles. Le CPU est en % d’un cœur, additionné.
- **Réseau** : la latence d’une poignée de main TCP vers les services qu’appelle Prism (sa forge, son tracker, l’API de ses agents, et `[system] hosts`), toutes les 15 s, avec les dernières poignées de main de chaque hôte ; la perte et la gigue sur 5 minutes, la passerelle par défaut, le trafic, et le signal et le débit du lien Wi-Fi (jamais le nom du réseau).

Dans la barre d’état : le CPU avec ses dernières minutes, la RAM et la latence ; un clic affiche le bloc. Les Alertes signalent une pression mémoire rouge et un disque presque plein (tous deux avec une notification), un disque qui se remplit, un CPU à 90 % ou plus pendant 5 minutes, et plus de 5 % de poignées de main perdues (`[system] alerts`).

Sur le téléphone, **Système** est l’une des vues de l’accès à distance (sous **Plus**, ou un onglet du bas avec `[remote] tabs`) : les jauges deux par deux, puis qui consomme, les disques et le réseau. Le bloc prend aussi cette disposition dans une colonne étroite sur l’ordinateur.

[Ouvrir le bloc Système](prism:block/system)
[Ouvrir Réglages › Infra › Système](prism:settings/system)

## Espace disque
Le bloc **Espace disque** montre où va la place d’un dossier ou de tout le disque, scanné à la demande : rien n’est lu avant que vous scanniez, rien n’est surveillé entre deux scans. Il est activé par défaut (Réglages › Infra › Espace disque) ; ajoutez-le à une page.

- **Scanner** : choisissez un dossier de `[disk_usage] roots` (le dossier personnel `~`, tout le disque `/`…) ou **Choisir…** n’importe quel autre, puis **Scanner** ; **Rescanner** le relit, **Arrêter** met fin au scan tout de suite. Le bloc reste sur le disque du dossier (`[disk_usage] one_volume`) : les autres disques et partages réseau montés à l’intérieur sont laissés de côté, comme les dossiers gardés seulement dans le cloud (iCloud Drive, OneDrive…), qui n’occupent pas de place. Sur macOS, le système peut demander une fois si Prism peut lire Documents, Bureau, Téléchargements ou Photos : le scan attend votre réponse (l’accès complet au disque l’évite).
- **La liste**, à gauche : les dossiers par taille, ouverts un niveau à la fois avec ▸, leur part et **Δ**, de combien chacun a grossi ou diminué depuis le scan précédent (`[disk_usage] delta`). Les dépôts git et les worktrees sont signalés.
- **La treemap**, à droite : chaque dossier un rectangle de sa taille, sur trois niveaux, coloré par type (build, cache, VM, médias, modèles, archives…) ou par dossier.
- Un clic sélectionne (son chemin et sa taille en dessous, avec ses actions), un double-clic ouvre un dossier, le chemin au-dessus permet de remonter ; un clic droit donne les actions : l’afficher dans le gestionnaire de fichiers, un terminal à cet endroit, copier son chemin, le scanner seul, le mettre à la corbeille (deux clics).
- **Récupérable** : ce qui peut être reconstruit, reconnu par son nom — `node_modules`, `target/` de Rust, `.venv`, `.next` et les autres builds JavaScript, `.terraform`, CocoaPods, le DerivedData de Xcode, les caches, les simulateurs iOS qu’aucun Xcode ne peut lancer, l’image disque de Docker, la corbeille. Chaque groupe indique comment il est libéré et part en deux clics ; **Afficher** trouve le plus gros. Désactivez `[disk_usage] cleanup` pour un bloc qui ne fait que lire.
- **Corbeille**, en haut du bloc, avec ou sans scan : sa taille et ses éléments, sur tous les disques (celle du dossier personnel et celles des autres disques, le détail sur « N disques »), mesurée à l’affichage du bloc et après une mise à la Corbeille (un clic sur Corbeille la mesure à nouveau). **Vider** les supprime définitivement en deux clics, sans passer par le Finder ; ce qui est verrouillé ou en cours d’utilisation reste et est compté. Sur macOS, Prism a besoin de l’accès complet au disque pour lire `~/.Trash` : la ligne le dit quand il ne peut pas.

Les tailles sont ce que les fichiers occupent sur le disque (une image disque creuse compte ce qu’elle contient, un lien dur une seule fois). Le dernier scan de chaque dossier est conservé dans le dossier de données de Prism : Δ s’y compare, et le bloc le réaffiche après un redémarrage, moins détaillé (les petits dossiers additionnés dans leur parent) jusqu’au scan suivant.

[Ouvrir le bloc Espace disque](prism:block/disk_usage)
[Ouvrir Réglages › Infra › Espace disque](prism:settings/disk_usage)
