# ADR 0004 — Écritures du parc

- **Statut** : accepté (complété le 2026-10-10 : règles métier,
  informations financières, unicité des champs, partage, champs propres à
  chaque type, suppression définitive, gabarits, baies, châssis, PDU,
  équipements passifs, clusters et logiciels portés)
- **Date** : 2026-10-10

## Contexte

Le lot 2 ouvre les premières écritures métier : création, modification,
mise à la corbeille et restauration des ordinateurs, moniteurs,
imprimantes, matériels réseau, périphériques et téléphones, puis des
éléments du centre de données (baies, châssis, PDU, équipements passifs),
des clusters et des logiciels.

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
  `monitor`, `printer`, `networking`, `peripheral`, `phone`, `datacenter`
  pour les baies, châssis, PDU et équipements passifs, `cluster`,
  `software`). Comme
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

### Champs propres à chaque type

- Moniteurs : taille, connectique (microphone, haut-parleurs, Sub-D, BNC,
  DVI, pivot, HDMI, DisplayPort). Imprimantes : ports (série, parallèle,
  USB, Ethernet, Wi-Fi), mémoire, compteurs de pages initial et actuel.
  Matériels réseau : mémoire (Mio). Téléphones : alimentation, casque,
  haut-parleur. Moniteurs, imprimantes, périphériques et téléphones : type
  de gestion (unitaire ou globale).
