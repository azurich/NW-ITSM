# ADR 0002 — Sessions, droits et entités côté Rust

- **Statut** : accepté (complété le 2026-10-10, lot 1)
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
  aucun profil utilisable. Un mot de passe expiré peut être remplacé
  directement (`POST /api/v2/auth/password/expired`).
- **Limitation des tentatives** (absente du PHP) : 5 échecs pour un même
  identifiant depuis une même adresse, ou 20 échecs depuis une adresse, en
  15 minutes, bloquent les essais suivants (`429` et `Retry-After`).
  Compteurs en mémoire, par instance de l'API. Derrière un proxy,
  `NW_TRUST_PROXY=true` prend l'adresse dans `X-Forwarded-For`.
- Succès et échecs sont journalisés dans `glpi_events` et `last_login` est
  mis à jour, comme le PHP.
- Un ancien hachage MD5 ou SHA1 est **remplacé par bcrypt** à la connexion
  (le PHP le prévoit mais ne le fait pas, sa validation refusant un mot de
  passe identique à l'ancien).
- Les comptes LDAP, CAS, OIDC et IMAP passent par l'interface classique et
  le pont de connexion ([ADR 0003](0003-pont-de-connexion.md)).

### Session

- Jeton signé (HS256, clé `NW_SESSION_SECRET`) contenant l'utilisateur, le
  profil actif, les entités sélectionnées, la date du dernier changement de
  mot de passe et l'expiration (`NW_SESSION_TTL_MINUTES`, 8 h par défaut).
- Transmis au navigateur dans un cookie `nw_session` `HttpOnly`,
  `SameSite=Lax` et `Secure` (sauf `NW_COOKIE_SECURE=false` en
  développement). Les clients d'API peuvent l'envoyer en
  `Authorization: Bearer`.
- **À chaque requête**, l'utilisateur, ses profils, ses droits, ses entités
  et ses préférences d'affichage sont relus en base : désactiver un compte
  ou retirer un profil coupe l'accès immédiatement.
- **Changer de mot de passe ferme les autres sessions** : un jeton dont la
  date de changement ne correspond plus est refusé. La session courante
  reçoit un nouveau cookie.
- Changer de profil ou d'entités émet aussi un nouveau cookie.

### Droits et entités (`nw-domain`)

- Choix du profil : profil par défaut de l'utilisateur, sinon le premier
  par ordre alphabétique (`Session::init`).
- Entités à la connexion et au changement de profil : même algorithme que
  `Session::changeProfile` (entité par défaut, puis sous-arbre récursif,
  puis toutes les entités du profil).
- Sélection d'entités (`PUT /api/v2/auth/session/entity`) : une entité
  n'est acceptée que si le profil la voit réellement, et avec ses
  sous-entités seulement sous une habilitation récursive. C'est **plus
  strict** que `Session::changeActiveEntities`, qui accepte aussi une
  sous-entité d'une habilitation non récursive.
- Visibilité d'un objet : son entité est active, ou c'est une entité
  parente et l'objet est partagé avec les sous-entités (`is_recursive = 1`),
  comme `getEntitiesRestrictCriteria`.
- Droits : masques de bits de `glpi_profilerights`. Un profil `helpdesk` ne
  garde que les droits de `Profile::$helpdesk_rights`. Un droit est accordé
  si au moins un de ses bits est présent (`Session::haveRight`).
- Un objet invisible renvoie `404` (pas `403`), pour ne pas révéler son
  existence.

### Écritures et historique

- Les modifications passent par une transaction qui écrit aussi
  l'historique `glpi_logs` au format de `Log::history` (option de recherche
  du champ, ancienne et nouvelle valeur limitées à 180 caractères, auteur
  « nom (id) »). Le format a été comparé à une écriture faite par le PHP.
- Le changement de mot de passe suit `User::prepareInputForUpdate`
  (`password_last_update` et `date_mod`, pas d'historique) et la politique
  `Config::validatePassword`, reproduite octet par octet (`_` n'est pas un
  symbole, une lettre accentuée en est un).

## Conséquences

- La déconnexion supprime le cookie mais ne révoque pas un jeton copié
  ailleurs avant son expiration (sauf changement de mot de passe).
- Le rechargement à chaque requête coûte quelques petites requêtes SQL. Un
  cache court pourra être ajouté si les mesures le justifient.
- Les messages de `glpi_events` écrits par l'API sont en anglais (texte
  source des traductions), ceux du PHP dans la langue de la session.
