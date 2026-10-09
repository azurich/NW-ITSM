# Développer sur NW-ITSM

Pendant la migration, le dépôt contient deux applications qui partagent la
même base MariaDB (voir [l'ADR 0001](adr/0001-migration-progressive.md)) :

```
.                    application PHP existante (ITSM-NG), inchangée
├── backend/         API Rust /api/v2 (workspace Cargo)
│   ├── crates/nw-domain   règles métier pures : entités, droits, mots de passe
│   ├── crates/nw-db       requêtes SQL sur le schéma existant
│   ├── crates/nw-server   serveur HTTP Axum, sessions, routes, OpenAPI
│   └── openapi.json       contrat de l'API (généré, versionné)
├── frontend/        application React servie sous /app
├── dev/             environnement Docker de développement
└── docs/            décisions d'architecture et feuille de route
```

## Prérequis

- Rust 1.85 ou plus récent (`rustup`)
- Node.js 22 ou plus récent
- MariaDB 10.11 (ou Docker)
- PHP 8.1 ou plus récent et Composer, pour l'application PHP

## Option A : tout avec Docker

```sh
# 1. Base de données
docker compose -f dev/compose.yaml up -d db

# 2. Dépendances et installation de l'application PHP (une seule fois)
docker compose -f dev/compose.yaml run --rm legacy bin/console dependencies install
docker compose -f dev/compose.yaml run --rm legacy \
  bin/console db:install -H db -d itsm -u itsm -p itsm -L fr_FR -n

# 3. Tout le reste
docker compose -f dev/compose.yaml up -d --build
```

- Interface classique : <http://localhost:8000/>
- Nouvelle interface : <http://localhost:8000/app/>

Comptes créés par l'installation : `itsm`/`itsm` (Super-Admin),
`tech`/`tech`, `normal`/`normal` et `post-only`/`postonly`.

Pour modifier le frontend avec le rechargement à chaud, lancez Vite sur la
machine hôte et faites-le passer derrière le proxy :

```sh
cd frontend && NW_DEV_HOST=0.0.0.0 npm run dev
APP_UPSTREAM=host.docker.internal:5173 docker compose -f dev/compose.yaml up -d proxy
```

## Option B : sans Docker

Avec une base ITSM-NG installée (`bin/console db:install`) :

```sh
# API sur http://127.0.0.1:8080
cd backend
NW_DATABASE_URL=mysql://itsm:itsm@127.0.0.1:3306/itsm \
NW_SESSION_SECRET=une-cle-de-developpement-d-au-moins-32-octets \
NW_COOKIE_SECURE=false \
cargo run

# Frontend sur http://127.0.0.1:5173/app/ (redirige /api vers l'API)
cd frontend
npm install
npm run dev
```

## Configuration de l'API

| Variable | Défaut | Rôle |
|---|---|---|
| `NW_DATABASE_URL` | *(obligatoire)* | `mysql://utilisateur:mot-de-passe@hôte:port/base`, la base ITSM-NG |
| `NW_SESSION_SECRET` | *(obligatoire)* | Clé de signature des sessions, au moins 32 octets |
| `NW_BIND_ADDRESS` | `127.0.0.1:8080` | Adresse d'écoute |
| `NW_SESSION_TTL_MINUTES` | `480` | Durée de vie d'une session |
| `NW_COOKIE_SECURE` | `true` | Cookie réservé au HTTPS ; `false` en développement HTTP |
| `NW_STATIC_DIR` | *(aucun)* | Dossier `frontend/dist` à servir sous `/app` |
| `NW_DATABASE_MAX_CONNECTIONS` | `10` | Taille du pool de connexions |
| `RUST_LOG` | `info,sqlx=warn` | Niveau des journaux |

## Tests et vérifications

```sh
# Backend : format, lint, tests
cd backend
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
NW_TEST_DATABASE_URL=mysql://root:root@127.0.0.1:3306 cargo test --workspace
```

Les tests de l'API créent une base temporaire par test à partir de
`install/mysql/glpi-empty.sql` et de `backend/crates/nw-server/tests/fixtures/seed.sql`,
puis la suppriment. `NW_TEST_DATABASE_URL` désigne le **serveur** (sans nom
de base), avec un utilisateur autorisé à créer des bases. Sans cette
variable, ces tests sont ignorés.

```sh
# Frontend
cd frontend
npm run lint
npm run typecheck
npm test
npm run build
```

## Modifier l'API

Le fichier `backend/openapi.json` est le contrat entre le backend et le
frontend. Après une modification des routes ou des types de réponse :

```sh
cd backend && UPDATE_OPENAPI=1 cargo test -p nw-server --test openapi
cd ../frontend && npm run generate:api
```

Committez `backend/openapi.json` et `frontend/src/api/schema.ts` avec la
modification : la CI échoue s'ils ne correspondent pas au code.

## Ajouter un domaine

1. `nw-domain` : règles propres au domaine, sans accès à la base, avec des
   tests qui citent le code PHP reproduit.
2. `nw-db` : requêtes SQL, toujours restreintes par `EntityScope`.
3. `nw-server` : routes avec `Session::require(<droit>, <bit>)`, objets
   invisibles en `404`, types documentés avec utoipa, tests d'API dans
   `tests/`.
4. `frontend` : régénérer les types, ajouter les routes dans
   `src/router.tsx`, les textes dans `src/i18n/fr.ts` et `en.ts`, et un lien
   vers la page équivalente de l'interface classique.