- **Formulaires PHP rétablis** : en passant à Twig, ITSM-NG avait perdu
  des champs du formulaire GLPI (contact et numéro de contact des
  moniteurs ; commentaire, mémoire et compteurs de pages des imprimantes),
  qui ne pouvaient plus être modifiés. Ils sont rétablis, pour que les deux
  interfaces postent le même formulaire (que lisent les règles et
  l'unicité).
- Valeurs numériques comme le filtre d'entrée du PHP (`filterValues`) : la
  taille passe par `floatval` (« 21,50 » donne 21.5, journalisé « 21.5 »),
  la mémoire réseau et les compteurs par `intval`. Le PHP transforme
  silencieusement une saisie ambiguë (« 1.5 » devient 15 pour la mémoire
  réseau, « abc » devient 0 pour la taille) : l'API la refuse (`400`)
  plutôt que d'enregistrer autre chose que ce qui a été saisi. Une taille
  vide ou hors de `decimal(5,2)` est refusée, comme la base la refuse au
  PHP en mode strict.
- Comparaison comme le PHP : un nombre égal écrit autrement (« 21.5 » pour
  21.50, « 0512 » pour 512) n'est pas un changement.
- Un nouvel élément PHP a tous ses champs vides : à la création, une taille
  non saisie garde sa valeur par défaut et n'est ni lue par les règles ni
  par l'unicité (`prepareInputForAdd` la retire), les compteurs valent 0,
  le compteur actuel ne reprend pas le compteur initial.
- Le réglage « type de gestion » par type (`*_management_restrict`) n'a
  plus d'effet dans ITSM-NG : le script du formulaire réactive toujours la
  liste. L'API laisse donc choisir, comme le formulaire.
- **Ordre de l'historique** : les changements sont écrits dans l'ordre du
  formulaire PHP du type (entité d'abord), et les éléments connectés et
  composants suivent l'ordre des changements de l'ordinateur, comme
  `Computer::post_updateItem`. Une valeur posée par une règle garde sa
  place.
- Listes de choix : statuts visibles pour le type, groupes « éléments »
  pour le groupe, groupes « assignables » pour le groupe technique, comme
  GLPI, des deux côtés depuis la correction des listes du PHP (voir
  ci-dessous) ; les valeurs restent vérifiées en existence seulement,
  comme en PHP.

Vérification : formulaire de chaque type récupéré dans l'interface
classique et posté comme par un navigateur (champs dans l'ordre de la
page), et même modification par l'API sur un jumeau : mêmes valeurs et
mêmes lignes d'historique, dans le même ordre. Créations par le formulaire
« nouvel élément » : mêmes valeurs enregistrées.

### Suppression définitive (purge)

- Depuis la corbeille, avec le droit « Purger » du type, dans une entité
  active, et le droit de purger les informations financières si l'élément
  en a (`canPurgeItem`). Événement « purge un élément ».
- Supprimés avec l'élément, comme `CommonDBTM::deleteFromDB` : son
  historique ; ses informations financières, avec leur historique et
  leurs alertes ; ses connexions ; ses liens vers contrats, documents,
  articles de la base de connaissances (avec leur historique), projets,
  changements, problèmes, domaines, systèmes d'exploitation ; ses notes et
  ses objets de réservation (avec leurs alertes). Selon le type, comme le
  PHP : certificats (ordinateurs, imprimantes, matériels réseau,
  périphériques), grappes (ordinateurs, matériels réseau), disques,
  antivirus et machines virtuelles (ordinateurs). Ce que le PHP laisse
  (disques d'un autre type, applicatifs...) reste aussi.
- Renvoyé vers l'interface classique (`linked_data`) quand le PHP ferait
  plus que supprimer des lignes : ports réseau (noms, adresses, liens),
  composants, logiciels installés et licences, tickets (mis à jour, mis à
  la corbeille s'ils n'avaient que cet élément), baies et châssis,
  cartouches, réservations en cours, analyse d'impact, et connexions quand
  le nettoyage à la déconnexion est activé (`is_*_autoclean`,
  `state_autoclean_mode` : l'élément déconnecté est modifié comme par un
  formulaire).

Vérification : sur deux copies de la base de référence, un moniteur et un
ordinateur de la corbeille avec toutes les sortes de liens portés (et
quelques-uns que le PHP laisse), purgés par le formulaire PHP sur l'une et
par l'API sur l'autre : bases identiques, mêmes événements.

### Gabarits

- Un gabarit est un élément marqué `is_template`, avec un nom de gabarit
  (`template_name`). Routes à part (`/assets/{type}/templates`) : un
  gabarit n'apparaît ni dans les listes ni dans les routes des éléments, et
  inversement.
- **Liste** comme `CommonDBTM::listTemplates` : gabarits des entités
  actives et gabarits partagés de leurs parentes, par nom ; lisible avec le
  droit de lecture ou de création du type. Modifier un gabarit demande le
  droit de création dans l'interface classique (lien de la liste) ; l'API
  applique les droits de ses formulaires : `CREATE` pour en ajouter,
  `UPDATE` pour en modifier un, `PURGE` pour en supprimer un.
- **Création et modification** comme le formulaire « gabarit »
  (`withtemplate=1`) : mêmes règles métier qu'un élément, mais pas
  d'unicité des champs (`checkUnicity` ignore les gabarits). À la création,
  le statut ne remplit pas de date (il en remplit à la modification, comme
  le PHP), l'information financière automatique est créée. Le nom du
  gabarit est historisé (option 65 pour les ordinateurs et matériels
  réseau, 61 sinon), juste après l'entité.
- **Suppression** : définitive d'emblée (pas de corbeille pour un gabarit),
  avec les mêmes liens et refus que la purge d'un élément.
- **Création d'un élément depuis un gabarit** comme le formulaire
  `withtemplate=2` (`CommonDBTM::clone`, `Clonable::post_clone`) : dans
  l'entité du gabarit, qui doit être active ; les champs non donnés
  reprennent les valeurs du gabarit (un texte vide du gabarit est posté
  vide), ses autres colonnes sont recopiées (inventaire automatique, coût
  total de possession...). Règles métier et unicité des champs s'appliquent
  (l'unicité sur toutes les colonnes recopiées, comme le PHP), pas de
  création automatique d'information financière, dates du statut remplies.
  Puis, dans l'ordre du PHP : l'information financière du gabarit est
  recopiée (avec l'alerte par défaut de l'entité, sauf si le statut en a
  déjà créé une), les liens vers les contrats (sauf contrat déjà au maximum
  d'éléments, `max_links_allowed`) et les documents (auteur, position et
  date conservés), avec l'historique des deux côtés (`Nom (id)`, `N/A` pour
  un nom vide).
- Renvoyé vers l'interface classique (`linked_data`) quand le gabarit a des
  liens dont la recopie n'est pas portée : système d'exploitation,
  composants, ports réseau, disques, logiciels et licences, connexions,
  notes, articles de la base de connaissances, cartouches.
- **Correction côté PHP** : `CommonDBTM::clone` ajoutait « (copie) » au nom
  saisi pour un élément créé depuis un gabarit (le gabarit était traité
  comme un élément dupliqué). Le nom donné est conservé (test atoum
  `tests/units/Computer.php`) ; la duplication d'un élément garde son
  suffixe.
- Non rétabli : la numérotation automatique des gabarits (`<###>` dans un
  champ) n'est plus appliquée par les formulaires Twig d'ITSM-NG ; un tel
  motif est donc recopié tel quel, des deux côtés.

Vérification : sur des copies identiques de la base de référence, un
gabarit d'ordinateur avec information financière, deux contrats (dont un
plein) et deux documents (dont un sans nom) ; création d'un élément depuis
ce gabarit (avec et sans statut qui remplit une date), création,
modification (avec dates du statut), retrait du partage et suppression d'un
gabarit, et création d'un moniteur depuis un gabarit, faites par les
formulaires PHP postés comme par un navigateur puis par l'API : bases
identiques, mêmes événements.

### Baies, châssis, PDU, équipements passifs et clusters

- Routes `/assets/racks`, `/assets/enclosures`, `/assets/pdus`,
  `/assets/passive-dc-equipments` et `/assets/clusters`, avec les mêmes
  écritures que les autres types (création, modification, corbeille,
  purge, gabarits) et les événements de leurs formulaires PHP (types
  `racks`, `enclosure`, `pdus`, `passivedcequipment`, `cluster`).
- **Champs** : ni utilisateur, ni groupe, ni contact ; pas de type pour
  les châssis. Les clusters n'ont ni lieu, ni numéros de série et
  d'inventaire, ni fabricant, ni modèle, mais un UUID, une version et une
  source de mise à jour. Baies : salle serveur, position dans la salle,
  orientation de la porte, nombre d'unités (1 à 100), largeur, hauteur,
  profondeur, puissances maximale et mesurée, poids maximal, couleur de
  fond. Châssis : nombre d'alimentations.
- **Historique** : seuls les champs ayant une option de recherche y
  entrent, comme le PHP (salle 7, unités 8, UUID 5 des clusters...). N'y
  entrent pas : dimensions, puissances, orientation, position et couleur
  des baies, alimentations des châssis, version, type et source de mise à
  jour des clusters, commentaire des PDU, commentaire et partage des
  équipements passifs. Les modèles de ces types peuvent être NULL :
  « aucun » s'écrit alors `&nbsp; ()`.
- **Pas de règles métier** (ces types ne sont pas dans `asset_types`) ;
  unicité des champs pour les baies, châssis, PDU et clusters (pas pour
  les équipements passifs, que `FieldUnicity` ne propose pas).
- **Nouvel élément** comme le formulaire PHP : 42 unités, porte au nord,
  position « 0 », fond `#F5B7B1`, une alimentation ; une dimension vide est
  enregistrée 0 (le PHP se connecte sans mode SQL strict).
- **Position dans la salle** (`Rack::prepareInputForAdd` /
  `prepareInputForUpdate`) : avec une salle et une position non nulle, une
  autre baie hors corbeille (gabarits compris) déjà à cette place refuse
  l'écriture (`409 position_unavailable`). Les places proposées
  (`/assets/racks/positions`, comme `ajax/dcroom_size.php`) sont celles de
  la grille visible de la salle que n'occupe aucune autre baie, la place
  actuelle comprise, nommées par colonne en lettres (A, B... AA) et rangée.
- **Événement d'une modification refusée** : le formulaire PHP écrit
  « updates an item » même quand la position ou l'unicité des champs
  refuse la modification ; l'API aussi, pour tous les types.
- **Informations financières** (pas pour les clusters) : création
  automatique et dates du statut comme les autres types, mais dans
  l'entité racine, non partagée, avec l'alerte par défaut de l'entité de
  l'élément : ces classes ne transmettent pas leur entité
  (`$forward_entity_to` vide, `CommonDBChild::addNeededInfoToInput`). Pour
  la même raison, changer le partage ne recopie rien.
- **Purge** : supprimés avec l'élément, selon le type comme le PHP :
  information financière (sauf clusters), liens vers les contrats (sauf
  équipements passifs), les documents (clusters), les changements et les
  problèmes. Renvoyé vers l'interface classique (`linked_data`) : tickets,
  ports réseau (châssis, PDU, clusters), analyse d'impact (sauf
  équipements passifs), composants (châssis), place dans une baie ou un
  châssis, et contenu propre : éléments et PDU d'une baie, éléments d'un
  châssis, prises et baies d'un PDU, éléments d'un cluster.
- **Gabarits** : pour tous sauf les clusters (sans colonne de gabarit).
  Baies et équipements passifs ne sont pas `Clonable` : un élément créé
  depuis leur gabarit n'en recopie aucun lien (ni information financière,
  ni contrat). Châssis et PDU recopieraient leurs éléments, prises,
  composants et ports : renvoyés vers l'interface classique quand le
  gabarit en a. Les champs non donnés reprennent les valeurs du gabarit
  telles que le formulaire les affiche, donc les poste : modèle NULL
  posté 0, orientation hors liste postée « nord », 0 unité postée 42, 0
  alimentation postée 1.
