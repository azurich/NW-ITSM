# ADR 0003 — Pont de connexion entre les deux interfaces

- **Statut** : accepté
- **Date** : 2026-10-10

## Contexte

Pendant la migration, les utilisateurs passent sans cesse de l'interface
classique (PHP) à la nouvelle (React + API Rust). Les deux ont leur propre
session : sans pont, il faudrait se connecter deux fois. De plus, seule
l'interface classique sait authentifier les comptes LDAP, CAS, OIDC ou IMAP.

## Décision

Un **jeton à usage court**, signé avec une clé partagée, fait passer
l'utilisateur d'une interface à l'autre, dans les deux sens.

- **Clé** : `config/nw-bridge.key` (au moins 32 octets, jamais versionnée),
  lue par le PHP (`src/NwBridge.php`) et par l'API (`NW_BRIDGE_SECRET_FILE`).
  Sans clé, le pont est désactivé des deux côtés.
- **Jeton** : JWT HS256, valable 60 secondes, avec l'utilisateur, le profil
  actif et les entités sélectionnées (`ent`, `rec`), et une audience
  (`nw-itsm` ou `legacy`) qui empêche de réutiliser un jeton dans l'autre sens.
- **Liaison au navigateur** : l'émetteur pose un nonce aléatoire dans le
  cookie `nw_bridge` (`HttpOnly`, `SameSite=Lax`) et le jeton en contient
  l'empreinte SHA-256. Le destinataire exige le cookie correspondant puis le
  supprime. Un jeton envoyé à quelqu'un d'autre ne le connecte donc pas
  (pas de « login CSRF »), et il n'y a aucun état à stocker côté serveur.
- **Redirections** : seulement vers des chemins de l'application concernée
  (`/app/...` côté Rust, chemins relatifs à `root_doc` côté PHP).

### Parcours

| Sens | Étapes |
|---|---|
| Classique → nouvelle | Menu « New interface » → `front/nw_bridge.php` émet le jeton (audience `nw-itsm`) et le cookie → `GET /api/v2/auth/bridge?token=…` vérifie, ouvre la session Rust et redirige vers `/app/...` |
| Nouvelle → classique | Lien « Interface classique » → `POST /api/v2/auth/bridge/legacy` renvoie le jeton (audience `legacy`) et pose le cookie → `front/nw_bridge.php?token=…` vérifie, ouvre la session PHP (`Session::init`), applique le profil et les entités, puis redirige |

Chaque connexion par le pont est journalisée dans `glpi_events` comme une
connexion normale.

## Conséquences

- Les comptes LDAP, CAS, OIDC et IMAP utilisent déjà la nouvelle interface :
  ils se connectent à l'interface classique puis ouvrent la nouvelle. Le
  portage natif de ces méthodes en Rust n'est plus un préalable ; il reste
  nécessaire avant l'arrêt du PHP.
- Les deux implémentations du format (PHP et Rust) sont testées l'une
  contre l'autre avec des jetons fixes (`tests/units/itsmng/NwBridge.php`,
  `backend/crates/nw-server/src/bridge.rs`).
- Les deux interfaces doivent partager la même origine (même domaine, via le
  proxy) pour que le cookie de nonce soit visible des deux côtés.
- En développement, le serveur PHP intégré ignore les `.htaccess` : le proxy
  de développement (`dev/Caddyfile`) refuse lui-même les chemins privés
  (`config/`, `files/`...), sans quoi les clés seraient téléchargeables.
