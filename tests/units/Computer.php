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

namespace tests\units;

use DbTestCase;

/* Tests for inc/computer.class.php: items created from a template */
class Computer extends DbTestCase
{
    public function testItemFromTemplateKeepsItsName()
    {
        $this->login();
        $entity = getItemByTypeName('Entity', '_test_root_entity', true);

        $computer = new \Computer();
        $template_id = $computer->add([
           'name'          => 'TPL-NAME',
           'template_name' => 'Template',
           'is_template'   => 1,
           'entities_id'   => $entity,
           'comment'       => 'from template',
        ]);
        $this->integer((int)$template_id)->isGreaterThan(0);
        $infocom = new \Infocom();
        $this->integer((int)$infocom->add(['itemtype' => 'Computer', 'items_id' => $template_id, 'value' => 100]))
           ->isGreaterThan(0);

        // The form of a new item from a template posts the template's id.
        $new_id = $computer->add([
           'id'          => $template_id,
           'name'        => 'NEW-NAME',
           'entities_id' => $entity,
        ]);
        $this->integer((int)$new_id)->isGreaterThan(0);
        $this->boolean($computer->getFromDB($new_id))->isTrue();
        $this->string($computer->fields['name'])->isEqualTo('NEW-NAME');
        $this->integer((int)$computer->fields['is_template'])->isEqualTo(0);
        $this->string($computer->fields['comment'])->isEqualTo('from template');
        $this->boolean($infocom->getFromDBforDevice('Computer', $new_id))->isTrue();
        $this->string($infocom->fields['value'])->isEqualTo('100.0000');

        // A copy of an item is still renamed.
        $copy_id = $computer->clone();
        $this->boolean($computer->getFromDB($copy_id))->isTrue();
        $this->string($computer->fields['name'])->isEqualTo('NEW-NAME (copy)');
    }
}