- **Corrections côté PHP** : le champ « numéro d'inventaire » des
  formulaires PDU et équipement passif affichait le numéro de série ;
  enregistrer le formulaire remplaçait donc le numéro d'inventaire par le
  numéro de série (tests `tests/functional/PDU.php` et
  `PassiveDCEquipment.php`).
- Remarque : une modification par l'API n'écrit que les champs donnés.
  Le formulaire PHP, lui, renvoie ce qu'il affiche. Il réécrit donc, à
  chaque enregistrement, les valeurs qu'il ne sait pas afficher, venues
  d'un autre outil ou d'un import :
  - modèle NULL écrit 0, avec une ligne d'historique ;
  - 0 unité écrit 42 ;
  - orientation inconnue écrite « nord ».

  L'API ne reproduit pas cet effet de l'affichage.

Vérification : sur des copies identiques de la base de référence (une
salle de 3 × 2, deux baies placées, un châssis, un PDU, un équipement
passif, un cluster, un contrat lié), faites par les formulaires PHP postés
comme par un navigateur puis par l'API, avec création automatique des
informations financières et date d'achat au statut :
- création d'une baie (salle, position, type, statut, largeur), d'un châssis,
  d'un PDU, d'un équipement passif et d'un cluster ;
- modification d'une baie (unités, modèle, couleur, statut), d'un
  châssis, d'un PDU, d'un équipement passif (partage) et d'un cluster
  (UUID, version, type) ;
