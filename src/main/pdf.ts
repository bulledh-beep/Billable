import { BrowserWindow, dialog, shell } from 'electron'
import path from 'path'
import { app } from 'electron'
import fs from 'fs'
import { getInvoice, getSettings, getTaxSettings, updateInvoice } from './database'

export async function generateInvoicePDF(invoiceId: number): Promise<string | null> {
  const invoice = getInvoice(invoiceId) as any
  if (!invoice) throw new Error('Invoice not found')

  const settings = getSettings()
  const html = generateInvoiceHTML(invoice, settings)

  // Create hidden window to render PDF
  const pdfWindow = new BrowserWindow({
    width: 800,
    height: 1130,
    show: false,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
    },
  })

  try {
    await pdfWindow.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`)

    // Wait for content to be ready
    await new Promise(resolve => setTimeout(resolve, 1000))

    const pdfData = await pdfWindow.webContents.printToPDF({
      pageSize: 'A4',
      printBackground: true,
      margins: { top: 0, bottom: 0, left: 0, right: 0 },
    })

    // Dev/test hook: write straight to a folder instead of asking
    const testDir = process.env.BILLABLE_EXPORT_DIR
    if (testDir) {
      const out = path.join(testDir, `${invoice.invoice_number}.pdf`)
      fs.writeFileSync(out, Buffer.from(pdfData))
      return out
    }

    // Ask user where to save
    const defaultPath = path.join(app.getPath('downloads'), `${invoice.invoice_number}.pdf`)
    const { filePath: savePath } = await dialog.showSaveDialog({
      title: 'Save Invoice PDF',
      defaultPath,
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    })

    if (!savePath) {
      return null // User cancelled
    }

    fs.writeFileSync(savePath, Buffer.from(pdfData))

    // Also save a copy in app data
    const pdfDir = path.join(app.getPath('userData'), 'invoices')
    if (!fs.existsSync(pdfDir)) {
      fs.mkdirSync(pdfDir, { recursive: true })
    }
    const internalPath = path.join(pdfDir, `${invoice.invoice_number}.pdf`)
    if (savePath !== internalPath) {
      fs.writeFileSync(internalPath, Buffer.from(pdfData))
    }

    // Update invoice with PDF path
    updateInvoice(invoiceId, { pdf_path: savePath })

    // Open the PDF
    shell.openPath(savePath)

    return savePath
  } finally {
    pdfWindow.destroy()
  }
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** "Jul 20, 2026" from a YYYY-MM-DD string, without timezone drift. */
function formatDay(ymd: string | null | undefined): string {
  const [y, m, d] = String(ymd || '').slice(0, 10).split('-').map(Number)
  if (!y || !m || !d) return String(ymd || '')
  return `${MONTHS[m - 1]} ${d}, ${y}`
}

function generateInvoiceHTML(invoice: any, settings: any): string {
  const tax = getTaxSettings() || {}
  const items = invoice.items || []
  const currency = invoice.currency || settings.default_currency || 'CAD'
  const locale = currency === 'CAD' ? 'en-CA' : 'en-US'
  const money = (amount: number) =>
    new Intl.NumberFormat(locale, { style: 'currency', currency }).format(Number(amount) || 0)

  const businessName = settings.business_name || tax.business_name || 'Your Business'
  const businessAddress = settings.business_address || tax.business_address || ''
  const taxId = settings.tax_id && settings.tax_id !== '0' ? settings.tax_id : ''

  const gstApplicable = !!invoice.gst_hst_applicable
  const gstRate = invoice.gst_hst_rate || 0
  const gstAmount = gstApplicable ? invoice.subtotal * (gstRate / 100) : 0
  const otherTax = invoice.subtotal * ((invoice.tax_rate || 0) / 100)
  const isPaid = invoice.status === 'paid'

  const rows = items.map((item: any) => `
    <tr>
      <td class="desc">${escapeHtml(item.description)}</td>
      <td class="num">${Number(item.quantity).toFixed(2)}</td>
      <td class="num">${money(item.unit_price)}</td>
      <td class="num strong">${money(item.total)}</td>
    </tr>`).join('')

  const clientLines = [
    invoice.client_company && invoice.client_company !== invoice.client_name ? escapeHtml(invoice.client_company) : '',
    invoice.client_address ? escapeHtml(invoice.client_address).replace(/\n/g, '<br>') : '',
    invoice.client_email ? escapeHtml(invoice.client_email) : '',
  ].filter(Boolean).map(l => `<div class="muted">${l}</div>`).join('')

  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  @page { size: A4; margin: 0; }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif;
    color: #18181b; background: #fff; font-size: 12.5px; line-height: 1.5;
    -webkit-print-color-adjust: exact; print-color-adjust: exact;
    font-variant-numeric: tabular-nums;
  }
  .page { padding: 52px 56px 48px; }
  .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 32px; }
  .biz-name { font-size: 17px; font-weight: 600; letter-spacing: -0.01em; }
  .muted { color: #52525b; }
  .faint { color: #71717a; }
  .title { font-size: 30px; font-weight: 600; letter-spacing: -0.02em; text-align: right; line-height: 1.1; }
  .number { text-align: right; color: #52525b; margin-top: 4px; }
  .page { position: relative; }
  /* A rubber PAID stamp, matching the invoice page in the app */
  .stamp {
    position: absolute; top: 46px; right: 212px;
    display: inline-flex; flex-direction: column; align-items: center;
    padding: 6px 14px 4px; border-radius: 7px;
    border: 2.5px solid #1E7F3C; box-shadow: inset 0 0 0 2px #fff, inset 0 0 0 3px #1E7F3C;
    color: #1E7F3C; opacity: 0.88; transform: rotate(-8deg);
    font-family: "DIN Condensed", "DIN Alternate", "Helvetica Neue", Arial, sans-serif; font-weight: 700;
    text-transform: uppercase;
  }
  .stamp-label { font-size: 32px; line-height: 28px; letter-spacing: 0.1em; padding-left: 0.1em; }
  .stamp-date { font-size: 10.5px; line-height: 12px; letter-spacing: 0.16em; padding-left: 0.16em; margin-top: 3px; }
  .meta { display: flex; justify-content: space-between; gap: 32px; margin-top: 44px; }
  .label { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #a1a1aa; margin-bottom: 6px; }
  .client-name { font-weight: 600; font-size: 13.5px; }
  .facts { min-width: 230px; }
  .fact { display: flex; justify-content: space-between; gap: 24px; padding: 2px 0; }
  .fact.due { border-top: 1px solid #e4e4e7; margin-top: 6px; padding-top: 8px; font-weight: 600; font-size: 14px; }
  table { width: 100%; border-collapse: collapse; margin-top: 40px; }
  th { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #71717a; text-align: left; padding: 0 0 8px; border-bottom: 1.5px solid #18181b; }
  th.num, td.num { text-align: right; white-space: nowrap; }
  th.num { width: 90px; }
  td { padding: 10px 0; border-bottom: 1px solid #f0f0f2; vertical-align: top; }
  td.desc { padding-right: 24px; }
  td.num { color: #52525b; }
  td.strong { color: #18181b; font-weight: 500; }
  .totals { display: flex; justify-content: flex-end; margin-top: 20px; }
  .totals-box { width: 270px; }
  .t-row { display: flex; justify-content: space-between; padding: 3px 0; }
  .t-row.total { border-top: 1.5px solid #18181b; margin-top: 8px; padding-top: 10px; font-size: 16px; font-weight: 600; }
  .t-row.paid { color: #15803d; font-size: 11.5px; padding-top: 6px; }
  .notes { margin-top: 44px; padding-top: 16px; border-top: 1px solid #e4e4e7; }
  .notes p { white-space: pre-line; color: #52525b; }
  .footer { margin-top: 36px; color: #a1a1aa; font-size: 11px; }
</style>
</head>
<body>
  <div class="page">
    <div class="top">
      <div>
        ${settings.business_logo ? `<img src="${settings.business_logo}" alt="" style="max-height: 44px; margin-bottom: 12px;">` : ''}
        <div class="biz-name">${escapeHtml(businessName)}</div>
        ${businessAddress ? `<div class="muted" style="white-space: pre-line; margin-top: 2px;">${escapeHtml(businessAddress)}</div>` : ''}
        ${settings.business_email ? `<div class="muted">${escapeHtml(settings.business_email)}</div>` : ''}
        ${taxId ? `<div class="muted">Business number ${escapeHtml(taxId)}</div>` : ''}
        ${gstApplicable && invoice.gst_hst_number ? `<div class="muted">GST/HST ${escapeHtml(invoice.gst_hst_number)}</div>` : ''}
      </div>
      <div>
        <div class="title">Invoice</div>
        <div class="number">${escapeHtml(invoice.invoice_number)}</div>
      </div>
    </div>
    ${isPaid ? `<div class="stamp"><span class="stamp-label">Paid</span>${invoice.payment_date ? `<span class="stamp-date">${escapeHtml(formatDay(invoice.payment_date))}</span>` : ''}</div>` : ''}

    <div class="meta">
      <div>
        <div class="label">Bill to</div>
        <div class="client-name">${escapeHtml(invoice.client_name || '')}</div>
        ${clientLines}
      </div>
      <div class="facts">
        <div class="fact"><span class="faint">Issue date</span><span>${formatDay(invoice.issue_date)}</span></div>
        <div class="fact"><span class="faint">Due date</span><span>${formatDay(invoice.due_date)}</span></div>
        <div class="fact due"><span>${isPaid ? 'Amount paid' : 'Amount due'}</span><span>${money(invoice.total)}</span></div>
      </div>
    </div>

    <table>
      <thead>
        <tr>
          <th>Description</th>
          <th class="num">Hours</th>
          <th class="num">Rate</th>
          <th class="num">Amount</th>
        </tr>
      </thead>
      <tbody>${rows}</tbody>
    </table>

    <div class="totals">
      <div class="totals-box">
        <div class="t-row"><span class="faint">Subtotal</span><span>${money(invoice.subtotal)}</span></div>
        ${gstApplicable && gstRate > 0 ? `<div class="t-row"><span class="faint">GST/HST (${gstRate}%)</span><span>${money(gstAmount)}</span></div>` : ''}
        ${invoice.tax_rate > 0 ? `<div class="t-row"><span class="faint">Tax (${invoice.tax_rate}%)</span><span>${money(otherTax)}</span></div>` : ''}
        <div class="t-row total"><span>Total ${currency !== 'USD' ? `<span class="faint" style="font-size:11px;font-weight:500">${currency}</span>` : ''}</span><span>${money(invoice.total)}</span></div>
        ${isPaid && invoice.payment_date ? `<div class="t-row paid"><span>Paid ${formatDay(invoice.payment_date)}${invoice.payment_method ? ` by ${escapeHtml(invoice.payment_method)}` : ''}</span><span>${money(invoice.total)}</span></div>` : ''}
      </div>
    </div>

    ${invoice.notes ? `
    <div class="notes">
      <div class="label">Notes</div>
      <p>${escapeHtml(invoice.notes)}</p>
    </div>` : ''}

    <div class="footer">Thank you for your business.</div>
  </div>
</body>
</html>`
}

function escapeHtml(input: unknown): string {
  return String(input ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
