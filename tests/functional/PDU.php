<?php

/**
 * ---------------------------------------------------------------------
 * GLPI - Gestionnaire Libre de Parc Informatique
 * Copyright (C) 2015-2022 Teclib' and contributors.
 *
 * http://glpi-project.org
 *
 * based on GLPI - Gestionnaire Libre de Parc Informatique
 * Copyright (C) 2003-2014 by the INDEPNET Development Team.
 *
 * ---------------------------------------------------------------------
 *
 * LICENSE
 *
 * This file is part of GLPI.
 *
 * GLPI is free software; you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation; either version 2 of the License, or
 * (at your option) any later version.
 *
 * GLPI is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with GLPI. If not, see <http://www.gnu.org/licenses/>.
 * ---------------------------------------------------------------------
 */

namespace tests\units;

use DbTestCase;

/* Test for inc/pdu.class.php */

class PDU extends DbTestCase
{
    public function testFormShowsTheInventoryNumber()
    {
        $this->login();
        $obj = new \PDU();
        $id = $obj->add([
           'name'        => __FUNCTION__,
           'serial'      => 'SN-PDU',
           'otherserial' => 'INV-PDU',
           'entities_id' => 0
        ]);
        $this->integer((int)$id)->isGreaterThan(0);

        // The inventory number field shows the inventory number, so that
        // saving the form does not replace it with the serial number.
        $this->output(
            function () use ($obj, $id) {
                $obj->showForm($id);
            }
        )->matches('/name="otherserial"\s+type="text"\s+value="INV-PDU"/');
    }
}
