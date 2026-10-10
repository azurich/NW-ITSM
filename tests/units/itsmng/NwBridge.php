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

namespace tests\units\itsmng;

/* Test for src/NwBridge.php */

class NwBridge extends \GLPITestCase
{
    private const SECRET = 'test-bridge-secret-test-bridge-secret';

    /**
     * Signed by the Rust API (backend/crates/nw-server/src/bridge.rs,
     * test "php_vector"): user 2, profile 4, every entity, nonce "n0nce",
     * issued at 1000.
     */
    private const RUST_TOKEN = 'eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJsZWdhY3kiLCJ1aWQiOjIsInBpZCI6NCwiZW50IjpudWxsLCJyZWMiOmZhbHNlLCJub25jZSI6IjBhOTIxMzNiNDUxMzc4Y2ExZjk5MWU1MDdmZDhmMTUwMTdkMTU4ZTFiODU2ZDc3ZTBiNjIyMzRhN2NmYTg1M2EiLCJpYXQiOjEwMDAsImV4cCI6MTA2MH0.g_pfFUvweuSZEf730ykLsWeR_O0cesl9BY_i3-yBcM0';

    public function testAcceptsTokensSignedByTheRustApi()
    {
        $claims = \itsmng\NwBridge::verify(self::RUST_TOKEN, self::SECRET, 'legacy', 'n0nce', 1030);
        $this->array($claims)
            ->integer['uid']->isEqualTo(2)
            ->integer['pid']->isEqualTo(4);
        $this->variable($claims['ent'])->isNull();
        $this->boolean($claims['rec'])->isFalse();
    }

    public function testRefusesOtherNoncesAudiencesKeysAndTimes()
    {
        $verify = fn ($token, $audience, $nonce, $now, $secret = self::SECRET)
            => \itsmng\NwBridge::verify($token, $secret, $audience, $nonce, $now);

        $this->variable($verify(self::RUST_TOKEN, 'legacy', null, 1030))->isNull();
        $this->variable($verify(self::RUST_TOKEN, 'legacy', 'other', 1030))->isNull();
        $this->variable($verify(self::RUST_TOKEN, 'nw-itsm', 'n0nce', 1030))->isNull();
        $this->variable($verify(self::RUST_TOKEN, 'legacy', 'n0nce', 1061))->isNull();
        $this->variable($verify(self::RUST_TOKEN, 'legacy', 'n0nce', 990))->isNull();
        $this->variable($verify(self::RUST_TOKEN, 'legacy', 'n0nce', 1030, str_repeat('x', 32)))->isNull();
        $this->variable($verify('not.a.token', 'legacy', 'n0nce', 1030))->isNull();

        // Unsigned token ("alg": "none").
        [, $payload] = explode('.', self::RUST_TOKEN);
        $unsigned = rtrim(strtr(base64_encode('{"alg":"none"}'), '+/', '-_'), '=') . '.' . $payload . '.';
        $this->variable($verify($unsigned, 'legacy', 'n0nce', 1030))->isNull();
    }

    public function testSignedTokensRoundTrip()
    {
        $session = [
            'glpiID'                      => '2',
            'glpiactiveprofile'           => ['id' => '4'],
            'glpiactive_entity'           => '1',
            'glpiactive_entity_recursive' => true,
            'glpiactiveentities'          => [1 => 1, 3 => 3],
        ];
        $claims = \itsmng\NwBridge::claimsForApi($session, 'n0nce', 1000);
        $this->array($claims)
            ->string['aud']->isEqualTo('nw-itsm')
            ->integer['uid']->isEqualTo(2)
            ->integer['ent']->isEqualTo(1)
            ->boolean['rec']->isTrue()
            ->integer['exp']->isEqualTo(1060);

        $token = \itsmng\NwBridge::sign($claims, self::SECRET);
        $this->array(\itsmng\NwBridge::verify($token, self::SECRET, 'nw-itsm', 'n0nce', 1000))
            ->isEqualTo($claims);
    }

    public function testEntitiesOfTheLegacySession()
    {
        $base = ['glpiID' => 2, 'glpiactiveprofile' => ['id' => 4], 'glpiactive_entity' => 3];
        $one = \itsmng\NwBridge::claimsForApi($base + ['glpiactiveentities' => [3 => 3]], 'n', 0);
        $this->variable($one['ent'])->isEqualTo(3);
        $this->boolean($one['rec'])->isFalse();

        $all = \itsmng\NwBridge::claimsForApi($base + ['glpiactiveentities' => [1 => 1, 3 => 3]], 'n', 0);
        $this->variable($all['ent'])->isNull();
    }

    public function testSafePaths()
    {
        $this->boolean(\itsmng\NwBridge::isSafePath('/front/computer.form.php?id=2'))->isTrue();
        foreach (['https://evil.example', '//evil.example', '/\\evil.example', 'front/central.php', null] as $path) {
            $this->boolean(\itsmng\NwBridge::isSafePath($path))->isFalse();
        }
    }
}
