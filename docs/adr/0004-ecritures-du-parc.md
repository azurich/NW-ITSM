# ADR 0004 — Écritures du parc

- **Statut** : accepté
- **Date** : 2026-10-10

## Contexte

Le lot 2 ouvre les premières écritures métier : création, modification,
mise à la corbeille et restauration des ordinateurs, moniteurs,
imprimantes, matériels réseau, périphériques et téléphones.

Côté PHP, une écriture (`CommonDBTM::add`, `update`, `delete`, `restore`)
déclenche de nombreux effets de bord :

- l'historique (`glpi_logs`) et le journal des événements (`glpi_events`) ;
- les règles métier du parc (`RuleAsset`), qui peuvent modifier les valeurs ;
- l'unicité des champs (`FieldUnicity`), qui peut refuser ou notifier ;
- les informations financières : création automatique (`auto_create_infocoms`)
  et dates remplies selon le statut (`autofill_*` des entités) ;
- pour un ordinateur, la recopie de certains champs vers les éléments
  connectés et les composants (`Computer::post_updateItem`), et l'état des
  logiciels installés à la mise à la corbeille ;
- les hooks des plugins (`pre_item_add`, `item_update`, `item_delete`...).

Trois voies possibles : écrire via l'API REST du PHP (le Rust dépend alors
du PHP pour longtemps), tout porter d'un coup (long, et chaque oubli
corrompt silencieusement les données), ou porter le chemin courant et
**refuser explicitement** ce qui ne l'est pas encore.

## Décision

Le Rust écrit **nativement**, dans une transaction, avec les mêmes effets
que le PHP pour tout ce qui est porté. Quand une fonction PHP non portée
pourrait s'appliquer, l'écriture est refusée avec `409 legacy_required` et
un motif ; l'interface propose alors de continuer dans l'interface
classique, sur le même élément. Rien n'est écrit en cas de refus.

### Ce qui est porté

- Droits : `CREATE`, `UPDATE`, `DELETE` du profil sur le type (`computer`,
  `monitor`, `printer`, `networking`, `peripheral`, `phone`). Comme
  `canCreateItem` / `canUpdateItem`, l'élément doit appartenir à une entité
  **active** : un élément partagé par une entité parente est visible mais
  en lecture seule. La corbeille et la restauration demandent `DELETE`,
  comme les formulaires PHP.
- Valeurs : texte enregistré avec `<` et `>` encodés en `&lt;` et `&gt;`
  (comme le nettoyage des entrées du PHP), 255 caractères au plus (65 535
  octets pour le commentaire) ; identifiants de listes vérifiés en base.
  Seules les colonnes modifiées sont écrites, avec `date_mod`.
- Historique au format de `Log::constructHistory` : option de recherche du
  champ, valeurs brutes pour le texte, `Nom (id)` pour les listes (nom
  complet des arbres avec `&gt;`, identifiant de connexion des
  utilisateurs, `&nbsp; (0)` pour « aucun »), auteur `Nom (id)`. Création,
  corbeille et restauration ont leurs actions propres (20, 13, 14, ou 26
  et 27 pour les éléments d'un outil d'inventaire).
- Événements `glpi_events` des formulaires PHP (`adds the item`,
  `updates an item`, `deletes an item`, `restores an item`, niveau 4,
  service `inventory`).
- Ordinateurs : recopie du contact, de l'utilisateur, du groupe, du lieu
  et du statut vers les éléments connectés non globaux et vers le lieu et
  le statut des composants, selon la configuration
  (`is_*_autoupdate`, `state_autoupdate_mode`), avec leur historique ;
  logiciels installés marqués à la corbeille et à la restauration.
- Listes de choix des formulaires : valeurs de l'entité de l'élément et
  valeurs récursives de ses parents ; statuts visibles pour le type,
  groupes « éléments » pour le groupe, groupes « assignables » pour le
  groupe technique, utilisateurs ayant un profil dans l'entité, et pour le
  technicien ceux qui peuvent se voir attribuer des tickets (`own_ticket`).

Vérification : une même modification (texte avec `<`, lieu, statut,
utilisateur, groupe, avec un moniteur connecté et un processeur), puis une
mise à la corbeille et une restauration, appliquées par le code PHP à un
ordinateur et par l'API à son jumeau, donnent des lignes `glpi_logs` et des
valeurs identiques, y compris sur le moniteur et le composant.

### Ce qui renvoie vers l'interface classique

| Motif (`details`) | Situation |
| --- | --- |
| `plugins` | Un plugin est activé : ses hooks ne s'exécuteraient pas |
| `business_rules` | Une règle métier du parc active peut s'appliquer à une saisie manuelle de ce type (y compris aux éléments connectés qui suivraient la modification). Les règles limitées aux outils d'inventaire (`_auto` = 1) ou à d'autres types sont ignorées |
| `field_unicity` | Un critère d'unicité actif existe pour ce type |
| `infocom_creation` | Les informations financières sont créées avec les éléments |
| `infocom_dates` | Le nouveau statut remplit une date des informations financières dans l'entité (paramètre hérité compris) |
| `unrecursive` | Retirer le partage avec les sous-entités : le PHP vérifie d'abord les éléments des autres entités qui l'utilisent |

Une modification qui ne change rien, ou une mise à la corbeille d'un
élément déjà à la corbeille, n'est jamais refusée.

### Hors périmètre de cette étape

- **Suppression définitive** : elle nettoie de nombreuses tables liées
  (connexions, ports, informations financières, documents, contrats,
  tickets) ; elle reste dans l'interface classique.
- **Gabarits** : exclus des listes et des écritures.
- Champs propres à un type (taille d'écran, ports de l'imprimante,
  mémoire...) : affichés et modifiés dans l'interface classique, comme les
  onglets (composants, connexions, logiciels, informations financières).
- Les valeurs choisies sont vérifiées en existence, pas en visibilité,
  comme le PHP ; seule l'interface limite les choix à l'entité.

## Conséquences

- Les écritures courantes du parc se font dans la nouvelle interface sans
  dépendre du PHP, avec des données identiques à celles du PHP.
- Une installation avec des plugins actifs, des règles métier manuelles ou
  la création automatique des informations financières continue d'écrire
  par l'interface classique, sans perte silencieuse ; chaque fonction portée
  plus tard retire un motif de refus.
- Le motif `plugins` disparaîtra avec la phase 2 (modèle d'extension
  natif) : en attendant, désactiver un plugin inutile ouvre les écritures
  natives.
