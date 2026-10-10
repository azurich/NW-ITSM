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

describe('fields of one type', () => {
  const monitor = {
    ...detail,
    size: '21.50',
    is_global: true,
    has_hdmi: true,
    has_dvi: false,
  } as AssetDetail

  it('reads and sends the size, the management type and the features', () => {
    const initial = formFromDetail(monitor)
    expect(initial.text.size).toBe('21.50')
    expect(initial.flags).toMatchObject({ is_global: true, has_hdmi: true, has_dvi: false, has_bnc: false })
    const form = formFromDetail(monitor)
    form.text.size = '22'
    form.flags.has_hdmi = false
    form.flags.has_pivot = true
    expect(changedFields('monitors', form, initial)).toEqual({ size: '22', has_hdmi: false, has_pivot: true })
  })

  it('sends the page counters as numbers, 0 when empty', () => {
    const printer = { ...detail, kind: 'printers', initial_page_counter: 10, page_counter: 250 } as AssetDetail
    const initial = formFromDetail(printer)
    const form = formFromDetail(printer)
    form.numbers.page_counter = ''
    expect(changedFields('printers', form, initial)).toEqual({ page_counter: 0 })
    // Not a field of monitors.
    expect(changedFields('monitors', form, initial)).toEqual({})
  })

  it('sends the switched features of a creation only', () => {
    const form = emptyForm()
    form.text.name = 'TEL'
    form.flags.has_headset = true
    form.flags.has_hdmi = true
    form.refs.power_supply = { id: 1, label: 'PoE' }
    expect(changedFields('phones', form)).toEqual({ name: 'TEL', has_headset: true, power_supply_id: 1 })
  })
})
