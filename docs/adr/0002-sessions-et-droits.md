# ADR 0002 — Sessions, droits et entités côté Rust

- **Statut** : accepté
- **Date** : 2026-10-09

## Contexte

L'API Rust doit authentifier les utilisateurs existants et appliquer
exactement les mêmes droits que l'application PHP, sur la même base. Côté
PHP, tout est stocké dans la session (`$_SESSION`) : profil actif, droits,
entités actives.

## Décision

### Connexion

- `POST /api/v2/auth/login` vérifie les comptes **locaux** (`authtype = 1`)
  comme `Auth::connection_db` : bcrypt, et les anciens formats MD5, SHA1 et
  SHA1 salé. Un identifiant inconnu coûte le même temps qu'un mot de passe
  faux (pas de découverte des comptes par le temps de réponse).
- Mêmes refus que le PHP : compte inactif, supprimé, hors de sa période de
  validité, mot de passe expiré ou verrouillé (`password_expiration_*`),
  aucun profil utilisable.
- LDAP, CAS, OIDC et IMAP restent gérés par l'interface classique jusqu'à
  leur portage (cœur, lot 1).

### Session

- Jeton signé (HS256, clé `NW_SESSION_SECRET`) contenant l'utilisateur, le
  profil actif et l'expiration (`NW_SESSION_TTL_MINUTES`, 8 h par défaut).
- Transmis au navigateur dans un cookie `nw_session` `HttpOnly`,
  `SameSite=Lax` et `Secure` (sauf `NW_COOKIE_SECURE=false` en
  développement). Les clients d'API peuvent l'envoyer en
  `Authorization: Bearer`.
- **À chaque requête**, l'utilisateur, ses profils, ses droits et ses
  entités sont relus en base : désactiver un compte ou retirer un profil
  coupe l'accès immédiatement, sans liste de révocation.

### Droits et entités (`nw-domain`)

- Choix du profil : profil par défaut de l'utilisateur, sinon le premier
  par ordre alphabétique (`Session::init`).
- Entités actives : même algorithme que `Session::changeProfile` et
  `Session::changeActiveEntities` (entité par défaut, puis sous-arbre
  récursif, puis toutes les entités du profil).
- Visibilité d'un objet : son entité est active, ou c'est une entité
  parente et l'objet est partagé avec les sous-entités (`is_recursive = 1`),
  comme `getEntitiesRestrictCriteria`.
- Droits : masques de bits de `glpi_profilerights`. Un profil `helpdesk` ne
  garde que les droits de `Profile::$helpdesk_rights`. Un droit est accordé
  si au moins un de ses bits est présent (`Session::haveRight`).
- Un objet invisible renvoie `404` (pas `403`), pour ne pas révéler son
  existence.

## Conséquences

- La session Rust est indépendante de la session PHP : il faut se connecter
  une fois de chaque côté. Un pont de connexion unique (SSO entre les deux)
  est prévu dans le cœur, lot 1.
- La déconnexion supprime le cookie mais ne révoque pas un jeton copié
  ailleurs avant son expiration. Une liste de révocation pourra être ajoutée
  si nécessaire.
- Le rechargement à chaque requête coûte quatre petites requêtes SQL. Un
  cache court pourra être ajouté si les mesures le justifient.
- Pas encore de limitation des tentatives de connexion (l'application PHP
  n'en a pas non plus) : prévu dans le cœur, lot 1.
