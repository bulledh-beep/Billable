import { BrowserWindow, dialog, shell, app } from 'electron'
import path from 'path'
import fs from 'fs'
import { getCommissionInvoice, getSettings } from './database'

function effective(c: any): number {
  if (c.manual_override != null) return c.manual_override
  if (c.needs_review) return 0
  return c.calculated_commission || 0
}

function money(n: number): string {
  return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(n || 0)
}

function escapeHtml(str: string): string {
  return String(str || '')
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#039;')
}

/** Generate a clean commission payout report PDF and prompt to save it. */
export async function generateCommissionInvoicePDF(invoiceId: number): Promise<string | null> {
  const invoice = getCommissionInvoice(invoiceId) as any
  if (!invoice) throw new Error('Commission invoice not found')
  const settings = getSettings()
  const html = buildHTML(invoice, settings)

  const pdfWindow = new BrowserWindow({
    width: 800, height: 1130, show: false,
    webPreferences: { nodeIntegration: false, contextIsolation: true },
  })
  try {
    await pdfWindow.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`)
    await new Promise(r => setTimeout(r, 600))
    const pdfData = await pdfWindow.webContents.printToPDF({
      pageSize: 'A4', printBackground: true, margins: { top: 0, bottom: 0, left: 0, right: 0 },
    })
    // Dev/test hook: write straight to a folder instead of asking
    if (process.env.BILLABLE_EXPORT_DIR) {
      const out = path.join(process.env.BILLABLE_EXPORT_DIR, `${invoice.invoice_number}.pdf`)
      fs.writeFileSync(out, Buffer.from(pdfData))
      return out
    }
    const defaultPath = path.join(app.getPath('downloads'), `${invoice.invoice_number}.pdf`)
    const { filePath: savePath } = await dialog.showSaveDialog({
      title: 'Save Commission Invoice PDF', defaultPath, filters: [{ name: 'PDF', extensions: ['pdf'] }],
    })
    if (!savePath) return null
    fs.writeFileSync(savePath, Buffer.from(pdfData))
    shell.openPath(savePath)
    return savePath
  } finally {
    pdfWindow.destroy()
  }
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
function formatDay(ymd: string | null | undefined): string {
  const [y, m, d] = String(ymd || '').slice(0, 10).split('-').map(Number)
  if (!y || !m || !d) return String(ymd || '')
  return `${MONTHS[m - 1]} ${d}, ${y}`
}

function todayDay(): string {
  const d = new Date()
  return `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`
}

function lineRows(jobs: any[]): string {
  return jobs.map(j => {
    const isSolar = j.job_type === 'solar'
    const detail = isSolar ? `${j.system_size_kw ?? 0} kW` : money(j.contract_amount ?? 0)
    const rule = isSolar
      ? '$50 per kW'
      : (j.manual_override != null ? 'Set by hand'
        : (j.contract_amount >= 30000 ? '$30,000 or more'
          : (j.contract_amount <= 20000 ? '$20,000 or less' : 'Needs review')))
    return `
      <tr>
        <td>${escapeHtml(j.client_name)}</td>
        <td class="muted">${formatDay(j.appointment_date)}</td>
        <td class="num">${detail}</td>
        <td class="muted">${rule}</td>
        <td class="num strong">${money(effective(j))}</td>
      </tr>`
  }).join('')
}

function groupSection(title: string, jobs: any[], dot: string): string {
  if (jobs.length === 0) return ''
  const subtotal = jobs.reduce((s, j) => s + effective(j), 0)
  return `
    <div class="group">
      <div class="group-title"><span class="dot" style="background:${dot}"></span>${title}<span class="faint">· ${jobs.length} job${jobs.length === 1 ? '' : 's'}</span></div>
      <table>
        <thead>
          <tr>
            <th>Client</th>
            <th>Appointment</th>
            <th class="num">${title === 'Solar' ? 'System size' : 'Contract'}</th>
            <th>Rule</th>
            <th class="num">Commission</th>
          </tr>
        </thead>
        <tbody>${lineRows(jobs)}</tbody>
        <tfoot>
          <tr>
            <td colspan="4" class="sub-label">${title} subtotal</td>
            <td class="num strong">${money(subtotal)}</td>
          </tr>
        </tfoot>
      </table>
    </div>`
}

function buildHTML(invoice: any, settings: any): string {
  const jobs: any[] = invoice.jobs || []
  const solar = jobs.filter(j => j.job_type === 'solar')
  const roofing = jobs.filter(j => j.job_type === 'roofing')
  const total = jobs.reduce((s, j) => s + effective(j), 0)
  const payee = settings.business_name || 'Commission payee'
  const range = invoice.date_from || invoice.date_to
    ? `${invoice.date_from ? formatDay(invoice.date_from) : 'Start'} to ${invoice.date_to ? formatDay(invoice.date_to) : 'today'}`
    : 'All dates'
  const catLabel = invoice.category === 'mixed' ? 'Solar and roofing'
    : invoice.category.charAt(0).toUpperCase() + invoice.category.slice(1)

  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>
    @page { size: A4; margin: 0; }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif; color: #18181b; background: #fff; font-size: 12.5px; line-height: 1.5; font-variant-numeric: tabular-nums; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
    .page { padding: 52px 56px 48px; }
    .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 32px; margin-bottom: 40px; }
    .title { font-size: 28px; font-weight: 600; letter-spacing: -0.02em; line-height: 1.1; }
    .muted { color: #52525b; }
    .faint { color: #a1a1aa; font-weight: 400; margin-left: 6px; }
    .facts { min-width: 230px; }
    .fact { display: flex; justify-content: space-between; gap: 24px; padding: 2px 0; }
    .fact span:first-child { color: #71717a; }
    .group { margin-bottom: 28px; }
    .group-title { display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: 13px; margin-bottom: 8px; }
    .dot { width: 8px; height: 8px; border-radius: 50%; }
    table { width: 100%; border-collapse: collapse; }
    th { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #71717a; text-align: left; padding: 0 0 8px; border-bottom: 1.5px solid #18181b; }
    td { padding: 9px 0; border-bottom: 1px solid #f0f0f2; }
    th.num, td.num { text-align: right; white-space: nowrap; }
    td.num { color: #52525b; }
    td.strong { color: #18181b; font-weight: 500; }
    th:not(:last-child), td:not(:last-child) { padding-right: 22px; }
    tfoot td { border-bottom: none; padding-top: 10px; }
    .sub-label { text-align: right; color: #71717a; padding-right: 24px; }
    .total { display: flex; justify-content: flex-end; margin-top: 8px; }
    .total-box { width: 270px; border-top: 1.5px solid #18181b; padding-top: 10px; display: flex; justify-content: space-between; font-size: 16px; font-weight: 600; }
    .notes { margin-top: 40px; padding-top: 16px; border-top: 1px solid #e4e4e7; }
    .label { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #a1a1aa; margin-bottom: 6px; }
    .foot { margin-top: 44px; font-size: 11px; color: #a1a1aa; }
  </style></head><body>
    <div class="page">
      <div class="top">
        <div>
          <div class="title">Commission invoice</div>
          <div class="muted" style="margin-top: 6px;">Payable to <strong style="color:#18181b">${escapeHtml(payee)}</strong></div>
          ${settings.business_email ? `<div class="muted">${escapeHtml(settings.business_email)}</div>` : ''}
        </div>
        <div class="facts">
          <div class="fact"><span>Invoice</span><span>${escapeHtml(invoice.invoice_number)}</span></div>
          <div class="fact"><span>Jobs</span><span>${catLabel}</span></div>
          <div class="fact"><span>Period</span><span>${escapeHtml(range)}</span></div>
          <div class="fact"><span>Issued</span><span>${todayDay()}</span></div>
        </div>
      </div>

      ${groupSection('Solar', solar, '#F5A623')}
      ${groupSection('Roofing', roofing, '#3E8BF7')}

      <div class="total">
        <div class="total-box"><span>Total owed</span><span>${money(total)}</span></div>
      </div>

      ${invoice.notes ? `<div class="notes"><div class="label">Notes</div><p class="muted" style="white-space:pre-line">${escapeHtml(invoice.notes)}</p></div>` : ''}

      <div class="foot">Solar pays $50 per kW. Roofing pays $250 for contracts of $20,000 or less and $500 for $30,000 or more.</div>
    </div>
  </body></html>`
}
