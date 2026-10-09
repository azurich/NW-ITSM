-- Data set of the API integration tests, loaded on top of
-- install/mysql/glpi-empty.sql. Every password is "secret".
--
-- Entities:            Profiles:
--   0 Root               1 Technician (central, computer = read + update)
--   ├── 1 Paris          2 Self-Service (helpdesk, computer right dropped)
--   │   └── 3 Office A   3 Broken (no interface)
--   └── 2 Lyon

SET SESSION sql_mode = CONCAT(@@sql_mode, ',NO_AUTO_VALUE_ON_ZERO');

INSERT INTO glpi_entities (id, name, entities_id, completename, level) VALUES
  (0, 'Root', -1, 'Root', 1),
  (1, 'Paris', 0, 'Root > Paris', 2),
  (2, 'Lyon', 0, 'Root > Lyon', 2),
  (3, 'Office A', 1, 'Root > Paris > Office A', 3);

INSERT INTO glpi_profiles (id, name, interface) VALUES
  (1, 'Technician', 'central'),
  (2, 'Self-Service', 'helpdesk'),
  (3, 'Broken', '');

INSERT INTO glpi_profilerights (profiles_id, name, rights) VALUES
  (1, 'computer', 3),
  (1, 'ticket', 1024),
  (2, 'computer', 1),
  (2, 'ticket', 1),
  (3, 'computer', 1);

INSERT INTO glpi_users (id, name, password, realname, firstname, authtype, auths_id, is_active, is_deleted, begin_date, end_date, profiles_id, entities_id, password_last_update) VALUES
  (10, 'admin', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', 'Admin', 'Alice', 1, 0, 1, 0, NULL, NULL, 1, 0, NULL),
  (11, 'paris', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 1, 0, 1, 0, NULL, NULL, 1, 1, NULL),
  (12, 'lyon', '5ebe2294ecd0e0f08eab7690d2a6ee69', NULL, NULL, 1, 0, 1, 0, NULL, NULL, 1, 2, NULL),
  (13, 'helpdesk', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 1, 0, 1, 0, NULL, NULL, 2, 0, NULL),
  (14, 'disabled', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 1, 0, 0, 0, NULL, NULL, 1, 0, NULL),
  (15, 'expired', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 1, 0, 1, 0, NULL, '2000-01-01 00:00:00', 1, 0, NULL),
  (16, 'noprofile', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 1, 0, 1, 0, NULL, NULL, 0, 0, NULL),
  (17, 'ldapuser', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 3, 1, 1, 0, NULL, NULL, 1, 0, NULL),
  (18, 'broken', '$2y$10$cxuiz84fGGyx3k8v44yhaulzj6RUep9k743p0DuoTuHUnMb/HMIT6', NULL, NULL, 1, 0, 1, 0, NULL, NULL, 3, 0, NULL);

INSERT INTO glpi_profiles_users (users_id, profiles_id, entities_id, is_recursive) VALUES
  (10, 1, 0, 1),
  (11, 1, 1, 1),
  (12, 1, 2, 0),
  (13, 2, 0, 1),
  (14, 1, 0, 1),
  (15, 1, 0, 1),
  (17, 1, 0, 1),
  (18, 3, 0, 1);

INSERT INTO glpi_locations (id, name, completename, entities_id, is_recursive, locations_id, level) VALUES
  (1, 'Room 1', 'Paris HQ > Room 1', 1, 0, 0, 2);

INSERT INTO glpi_computertypes (id, name) VALUES (1, 'Laptop');

INSERT INTO glpi_computers (id, name, serial, otherserial, entities_id, is_recursive, is_deleted, is_template, locations_id, computertypes_id, users_id, date_mod) VALUES
  (100, 'PC-ROOT-PRIVATE', NULL, NULL, 0, 0, 0, 0, 0, 0, 0, '2026-01-01 10:00:00'),
  (101, 'PC-ROOT-SHARED', NULL, NULL, 0, 1, 0, 0, 0, 0, 0, '2026-01-02 10:00:00'),
  (102, 'PC-PARIS-1', 'SN-P1', 'INV-P1', 1, 0, 0, 0, 1, 1, 10, '2026-01-03 10:00:00'),
  (103, 'PC-PARIS-A', NULL, NULL, 3, 0, 0, 0, 0, 0, 0, '2026-01-04 10:00:00'),
  (104, 'PC-LYON-1', NULL, NULL, 2, 0, 0, 0, 0, 0, 0, '2026-01-05 10:00:00'),
  (105, 'PC-PARIS-TRASH', NULL, NULL, 1, 0, 1, 0, 0, 0, 0, '2026-01-06 10:00:00'),
  (106, 'PC-PARIS-TEMPLATE', NULL, NULL, 1, 0, 0, 1, 0, 0, 0, '2026-01-07 10:00:00'),
  (107, '100%_PC', NULL, NULL, 1, 0, 0, 0, 0, 0, 0, '2026-01-08 10:00:00');
