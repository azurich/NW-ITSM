# ADR 0001 — Migration progressive vers Rust et React

- **Statut** : accepté
- **Date** : 2026-10-09

## Contexte

NW-ITSM est un fork d'ITSM-NG 2.1.7 (lui-même issu de GLPI 9.5). Le code
existant représente environ 306 000 lignes de PHP dans `inc/`, 576 pages
`front/`, 110 points d'entrée `ajax/` et 356 tables MariaDB sans clé
étrangère. L'objectif est d'obtenir un backend en Rust et une interface en
React, sans PHP à terme.

Une réécriture complète d'un seul tenant prendrait des années sans rien
livrer, pendant que l'application actuelle continuerait d'évoluer : c'est le
scénario d'échec le plus courant pour ce type de projet.

## Décision

1. **Migration progressive (« strangler fig »).** Les deux applications
   tournent côte à côte derrière un même proxy et une même origine :
   - `/api/v2/*` → API Rust (`backend/`) ;
   - `/app/*` → application React (`frontend/`), servie par le binaire Rust ;
   - tout le reste → application PHP existante, inchangée.

2. **Une seule base de données, schéma inchangé.** Tant que le PHP s'en
   sert, le schéma appartient au PHP : le Rust le lit (puis l'écrit) tel
   quel, avec ses conventions (`0` pour « aucun », couples
   `itemtype`/`items_id`, entités en arbre). La modernisation du schéma
   (clés étrangères, `utf8mb4`...) viendra après l'arrêt du PHP.

3. **Migration par domaine, de bout en bout.** Chaque domaine migré apporte
   son API, ses écrans et ses tests, en commençant par la lecture. Tant
   qu'un écran n'est pas migré, l'application React renvoie vers la page
   équivalente de l'interface classique (et inversement).

4. **Même sémantique que le PHP.** Droits, entités et règles d'accès sont
   reproduits à l'identique dans `nw-domain`, avec des tests qui citent le
   code PHP d'origine. Une divergence est un défaut de sécurité.

5. **Ordre de travail** (voir [la feuille de route](../ROADMAP.md)) :
   fondations, puis cœur de l'application sans les plugins, puis nouveau
   modèle de plugins.

## Choix techniques

| Sujet | Choix | Raison |
|---|---|---|
| Serveur HTTP | Axum (Tokio) | Écosystème Tower (traces, en-têtes, fichiers statiques), extracteurs typés |
| Base de données | SQLx, requêtes écrites à la main | Un ORM s'adapte mal à un schéma sans clés étrangères et polymorphe |
| Contrat d'API | utoipa → `backend/openapi.json` → `openapi-typescript` | Types partagés de bout en bout ; la CI échoue si le contrat et le code divergent |
| Organisation Rust | `nw-domain` (règles pures), `nw-db` (SQL), `nw-server` (HTTP) | Les règles métier se testent sans base ni serveur |
| Frontend | React 19, TypeScript, Vite, TanStack Router et Query, Tailwind | Standard actuel, routage et cache typés |
| Traductions | i18next (FR/EN pour l'instant) | Les 123 fichiers `.po` existants pourront être convertis (Weblate conservé) |
| Licence | GPL-2.0-or-later | Le code Rust reprend la logique du code PHP : c'est une œuvre dérivée |

## Conséquences

- Chaque étape est livrable et réversible : on peut toujours revenir à
  l'interface classique.
- Le Rust doit produire les mêmes effets de bord que le PHP lorsqu'il écrira
  (historique `glpi_logs`, notifications, règles, SLA). Tant qu'un domaine
  n'est pas entièrement porté, ses écritures complexes peuvent passer par
  l'API REST PHP existante.
- Les plugins PHP ne fonctionneront pas avec le backend Rust : ils seront
  remplacés par des fonctions natives et un nouveau modèle d'extension
  (phase 2).
- Le dépôt garde l'arborescence PHP en place, ce qui permet de continuer à
  intégrer les correctifs d'ITSM-NG pendant la transition.
