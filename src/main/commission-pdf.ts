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

function lineRows(jobs: any[]): string {
  return jobs.map(j => {
    const isSolar = j.job_type === 'solar'
    const detail = isSolar
      ? `${j.system_size_kw ?? 0} kW`
      : money(j.contract_amount ?? 0)
    const rule = isSolar
      ? `$50/kW`
      : (j.manual_override != null ? 'Manual override'
        : (j.contract_amount >= 30000 ? '$30,000+ roofing job'
          : (j.contract_amount <= 20000 ? '$20,000 or less' : 'Review')))
    return `
      <tr>
        <td style="padding:10px 14px;border-bottom:1px solid #eee;font-size:13px;">${escapeHtml(j.client_name)}</td>
        <td style="padding:10px 14px;border-bottom:1px solid #eee;font-size:12px;color:#6b7280;">${j.appointment_date || ''}</td>
        <td style="padding:10px 14px;border-bottom:1px solid #eee;font-size:13px;font-family:'SF Mono',monospace;">${detail}</td>
        <td style="padding:10px 14px;border-bottom:1px solid #eee;font-size:12px;color:#6b7280;">${rule}</td>
        <td style="padding:10px 14px;border-bottom:1px solid #eee;font-size:13px;font-family:'SF Mono',monospace;text-align:right;font-weight:600;">${money(effective(j))}</td>
      </tr>`
  }).join('')
}

function groupSection(title: string, jobs: any[], accent: string): string {
  if (jobs.length === 0) return ''
  const subtotal = jobs.reduce((s, j) => s + effective(j), 0)
  return `
    <div style="margin-bottom:28px;">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;">
        <span style="width:10px;height:10px;border-radius:50%;background:${accent};"></span>
        <h3 style="font-size:13px;text-transform:uppercase;letter-spacing:1px;color:#374151;">${title} · ${jobs.length} job${jobs.length === 1 ? '' : 's'}</h3>
      </div>
      <table style="width:100%;border-collapse:collapse;">
        <thead>
          <tr style="background:#f9fafb;">
            <th style="padding:8px 14px;text-align:left;font-size:10px;text-transform:uppercase;letter-spacing:1px;color:#9ca3af;">Client</th>
            <th style="padding:8px 14px;text-align:left;font-size:10px;text-transform:uppercase;letter-spacing:1px;color:#9ca3af;">Appt</th>
            <th style="padding:8px 14px;text-align:left;font-size:10px;text-transform:uppercase;letter-spacing:1px;color:#9ca3af;">${title === 'Solar' ? 'Size' : 'Contract'}</th>
            <th style="padding:8px 14px;text-align:left;font-size:10px;text-transform:uppercase;letter-spacing:1px;color:#9ca3af;">Rule</th>
            <th style="padding:8px 14px;text-align:right;font-size:10px;text-transform:uppercase;letter-spacing:1px;color:#9ca3af;">Commission</th>
          </tr>
        </thead>
        <tbody>${lineRows(jobs)}</tbody>
        <tfoot>
          <tr>
            <td colspan="4" style="padding:10px 14px;text-align:right;font-size:12px;color:#6b7280;">${title} subtotal</td>
            <td style="padding:10px 14px;text-align:right;font-size:14px;font-weight:700;font-family:'SF Mono',monospace;">${money(subtotal)}</td>
          </tr>
        </tfoot>
      </table>
    </div>`
}

function buildHTML(invoice: any, settings: any): string {
  const accent = '#F5A623'
  const jobs: any[] = invoice.jobs || []
  const solar = jobs.filter(j => j.job_type === 'solar')
  const roofing = jobs.filter(j => j.job_type === 'roofing')
  const total = jobs.reduce((s, j) => s + effective(j), 0)
  const payee = settings.business_name || 'Commission Payee'
  const range = invoice.date_from || invoice.date_to
    ? `${invoice.date_from || '…'} → ${invoice.date_to || '…'}`
    : 'All dates'
  const catLabel = invoice.category === 'mixed' ? 'Solar & Roofing'
    : invoice.category.charAt(0).toUpperCase() + invoice.category.slice(1)

  return `<!DOCTYPE html><html><head><meta charset="utf-8"><style>
    *{margin:0;padding:0;box-sizing:border-box;}
    body{font-family:-apple-system,BlinkMacSystemFont,'Helvetica Neue',Helvetica,Arial,sans-serif;color:#1a1a1a;background:#fff;-webkit-print-color-adjust:exact;print-color-adjust:exact;}
  </style></head><body>
    <div style="height:4px;background:${accent};"></div>
    <div style="padding:48px;">
      <div style="display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:36px;">
        <div>
          <h1 style="font-size:30px;font-weight:700;color:${accent};letter-spacing:1px;">COMMISSION INVOICE</h1>
          <p style="font-size:13px;color:#6b7280;margin-top:6px;">Payable to <strong style="color:#374151;">${escapeHtml(payee)}</strong></p>
        </div>
        <div style="text-align:right;font-size:13px;color:#6b7280;">
          <p style="font-size:15px;font-weight:600;color:#111;">${escapeHtml(invoice.invoice_number)}</p>
          <p style="margin-top:6px;">Category: <strong style="color:#374151;">${catLabel}</strong></p>
          <p>Period: ${escapeHtml(range)}</p>
          <p>Generated: ${new Date().toLocaleDateString('en-US')}</p>
        </div>
      </div>

      ${groupSection('Solar', solar, accent)}
      ${groupSection('Roofing', roofing, '#3498DB')}

      <div style="display:flex;justify-content:flex-end;margin-top:8px;">
        <div style="width:280px;border-top:2px solid #1a1a1a;padding-top:12px;display:flex;justify-content:space-between;">
          <span style="font-size:16px;font-weight:700;">Total owed</span>
          <span style="font-size:20px;font-weight:700;color:${accent};font-family:'SF Mono',monospace;">${money(total)}</span>
        </div>
      </div>

      ${invoice.notes ? `<div style="margin-top:40px;padding:18px;background:#f9fafb;border-radius:8px;"><p style="font-size:12px;color:#6b7280;white-space:pre-line;">${escapeHtml(invoice.notes)}</p></div>` : ''}

      <p style="margin-top:48px;font-size:10px;color:#9ca3af;text-align:center;">Solar: $50 per kW · Roofing: $250 (≤ $20k) / $500 (≥ $30k) per closed job</p>
    </div>
  </body></html>`
}
