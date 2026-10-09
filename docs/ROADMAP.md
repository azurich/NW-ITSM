# Feuille de route NW-ITSM

Objectif : remplacer progressivement ITSM-NG (PHP, jQuery, Twig) par un
backend Rust et une interface React, sur la même base de données, jusqu'à
l'arrêt complet du PHP. L'approche est décrite dans
[l'ADR 0001](adr/0001-migration-progressive.md).

L'ordre est fixé :

1. **Phase 0 — Fondations**
2. **Phase 1 — Le cœur de l'application, sans les plugins**
3. **Phase 2 — Les plugins, avec un nouveau modèle d'extension**

---

## Phase 0 — Fondations ✅

Objectif : valider toute la chaîne sur un domaine simple, en production
possible à côté de l'interface classique.

- [x] Workspace Rust `backend/` : `nw-domain`, `nw-db`, `nw-server`
- [x] Connexion des comptes locaux, sessions, droits et entités identiques au PHP
- [x] Premier domaine en lecture : ordinateurs (liste paginée, recherche, tri, fiche)
- [x] Contrat OpenAPI `backend/openapi.json` et types TypeScript générés
- [x] Application React `frontend/` servie sous `/app` par le binaire Rust
- [x] Tests : règles métier (unitaires), API (sur MariaDB avec le vrai schéma), frontend
- [x] Environnement de développement (`dev/compose.yaml`, proxy Caddy), image Docker
- [x] CI GitHub Actions pour le backend et le frontend

## Phase 1 — Le cœur, sans les plugins

Chaque lot suit la même « définition de terminé » :

- API `/api/v2` documentée dans le contrat OpenAPI ;
- écrans React équivalents ou meilleurs que l'interface classique ;
- droits et entités vérifiés par des tests sur MariaDB ;
- **tests de parité** : pour un même jeu de données et un même utilisateur,
  le Rust renvoie les mêmes objets que le PHP ;
- liens croisés entre l'interface classique et la nouvelle ;
- écritures : mêmes effets de bord que le PHP (historique `glpi_logs`,
  notifications, règles).

### Lot 1 — Socle transverse

- [ ] Connexion unique entre l'interface classique et la nouvelle
      (jeton court signé émis par le PHP, échangé par le Rust)
- [ ] Changement de profil et d'entité active
- [ ] Authentification LDAP, puis OIDC, CAS et IMAP
- [ ] Limitation des tentatives de connexion, journal des connexions (`glpi_events`)
- [ ] Re-hachage bcrypt des anciens mots de passe à la connexion, changement de mot de passe
- [ ] Écriture de l'historique (`glpi_logs`) au format du PHP
- [ ] Préférences utilisateur, traductions (conversion des fichiers `.po`, Weblate)
- [ ] Compatibilité avec les données chiffrées (`glpicrypt.key`, XChaCha20-Poly1305)

### Lot 2 — Parc

- [ ] Ordinateurs : écriture (création, modification, corbeille, restauration)
- [ ] Autres types : moniteurs, imprimantes, matériel réseau, périphériques, téléphones, baies
- [ ] Logiciels, licences, versions installées
- [ ] Liaisons entre objets (`itemtype` / `items_id`), composants, ports réseau
- [ ] Gestion financière (`infocoms`), contrats, fournisseurs, documents
- [ ] **Inventaire natif** : réception des agents (GLPI-Agent et/ou OCS),
      aujourd'hui assurée par un plugin, et indispensable au cœur

### Lot 3 — Assistance

- [ ] Tickets en lecture : liste, fiche, fil (suivis, tâches, solutions, validations)
      avec les droits propres aux tickets (mes tickets, mon groupe, tous...)
- [ ] Tickets en écriture, d'abord via l'API REST PHP existante, puis en natif
- [ ] Moteur de règles, SLA/OLA et calendriers
- [ ] Problèmes, changements, projets
- [ ] Base de connaissances, catalogue de services
- [ ] Interface self-service (profils `helpdesk`)

### Lot 4 — Tâches de fond

- [ ] Planificateur (remplace `CronTask`)
- [ ] Notifications (e-mail, file d'attente)
- [ ] Collecteur d'e-mails (création de tickets par e-mail)

### Lot 5 — Administration et recherche

- [ ] Administration : utilisateurs, groupes, profils, entités, intitulés, configuration
- [ ] Recherche avancée et recherches sauvegardées, tableaux de bord, statistiques, rapports
      (sans reprendre tel quel le moteur `Search` et ses identifiants numériques)
- [ ] Outil d'import depuis ITSM-NG 2.1.x (au lieu de porter toutes les migrations depuis GLPI 0.72)

### Fin de la phase 1

- [ ] Arrêt de l'interface PHP
- [ ] Modernisation du schéma : clés étrangères, `utf8mb4`, suppression des caches d'arbre

## Phase 2 — Les plugins

Les plugins PHP actuels ne peuvent pas fonctionner avec le backend Rust.

1. **Fonctions natives** pour les plugins les plus utilisés : champs
   personnalisés, formulaires, import CSV, tags, escalade, rapports.
2. **Intégrations externes** : webhooks sur les événements
   (`ticket.created`, `asset.updated`...) et API REST, utilisables depuis
   n'importe quel langage.
3. **Extensions dans le serveur** : plugins WebAssembly isolés, avec des
   permissions explicites et une interface stable ; extensions d'interface
   décrites en données (onglets, champs, formulaires).

Non retenu : des plugins Rust compilés en bibliothèques dynamiques
(interface binaire instable, un plugin défaillant ferait tomber le serveur).
