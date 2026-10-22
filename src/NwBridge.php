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

namespace itsmng;

/**
 * Connection bridge with NW-ITSM (Rust API and React application).
 *
 * A user logged in one interface opens the other one without logging in
 * again. The issuing side signs a short-lived token (JWT HS256, key in
 * config/nw-bridge.key, shared with the Rust API) and sets a random nonce
 * in the "nw_bridge" cookie; the token carries the SHA-256 of that nonce.
 * The receiving side accepts the token only with the matching cookie, so a
 * token sent to someone else cannot log them in.
 *
 * Same format as backend/crates/nw-server/src/bridge.rs.
 */
final class NwBridge
{
    public const NONCE_COOKIE = 'nw_bridge';
    /** Tokens consumed by the Rust API. */
    public const AUDIENCE_API = 'nw-itsm';
    /** Tokens consumed by this application. */
    public const AUDIENCE_LEGACY = 'legacy';
    public const TTL = 60;
    /** Tolerated clock difference between the PHP and Rust servers. */
    private const CLOCK_SKEW = 5;
    private const KEY_FILE = 'nw-bridge.key';

    /**
     * Shared secret, or null when the bridge is not configured.
     */
    public static function secret(): ?string
    {
        $file = GLPI_CONFIG_DIR . '/' . self::KEY_FILE;
        if (!is_readable($file)) {
            return null;
        }
        $secret = trim((string) file_get_contents($file));
        return strlen($secret) >= 32 ? $secret : null;
    }

    public static function isEnabled(): bool
    {
        return self::secret() !== null;
    }

    public static function newNonce(): string
    {
        return bin2hex(random_bytes(32));
    }

    public static function sign(array $claims, string $secret): string
    {
        $header = self::base64UrlEncode(json_encode(['typ' => 'JWT', 'alg' => 'HS256']));
        $payload = self::base64UrlEncode(json_encode($claims));
        $signature = hash_hmac('sha256', "$header.$payload", $secret, true);
        return "$header.$payload." . self::base64UrlEncode($signature);
    }

    /**
     * Claims of a valid token for the audience, or null.
     */
    public static function verify(
        string $token,
        string $secret,
        string $audience,
        ?string $nonce,
        ?int $now = null
    ): ?array {
        $parts = explode('.', $token);
        if (count($parts) !== 3) {
            return null;
        }
        [$header64, $payload64, $signature64] = $parts;
        $header = json_decode((string) self::base64UrlDecode($header64), true);
        if (!is_array($header) || ($header['alg'] ?? null) !== 'HS256') {
            return null;
        }
        $expected = hash_hmac('sha256', "$header64.$payload64", $secret, true);
        $signature = self::base64UrlDecode($signature64);
        if ($signature === null || !hash_equals($expected, $signature)) {
            return null;
        }

        $claims = json_decode((string) self::base64UrlDecode($payload64), true);
        if (!is_array($claims)) {
            return null;
        }
        $now = $now ?? time();
        foreach (['uid', 'pid', 'iat', 'exp'] as $field) {
            if (!is_int($claims[$field] ?? null)) {
                return null;
            }
        }
        if (
            ($claims['aud'] ?? null) !== $audience
            || $claims['exp'] < $now
            || $claims['iat'] > $now + self::CLOCK_SKEW
            || $claims['exp'] - $claims['iat'] > self::TTL
            || !array_key_exists('ent', $claims)
            || ($claims['ent'] !== null && !is_int($claims['ent']))
            || !is_bool($claims['rec'] ?? null)
            || !is_string($claims['nonce'] ?? null)
        ) {
            return null;
        }
        if (!is_string($nonce) || $nonce === '' || !hash_equals($claims['nonce'], hash('sha256', $nonce))) {
            return null;
        }
        return $claims;
    }

    /**
     * Claims opening the Rust application as the user of a session
     * ($_SESSION), with its profile and entities.
     */
    public static function claimsForApi(array $session, string $nonce, int $now): array
    {
        $entities = $session['glpiactiveentities'] ?? [];
        $entity = (int) ($session['glpiactive_entity'] ?? 0);
        $recursive = !empty($session['glpiactive_entity_recursive']);
        if ($recursive) {
            [$ent, $rec] = [$entity, true];
        } elseif (count($entities) === 1) {
            [$ent, $rec] = [$entity, false];
        } else {
            // "Show all": every entity of the profile.
            [$ent, $rec] = [null, false];
        }
        return [
            'aud'   => self::AUDIENCE_API,
            'uid'   => (int) $session['glpiID'],
            'pid'   => (int) $session['glpiactiveprofile']['id'],
            'ent'   => $ent,
            'rec'   => $rec,
            'nonce' => hash('sha256', $nonce),
            'iat'   => $now,
            'exp'   => $now + self::TTL,
        ];
    }

    /**
     * Only paths of this application: never an open redirect.
     */
    public static function isSafePath(?string $path): bool
    {
        return is_string($path)
            && str_starts_with($path, '/')
            && !str_starts_with($path, '//')
            && !str_contains($path, '\\');
    }

    public static function setNonceCookie(string $nonce, bool $secure): void
    {
        setcookie(self::NONCE_COOKIE, $nonce, self::cookieOptions(time() + self::TTL, $secure));
    }

    public static function clearNonceCookie(bool $secure): void
    {
        setcookie(self::NONCE_COOKIE, '', self::cookieOptions(time() - 3600, $secure));
    }

    private static function cookieOptions(int $expires, bool $secure): array
    {
        return [
            'expires'  => $expires,
            'path'     => '/',
            'secure'   => $secure,
            'httponly' => true,
            'samesite' => 'Lax',
        ];
    }

    private static function base64UrlEncode(string $data): string
    {
        return rtrim(strtr(base64_encode($data), '+/', '-_'), '=');
    }

    private static function base64UrlDecode(string $data): ?string
    {
        $decoded = base64_decode(strtr($data, '-_', '+/'), true);
        return $decoded === false ? null : $decoded;
    }
}