- refus d'une position prise et d'un doublon ;
- purge d'une baie liée à un contrat, d'un cluster, d'un PDU et d'un
  châssis ;
- création, purge et utilisation de gabarits de baie, châssis, PDU et
  équipement passif.

Résultat : bases identiques, mêmes événements.

### Logiciels (le logiciel lui-même)

Première étape des logiciels : l'élément `Software`, sans ses versions,
installations ni licences (étapes suivantes).

- Route `/assets/softwares`, avec les écritures des autres types
  (création, modification, corbeille, purge, gabarits), le droit
  `software` et les événements du formulaire PHP (type `software`).
- **Champs** : éditeur (le fabricant), catégorie, lieu, utilisateur,
  groupe, technicien et groupe responsables, associable à un ticket, mise
  à jour d'un autre logiciel (et lequel), commentaire. Ni type, ni modèle,
  ni statut, ni numéros de série et d'inventaire, ni contact. Les licences
  valides (`is_valid`, calculé d'après les licences) sont affichées, pas
  modifiables.
- **Historique** comme les options du PHP : catégorie 62, associable à un
  ticket 61, éditeur 23, commentaire 16, gabarit 64... Le logiciel mis à
  jour (`softwares_id`) et l'indicateur de mise à jour n'y entrent pas.
- **Nouveau logiciel** comme `getEmpty` : associable à un ticket selon le
  réglage `default_software_helpdesk_visible` (lu par
  `/assets/softwares/defaults`, pour que le formulaire l'affiche).
- **`prepareInputForAdd` / `prepareInputForUpdate`** : sans l'indicateur
  de mise à jour, le logiciel mis à jour est effacé. Un nouveau logiciel
  sans catégorie prend celle des règles de catégorie
  (`RuleSoftwareCategory`) : aucune sans règle active ; avec une règle
  active, la création est renvoyée vers l'interface classique
  (`business_rules`), ces règles n'étant pas portées. Pas de règles
  métier du parc (le logiciel n'est pas dans `asset_types`).
- Unicité des champs, information financière (créée automatiquement, sans
  date de statut : le logiciel n'a pas de statut) comme les autres types.
- **Partage** : recopié à son information financière, ses objets de
  réservation et ses versions (`$forward_entity_to`, sans historique, avec
  `date_mod`). Il ne peut plus être retiré tant qu'un logiciel d'une autre
  entité est une mise à jour de celui-ci (`canUnrecurs`).
- **Purge** : ses liens vers projets, contrats, documents, notes, base de
  connaissances, changements, problèmes et domaines, son information
  financière et ses objets de réservation sont supprimés ; un logiciel qui
  en était la mise à jour l'oublie (`cleanRelationData` : `softwares_id` à
  0, nouvelle `date_mod`). Renvoyé vers l'interface classique tant qu'il a
  des versions (purgées avec leurs installations et l'historique des
  matériels), des licences (mises à la corbeille), des tickets, des
  réservations en cours ou une analyse d'impact.
- **Gabarits** : un logiciel créé depuis un gabarit en recopie
  l'information financière, les contrats et les documents ; les articles
  de la base de connaissances renvoient vers l'interface classique.
- **Correction côté PHP** : le formulaire Twig du logiciel avait perdu son
  commentaire, qui ne pouvait plus être modifié ; il est rétabli (test
  `tests/functional/Software.php`).

Vérification : sur des copies identiques de la base de référence (deux
logiciels de Paris, l'un mise à jour de l'autre, un contrat), faites par le
formulaire PHP posté comme par un navigateur puis par l'API :
- création (avec et sans catégorie, information financière automatique) ;
- modification (historique, indicateur de mise à jour retiré) ;
- partage recopié aux versions ;
- corbeille, restauration et purge (liens, logiciel qui l'oublie) ;
- création, et création depuis un gabarit (information financière et
  contrat recopiés, catégorie vide) ;
- refus d'unicité.

Résultat : bases identiques, mêmes événements.

### Correction côté PHP : conditions des listes des formulaires

Les formulaires Twig d'ITSM-NG n'appliquaient aucune des conditions de
leurs listes de choix (statuts visibles pour le type, groupes
« éléments » ou « assignables », valeurs actives, élément lui-même exclu
de ses parents possibles...) :

- `expandSelect` ne lisait que la clé `condition`, alors que la plupart
  des formulaires déclarent `conditions` ;
- dans le navigateur, select2 recharge la liste par
  `ajax/getDropdownValue.php`, qui n'accepte (à raison) qu'une condition
  enregistrée en session : le tableau envoyé était ignoré, quelle que soit
  la clé ;
- `getOptionForItems` passait aussi ses conditions en tableau, ignorées ;
  son filtre d'entité était un texte JSON invalide.

Correction : les deux clés sont lues ; la condition est enregistrée en
session (`Dropdown::addNewCondition`, comme `Dropdown::show`) et sa clé
envoyée au navigateur ; ses colonnes sont préfixées par la table de la
liste (sinon ambiguës quand les traductions des listes sont jointes) ;
`entities_id` reste une restriction d'entité (valeurs de l'entité et
valeurs récursives de ses parentes), pas une égalité.
`ajax/dropdownAllItems.php`, qui passait une condition reçue du
navigateur, n'accepte plus qu'une clé de session (sinon le navigateur
pourrait injecter des critères). Les conditions fausses ou absentes des
formulaires du parc sont rétablies comme dans GLPI :
- statuts des baies (`is_visible_computer`), des clusters
  (`is_visible_line`) et des matériels réseau (aucune) ;
- groupe des imprimantes et téléphones (« assignables » au lieu de
  « éléments ») ;
- groupes des ordinateurs et matériels réseau (aucune) ;
- statut du critère de recherche des châssis.

Tests `tests/functional/Dropdown.php` : listes filtrées dans la page et
par AJAX pour les deux clés, `getOptionForItems` (condition et entité),
et listes des formulaires de chaque type du parc ; ils échouent sans la
correction.

### Ce qui renvoie vers l'interface classique

| Motif (`details`) | Situation |
| --- | --- |
| `plugins` | Un plugin est activé : ses hooks ne s'exécuteraient pas |
| `business_rules` | Une règle métier utilise un critère ou une action de plugin, une condition textuelle sur « inventaire automatique » (libellé traduit), ou une expression régulière que les deux moteurs ne lisent pas pareil |
| `field_unicity` | Un critère d'unicité porte sur un champ que le formulaire n'a pas, ou pourrait s'appliquer aux informations financières (créées, datées ou recopiées d'un gabarit) ou aux cartes SIM écrites en même temps |
| `notifications` | Un doublon relève d'un critère qui envoie des notifications, et elles sont actives |
| `linked_data` | Une purge toucherait des données liées dont la suppression n'est pas portée (ports réseau, composants, logiciels, tickets, place ou contenu d'une baie, d'un châssis, d'un PDU ou d'un cluster, cartouches, réservations en cours, impact, connexions avec nettoyage à la déconnexion), ou le gabarit d'une création a des liens dont la recopie n'est pas portée (système d'exploitation, composants, ports réseau, disques, logiciels, connexions, notes, base de connaissances, cartouches, éléments d'un châssis, prises d'un PDU) |
| `linked_items` | Partager l'élément changerait l'entité d'objets liés rangés dans une autre entité (données incohérentes : seule la recopie du partage est portée) |

Une modification qui ne change rien, ou une mise à la corbeille d'un
élément déjà à la corbeille, n'est jamais refusée.

### Hors périmètre de cette étape

- Les onglets (composants, connexions, logiciels, informations
  financières) restent dans l'interface classique.
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
