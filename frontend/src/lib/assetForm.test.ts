import { describe, expect, it } from 'vitest'
import type { AssetDetail } from '../api/assets'
import { changedFields, emptyForm, formFromDetail } from './assetForm'

const detail = {
  kind: 'monitors',
  id: 200,
  name: 'MON-1',
  serial: null,
  inventory_number: 'INV',
  comment: 'a < b',
  is_deleted: false,
  is_dynamic: false,
  is_recursive: true,
  is_global: false,
  entity: { id: 1, name: 'Root > Paris' },
  location: { id: 3, name: 'Warehouse' },
  user: { id: 10, name: null },
  permissions: { update: true, trash: true },
} as AssetDetail

describe('formFromDetail', () => {
  it('fills the texts and choices', () => {
    const form = formFromDetail(detail)
    expect(form.text.name).toBe('MON-1')
    expect(form.text.serial).toBe('')
    expect(form.refs.location).toEqual({ id: 3, label: 'Warehouse' })
    expect(form.refs.user).toEqual({ id: 10, label: '#10' })
    expect(form.refs.state).toBeNull()
    expect(form.is_recursive).toBe(true)
  })
})

describe('changedFields', () => {
  it('sends only the changes of an update', () => {
    const initial = formFromDetail(detail)
    const form = formFromDetail(detail)
    expect(changedFields('monitors', form, initial)).toEqual({})

    form.text.name = 'MON-2'
    form.text.comment = ''
    form.refs.location = null
    form.refs.state = { id: 1, label: 'In use' }
    expect(changedFields('monitors', form, initial)).toEqual({
      name: 'MON-2',
      comment: '',
      location_id: null,
      state_id: 1,
    })
  })

  it('sends the filled fields of a creation, for the type only', () => {
    const form = emptyForm()
    form.text.name = 'PC'
    form.text.uuid = 'abc'
    form.text.brand = 'Logi'
    form.refs.network = { id: 2, label: 'LAN' }
    form.is_recursive = true
    expect(changedFields('computers', form)).toEqual({
      name: 'PC',
      uuid: 'abc',
      network_id: 2,
      is_recursive: true,
    })
    expect(changedFields('peripherals', form)).toEqual({ name: 'PC', brand: 'Logi', is_recursive: true })
  })
})
