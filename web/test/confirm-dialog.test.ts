import { describe, expect, it } from 'vitest'
import { mountSuspended } from '@nuxt/test-utils/runtime'
import { defineComponent } from 'vue'
import ConfirmDialog from '~/components/ConfirmDialog.vue'

const props = { open: true, title: 'Stop it?', body: 'Really.', confirmLabel: 'Stop print', danger: true }
const mount = (o: Record<string, unknown> = {}) => mountSuspended(ConfirmDialog, { props: { ...props, ...o }, attachTo: document.body })

describe('ConfirmDialog', () => {
  it('opens as a modal dialog, focuses Cancel and Escape cancels', async () => {
    const w = await mount()
    const dlg = w.find('dialog').element as HTMLDialogElement
    expect(dlg.open).toBe(true)
    expect(document.activeElement?.textContent).toBe('Cancel')
    dlg.dispatchEvent(new Event('cancel', { cancelable: true }))
    expect(w.emitted('cancel')).toHaveLength(1)
    expect(w.emitted('confirm')).toBeUndefined()
    w.unmount()
  })

  it('emits confirm and uses the danger style only when asked', async () => {
    const w = await mount()
    const btn = w.findAll('button').find((b) => b.text() === 'Stop print')!
    expect(btn.classes()).toContain('btn-danger')
    await btn.trigger('click')
    expect(w.emitted('confirm')).toHaveLength(1)
    await w.setProps({ danger: false })
    expect(w.findAll('button').find((b) => b.text() === 'Stop print')!.classes()).not.toContain('btn-danger')
    w.unmount()
  })

  it('shows Working… and disables confirm while busy, and shows an error inside', async () => {
    const w = await mount({ busy: true, error: 'The printer is busy.' })
    const btn = w.findAll('button').find((b) => b.text() === 'Working…')!
    expect((btn.element as HTMLButtonElement).disabled).toBe(true)
    expect(w.text()).toContain('The printer is busy.')
    w.unmount()
  })

  it('ties each dialog to its own heading id, even with two on one page', async () => {
    const Two = defineComponent({
      components: { ConfirmDialog },
      template: '<div><ConfirmDialog open title="A?" body="a" confirm-label="Go" /><ConfirmDialog open title="B?" body="b" confirm-label="Go" /></div>',
    })
    const w = await mountSuspended(Two, { attachTo: document.body })
    const dialogs = w.findAll('dialog')
    const ids = dialogs.map((d) => d.attributes('aria-labelledby')!)
    expect(new Set(ids).size).toBe(2)
    dialogs.forEach((d, i) => expect(d.find('h2').attributes('id')).toBe(ids[i]))
    w.unmount()
  })

  it('closes when open becomes false', async () => {
    const w = await mount()
    await w.setProps({ open: false })
    expect((w.find('dialog').element as HTMLDialogElement).open).toBe(false)
    w.unmount()
  })
})
