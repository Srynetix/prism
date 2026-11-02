---
title: Pipelines de données (GCP)
group: Watch the system
blocks: data, bqcost
related: settings, infra, todo, ref-settings
---
Les blocs **Data** et **Coût BigQuery** surveillent vos pipelines de données sur Google Cloud : un job est-il cassé, une table en retard, un flux à la traîne, ou la facture BigQuery du jour dérive-t-elle ? Prism ne lit que des métadonnées, et jamais une ligne de vos données.

## L’activer
Les blocs de données sont désactivés par défaut. Prism lit GCP avec votre connexion `gcloud`, en lecture seule : lancez donc d’abord `gcloud auth login`.

1. Ouvrez **Réglages › Infra › Data (GCP)** et activez **Accès GCP** (`[data] enabled`). Une vérification à côté de l’interrupteur indique si votre connexion gcloud fonctionne.
2. Listez les identifiants des projets GCP à lire dans **Projets**, et réglez la **Région** de vos jobs Cloud Run et de vos flux Datastream, et l’**Emplacement BigQuery** de vos requêtes planifiées.
3. Ajoutez dans **Tables** les tables censées être mises à jour (voir plus bas).
4. Enregistrez avec `⌘S`, puis cliquez sur **Redémarrer Prism** : la section Data est lue au lancement.
5. Ajoutez les blocs **Data** et **Coût BigQuery** à une page avec **+ Bloc** dans **Modifier la mise en page**, ou partez du modèle Data d’un pack dans l’assistant de pages.

[Ouvrir Réglages › Infra › Data (GCP)](prism:settings/data)

Chaque type de vérification a son propre interrupteur, tous activés par défaut : **Jobs Cloud Run**, **Fraîcheur des tables**, **Requêtes planifiées**, **Datastream** et **Coût BigQuery**.

> [!NOTE]
> Prism ne lit que des métadonnées : jamais de lignes, de schémas de table, de SQL, de lignes de log ni le message d’une exécution (seulement son code de sortie), et jamais le nom de la personne qui a lancé une requête. Tous les appels sont gratuits : aucune requête BigQuery n’est exécutée.

## Le bloc Data
Le titre résume : combien d’éléments sont cassés, combien dérivent, combien vont bien. Par défaut, le bloc ne montre que les **Problèmes** ; cliquez sur **Tout** pour tout voir. En format court, il ne liste que les problèmes.

**Jobs Cloud Run.** Chaque job montre sa planification, un petit historique de ses dernières exécutions, la durée de la dernière exécution et une pastille d’état :
- **en échec** (ou **en échec ×3** pour des échecs consécutifs), avec le code de sortie dans l’infobulle.
- **exécution manquée** : Cloud Scheduler n’a pas pu lancer le job.
- **2,5× la médiane** : l’exécution a pris, ou prend encore, bien plus longtemps que d’habitude. Le seuil est `[data] slow_factor` (2 par défaut).
- **en cours**, **annulé**, **jamais exécuté**, ou **ok**.

Cliquez sur un job pour le déplier : pourquoi il est signalé, la dernière exécution, sa durée face à la médiane, son déclencheur et sa prochaine exécution, quand il a été déployé, et des liens vers **Exécutions** et **Logs** dans la console GCP.

**Fraîcheur BigQuery.** Les tables que vous avez déclarées, avec leur âge face à leur âge maximal. Une table non modifiée depuis plus que son **Âge max** est en retard, en orange. Elle passe au rouge quand le job Cloud Run qui l’alimente a échoué.

**Requêtes planifiées.** Affichées quand quelque chose ne va pas (une dernière exécution en échec ou annulée), ou sous **Tout**.

**Datastream.** Chaque flux montre sa source et sa destination, son retard de réplication et son état. Un flux en échec est rouge ; un flux en retard de plus de `[data] stream_lag_minutes` (5 par défaut) est orange. Cliquez dessus pour voir son retard sur les 6 dernières heures et un lien vers la console Datastream.

## Déclarer les tables
Prism ne devine jamais quelles tables comptent. Dans **Réglages › Infra › Data (GCP) › Tables**, ajoutez une ligne par table :

| Colonne | Signification |
|---|---|
| Table | `project.dataset.table` |
| Âge max (h) | en retard au-delà de ce nombre d’heures |
| Alimentée par | le job Cloud Run qui l’écrit (facultatif) |

Avec **Alimentée par** renseigné, une table en retard dont le job a échoué apparaît en rouge : vous voyez la cause, pas seulement le symptôme.

## Le bloc Coût BigQuery
Ce bloc estime ce que coûtent les requêtes BigQuery : les octets facturés multipliés par le prix à la demande (`[data] price_per_tib`, 6,25 par défaut). C’est une estimation, pas votre facture : ni remises, ni stockage.

- **Aujourd’hui**, face à la médiane des 14 derniers jours. Au-delà de `[data] cost_spike_factor` (3 par défaut) fois cette médiane, il est signalé comme un pic.
- **Ce mois-ci**, et les octets facturés aujourd’hui, avec une barre par jour pour les 14 derniers jours.
- **Les plus chères aujourd’hui** : le type d’instruction, les tables lues et le compte de service. Une requête lancée par une personne apparaît comme « une personne » : personne n’est nommé.

> [!TIP]
> Pour voir les jobs de tout le monde, pas seulement les vôtres, votre compte a besoin de la permission `bigquery.jobs.listAll`.

## Rafraîchissement et alertes
Le bloc Data est lu toutes les `[data] poll_seconds` (60 s par défaut) ; les jobs BigQuery, pour le coût, toutes les 5 minutes. Le bouton ↻ de l’un ou l’autre bloc relit GCP tout de suite, coût compris.

Ce qui est cassé apparaît dans les **Alertes** et envoie une notification. Ce qui ne fait que dériver (un job lent, une table en retard, un flux à la traîne, un pic de coût) reste dans les **Alertes** sans notifier.

Les trois seuils se trouvent dans **Réglages › Interface › Alertes et son** et s’appliquent dès l’enregistrement.

[Ouvrir Réglages › Interface › Alertes et son](prism:settings/alerts)
