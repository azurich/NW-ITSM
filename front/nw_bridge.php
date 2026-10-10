<?php

/**
 * ---------------------------------------------------------------------
 * ITSM-NG
 * Copyright (C) 2022 ITSM-NG and contributors.
 *
 * https://www.itsm-ng.org
 *
 * based on GLPI - Gestionnaire Libre de Parc Informatique
 * Copyright (C) 2003-2014 by the INDEPNET Development Team.
 *
 * ---------------------------------------------------------------------
 *
 * LICENSE
 *
 * This file is part of ITSM-NG.
 *
 * ITSM-NG is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * ITSM-NG is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with ITSM-NG. If not, see <http://www.gnu.org/licenses/>.
 * ---------------------------------------------------------------------
 */

/**
 * Connection bridge with NW-ITSM (see src/NwBridge.php).
 *
 * - With a "token" parameter: logs in with a token issued by the Rust API
 *   (POST /api/v2/auth/bridge/legacy), then opens the "redirect" page.
 * - Without: opens the NW-ITSM application ("redirect" page under /app) as
 *   the current user.
 */

use Glpi\Event;
use itsmng\NwBridge;

include('../inc/includes.php');

/** Where the Rust API receives the tokens of this application. */
const NW_BRIDGE_API_URL = '/api/v2/auth/bridge';

$secret = NwBridge::secret();
if ($secret === null) {
    Html::displayErrorAndDie(__('The connection bridge with NW-ITSM is not configured.'));
}
$secure = (!empty($_SERVER['HTTPS']) && $_SERVER['HTTPS'] !== 'off')
    || (($_SERVER['HTTP_X_FORWARDED_PROTO'] ?? '') === 'https');

if (isset($_GET['token'])) {
    // From NW-ITSM to this application.
    $token = $_UGET['token'];
    $nonce = $_COOKIE[NwBridge::NONCE_COOKIE] ?? null;
    $claims = is_string($token)
        ? NwBridge::verify($token, $secret, NwBridge::AUDIENCE_LEGACY, is_string($nonce) ? $nonce : null)
        : null;
    NwBridge::clearNonceCookie($secure);
    if ($claims === null) {
        Html::redirect($CFG_GLPI['root_doc'] . '/index.php');
    }

    if ((int) Session::getLoginUserID() !== $claims['uid']) {
        $auth = new Auth();
        if (!$auth->user->getFromDB($claims['uid'])) {
            Html::redirect($CFG_GLPI['root_doc'] . '/index.php');
        }
        // The token proves the identity: no password to check here.
        $auth->auth_succeded = true;
        Session::init($auth);
        if (!$auth->auth_succeded || (int) Session::getLoginUserID() !== $claims['uid']) {
            Session::destroy();
            Html::redirect($CFG_GLPI['root_doc'] . '/index.php');
        }
        // Like Auth::login, but also with servers that do not export the
        // request to the environment (getenv).
        $ip = Toolbox::clean_cross_side_scripting_deep(
            $_SERVER['HTTP_X_FORWARDED_FOR'] ?? $_SERVER['REMOTE_ADDR'] ?? ''
        );
        Event::log(0, 'system', 3, 'login', sprintf(
            __('%1$s log in from IP %2$s'),
            $auth->user->fields['name'],
            $ip
        ));
    }

    // Same profile and entities as in NW-ITSM, when still allowed.
    if (isset($_SESSION['glpiprofiles'][$claims['pid']])) {
        Session::changeProfile($claims['pid']);
    }
    if ($claims['ent'] === null) {
        Session::changeActiveEntities('all');
    } else {
        Session::changeActiveEntities($claims['ent'], $claims['rec']);
    }

    $redirect = $_UGET['redirect'] ?? null;
    if (!NwBridge::isSafePath($redirect)) {
        $redirect = '/front/central.php';
    }
    Html::redirect($CFG_GLPI['root_doc'] . $redirect);
}

// From this application to NW-ITSM.
Session::checkLoginUser();
if (!isset($_SESSION['glpiactiveprofile']['id'])) {
    // Logged in without profile: the password must be changed first.
    Html::redirect($CFG_GLPI['root_doc'] . '/front/updatepassword.php');
}
$nonce = NwBridge::newNonce();
$token = NwBridge::sign(NwBridge::claimsForApi($_SESSION, $nonce, time()), $secret);
NwBridge::setNonceCookie($nonce, $secure);

$redirect = $_UGET['redirect'] ?? '/app/';
Html::redirect(NW_BRIDGE_API_URL . '?' . http_build_query([
    'token'    => $token,
    'redirect' => is_string($redirect) ? $redirect : '/app/',
]));
