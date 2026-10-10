# ADR 0004 — Écritures du parc

- **Statut** : accepté (complété le 2026-10-10 : règles métier,
  informations financières, unicité des champs et partage portés)
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

### Règles métier du parc (`RuleAsset`)

Le moteur est porté (`nw_domain::rules`) avec la sémantique exacte du PHP,
mesurée sur la base de référence :

- Seules les règles **actives de l'entité racine** s'exécutent (la
  collection est construite sur l'entité 0), par rang. Une règle sans
  critère ne s'exécute jamais. Chaque règle lit l'entrée d'origine ; les
  actions des règles qui correspondent s'accumulent, la dernière l'emporte.
- **Condition « Ajout / Mise à jour » respectée** : une création n'exécute
  que les règles d'ajout, une modification (et les éléments connectés qui
  la suivent) que celles de mise à jour ; une règle sans condition (possible
  seulement hors formulaire) ne s'exécute jamais, comme dans la requête PHP
  (`condition & opération`). ITSM-NG ignorait cette condition :
  `assetBusinessRules` la passait dans le mauvais argument de
  `processAllRules`, et toutes les règles s'exécutaient à la création comme
  à la modification. Corrigé des deux côtés dans ce fork, avec un second
  défaut du PHP qui faisait le même effet : le cache des règles
  (`getCollectionDatas`) gardait celles d'une autre condition quand aucune
  règle ne correspondait à la nouvelle. Les tests atoum, qui validaient le
  comportement erroné, sont corrigés et complétés.
- Les règles lisent **le formulaire complet**, comme le poste l'interface
  classique : tous les champs (valeurs enregistrées et modifications), l'entité,
  `_itemtype` et `_auto` = 0. Elles s'appliquent donc aussi quand un autre
  champ change. Pour un élément connecté qui suit un ordinateur, elles ne
  lisent que les champs recopiés, comme `Computer::post_updateItem`.
- Critères de `RuleAsset` (statut, lieu, entité, utilisateur, fabricant,
  commentaire, contacts, type, inventaire automatique) et conditions : est,
  n'est pas, contient, ne contient pas, commence par, finit par, existe,
  n'existe pas, sous, pas sous, expressions régulières. Comparaisons à la
  manière de PHP 8 (`==` entre chaînes numériques, `empty`, casse). Les
  conditions textuelles comparent les noms affichés (nom complet des arbres
  avec `&gt;`, nom d'utilisateur selon les préférences de l'auteur).
- Actions `assign` et `regex_result` (dont l'utilisateur retrouvé par son
  identifiant de connexion avec `#0`...).
- Expressions régulières PCRE exécutées avec la crate `regex` : délimiteurs,
  modificateurs, correspondances octet par octet sans `u`, itération de
  `preg_match_all` (correspondance vide gardée après une correspondance non
  vide). Ce que les deux moteurs ne lisent pas pareil (références arrière,
  assertions, quantificateurs possessifs, `$` devant un saut de ligne
  final...) est refusé plutôt qu'exécuté différemment.

Vérification : règles avec expression régulière et utilisateur retrouvé,
condition « sous », règle sur les moniteurs, règle de création, appliquées
par le PHP (avec le formulaire complet) et par l'API à des jumeaux, avec
recopie vers un moniteur connecté et création d'un moniteur : historique,
valeurs et informations financières identiques. Après la correction, règles
d'ajout, de mise à jour, des deux et sans condition, à la création et à la
modification d'un ordinateur avec un moniteur connecté : mêmes valeurs et
même historique que le PHP corrigé.

### Informations financières (`glpi_infocoms`)

- Création avec l'élément quand `auto_create_infocoms` est actif : entité et
  partage de l'élément, alerte par défaut de l'entité (héritage compris),
  création inscrite dans l'historique de l'information financière.
- Dates remplies quand l'élément prend un statut (`autofill_*` =
  `STATUS_<id>`, héritage compris), à la création comme à la modification,
  y compris pour un élément connecté qui reçoit le statut de son ordinateur :
  information financière créée si besoin, sinon mise à jour. Comme le PHP,
  la date est historisée sur l'élément avec l'heure, seulement si la valeur
  enregistrée change (une date déjà au jour est réécrite sans historique).
  Une fin de garantie repoussée efface l'alerte de fin de garantie.

Vérification : création automatique, dates à la création et à la
modification, date ancienne, date déjà au jour, date vide, partage recopié :
identiques au PHP.

### Unicité des champs (`FieldUnicity`)

Portée comme `CommonDBTM::checkUnicity`, mesurée sur la base de
référence :

- Critères actifs du type, de l'entité de l'élément ou récursifs d'une
  entité parente ; seuls ceux de la première entité par identifiant
  décroissant s'appliquent (`getUnicityFieldsConfig`).
- Vérifiés sur **tout le formulaire** après les règles métier : modifier un
  autre champ d'un élément déjà en double est aussi refusé. Un élément
  connecté qui suit son ordinateur n'est vérifié que sur les champs
  recopiés ; refusé, il ne suit pas, l'ordinateur est quand même modifié.
- Un champ vide (une clé étrangère à 0) ou une valeur de la liste noire
  (`glpi_fieldblacklists`) écarte le critère. Les doublons sont cherchés par
  la base (sans casse ni espaces finaux), dans l'entité du critère et ses
  sous-entités s'il est récursif, gabarits exceptés, corbeille comprise.
- Doublon refusé : `409 duplicate` avec les champs du critère. Doublon
  seulement signalé : l'écriture a lieu et la réponse liste les doublons
  (`duplicates`), affichés sur la fiche. Dans les deux cas, l'événement du
  PHP est journalisé (type de l'objet, « essaie d'ajouter un élément qui
  existe déjà », avec les doublons).
- Les notifications ne sont pas portées : un doublon d'un critère qui en
  envoie, quand elles sont actives, renvoie vers l'interface classique.

### Partage avec les sous-entités

- Partager ou ne plus partager l'élément est recopié, sans historique,
  vers ses objets liés qui ont l'indicateur
  (`forwardEntityInformations`) : informations financières, ports réseau,
  réservations, systèmes d'exploitation, avec une nouvelle date de
  modification là où la table en a une. Les objets sans indicateur
  (logiciels installés, disques, machines virtuelles, noms réseau) ne
  changent pas, comme en PHP.
- Le formulaire PHP permet de ne plus partager sans vérification
  (`canUnrecurs` n'y est plus appelé). Le Rust garde cette vérification de
  GLPI : si des objets d'autres entités utilisent l'élément (logiciel
  installé ou machine virtuelle d'une autre entité, port d'une imprimante ou
  d'un matériel réseau relié à un élément d'une autre entité), le retrait est
  refusé (`409 sharing_in_use`).

Vérification : doublon refusé à la création, doublon signalé à la
modification, partage puis retrait avec informations financières, port
réseau (et son nom réseau), réservation, système d'exploitation et logiciel
liés : mêmes écritures et même historique que le PHP.

### Messages du journal des événements

Le PHP écrit les messages de `glpi_events` traduits, dans la langue de la
session. L'API fait de même :

- Les traductions viennent des fichiers `locales/*.mo` de l'application,
  extraits dans un catalogue compilé avec l'API (les textes utilisés, dans
  les 61 langues) selon les règles de Laminas et des fonctions `__`, `_n`,
  `_x` : forme plurielle de chaque langue, contexte, repli sur l'anglais.
  Une langue dont le fichier manque (`es_CL`, `ta`) s'écrit en anglais,
  comme en PHP. Un test vérifie que le catalogue correspond aux fichiers.
- Connexion réussie ou échouée : la langue du navigateur
  (`Accept-Language`, au sens de `Session::getPreferredLanguage`), sinon la
  langue par défaut, jamais celle de l'utilisateur (le PHP journalise avant
  d'ouvrir la session). L'identifiant journalisé est celui saisi, encodé
  comme le PHP (`<` en `&lt;`).
- Autres événements : la langue de la session (celle de l'utilisateur, sinon
  la langue par défaut). Un changement de langue dans les préférences est
  journalisé dans l'ancienne langue, le PHP ne rechargeant pas ses
  traductions avant d'écrire l'événement.
- Le message de doublon reprend `getUnicityErrorMessage` : libellés des
  options de recherche (`comment` donne « Commentaires du lieu »,
  `is_recursive` « Sous-entités », les arbres et l'entité un libellé vide,
  comme en PHP), lien vers l'élément avec l'ID si les ID sont affichés,
  `N/A` sans nom, titre encodé par `htmlentities`.
- Le changement de mot de passe dans les préférences, qui n'était pas
  journalisé, l'est désormais comme dans le formulaire PHP.
- Les surcharges locales de traductions (`files/_locales/core*`) ne sont
  pas lues : les messages restent ceux des fichiers de l'application.

Vérification : connexions avec plusieurs `Accept-Language`, création
refusée pour doublon, création, modification, corbeille et restauration en
français, doublon signalé en allemand sans les ID, changements de langue :
mêmes messages que le PHP, au caractère près (hors adresse IP, que le
serveur PHP de test ne fournit pas).

### Ce qui renvoie vers l'interface classique

| Motif (`details`) | Situation |
| --- | --- |
| `plugins` | Un plugin est activé : ses hooks ne s'exécuteraient pas |
| `business_rules` | Une règle métier utilise un critère ou une action de plugin, une condition textuelle sur « inventaire automatique » (libellé traduit), ou une expression régulière que les deux moteurs ne lisent pas pareil |
| `field_unicity` | Un critère d'unicité porte sur un champ que le formulaire n'a pas, ou pourrait s'appliquer aux informations financières ou aux cartes SIM écrites en même temps |
| `notifications` | Un doublon relève d'un critère qui envoie des notifications, et elles sont actives |
| `linked_items` | Partager l'élément changerait l'entité d'objets liés rangés dans une autre entité (données incohérentes : seule la recopie du partage est portée) |

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
- Une installation avec des plugins actifs continue d'écrire par
  l'interface classique, sans perte silencieuse ; chaque fonction portée
  plus tard retire un motif de refus.
- Les messages du journal des événements sont écrits dans la même langue
  que le PHP ; un texte ajouté au journal doit l'être aussi au catalogue
  de traductions.
- Le motif `plugins` disparaîtra avec la phase 2 (modèle d'extension
  natif) : en attendant, désactiver un plugin inutile ouvre les écritures
  natives.
