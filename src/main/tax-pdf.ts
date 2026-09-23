import { BrowserWindow, dialog, shell, app } from 'electron'
import path from 'path'
import fs from 'fs'
import { getTaxOverview, getTaxSettings, getSettings } from './database'

const CATEGORY_LABELS: Record<string, string> = {
  equipment: 'Equipment and hardware',
  software: 'Software and subscriptions',
  home_office: 'Home office',
  phone_internet: 'Phone and internet',
  travel: 'Travel',
  meals: 'Meals and entertainment',
  professional_development: 'Professional development',
  other: 'Other',
}

export async function generateTaxSummaryPDF(taxYear: number): Promise<string | null> {
  const overview = getTaxOverview(taxYear)
  const taxSettings = getTaxSettings()
  const appSettings = getSettings()

  const html = generateTaxSummaryHTML(overview, taxSettings, appSettings, taxYear)

  const pdfWindow = new BrowserWindow({
    width: 800,
    height: 1130,
    show: false,
    webPreferences: { nodeIntegration: false, contextIsolation: true },
  })

  try {
    await pdfWindow.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(html)}`)
    await new Promise(resolve => setTimeout(resolve, 700))

    const pdfData = await pdfWindow.webContents.printToPDF({
      pageSize: 'A4',
      printBackground: true,
      margins: { top: 0, bottom: 0, left: 0, right: 0 },
    })

    // Dev/test hook: write straight to a folder instead of asking
    if (process.env.BILLABLE_EXPORT_DIR) {
      const out = path.join(process.env.BILLABLE_EXPORT_DIR, `tax-summary-${taxYear}.pdf`)
      fs.writeFileSync(out, Buffer.from(pdfData))
      return out
    }

    const defaultPath = path.join(app.getPath('downloads'), `tax-summary-${taxYear}.pdf`)
    const { filePath: savePath } = await dialog.showSaveDialog({
      title: 'Save Tax Summary PDF',
      defaultPath,
      filters: [{ name: 'PDF', extensions: ['pdf'] }],
    })

    if (!savePath) return null
    fs.writeFileSync(savePath, Buffer.from(pdfData))
    shell.openPath(savePath)
    return savePath
  } finally {
    pdfWindow.destroy()
  }
}

function generateTaxSummaryHTML(
  overview: ReturnType<typeof getTaxOverview>,
  taxSettings: any,
  appSettings: any,
  taxYear: number,
): string {
  const currency = taxSettings?.currency || appSettings?.default_currency || 'CAD'
  const businessName = taxSettings?.business_name || appSettings?.business_name || 'Your Business'
  const businessAddress = taxSettings?.business_address || appSettings?.business_address || ''
  const province = taxSettings?.province || ''
  const gstNumber = taxSettings?.gst_hst_number || ''
  const incomeTaxBracket = taxSettings?.income_tax_bracket || 0

  const fmt = (n: number) =>
    new Intl.NumberFormat('en-CA', { style: 'currency', currency }).format(n || 0)

  // Net income before tax: paid - GST owed - expenses
  const netIncome =
    overview.total_paid - overview.gst_collected_paid - overview.total_expenses
  const incomeTaxEstimate = Math.max(0, netIncome) * (incomeTaxBracket / 100)

  const expenseRows = overview.expenses_by_category.length
    ? overview.expenses_by_category.map(e => `
      <tr>
        <td>${escape(CATEGORY_LABELS[e.category] || e.category)} <span class="faint">${e.count} item${e.count === 1 ? '' : 's'}</span></td>
        <td class="num">${fmt(e.total)}</td>
      </tr>`).join('')
    : `<tr><td colspan="2" class="faint">No expenses logged for ${taxYear}.</td></tr>`

  const now = new Date()
  const generated = now.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })

  return `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    @page { size: A4; margin: 0; }
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif; color: #18181b; background: #fff; font-size: 12.5px; line-height: 1.5; font-variant-numeric: tabular-nums; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
    .page { padding: 52px 56px 48px; }
    .top { display: flex; justify-content: space-between; align-items: flex-start; gap: 32px; margin-bottom: 36px; }
    .title { font-size: 28px; font-weight: 600; letter-spacing: -0.02em; line-height: 1.1; }
    .muted { color: #52525b; }
    .faint { color: #a1a1aa; }
    .section { margin-bottom: 26px; }
    .label { font-size: 10px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: #71717a; padding-bottom: 8px; border-bottom: 1.5px solid #18181b; margin-bottom: 2px; }
    .row { display: flex; justify-content: space-between; padding: 8px 0; border-bottom: 1px solid #f0f0f2; }
    .row span:first-child { color: #52525b; }
    .row.total { border-bottom: none; border-top: 1.5px solid #18181b; margin-top: 4px; padding-top: 10px; font-size: 15px; font-weight: 600; }
    .row.total span:first-child { color: #18181b; }
    table { width: 100%; border-collapse: collapse; }
    td { padding: 8px 0; border-bottom: 1px solid #f0f0f2; }
    td.num { text-align: right; }
    tfoot td { border-bottom: none; border-top: 1.5px solid #18181b; padding-top: 10px; font-weight: 600; font-size: 14px; }
    .setaside { background: #fff8eb; border: 1px solid #f5d9a8; border-radius: 8px; padding: 16px 18px; }
    .setaside .row { border-bottom-color: #f5e6c8; }
    .note { margin-top: 30px; font-size: 11px; color: #71717a; line-height: 1.6; }
  </style>
</head>
<body>
  <div class="page">
    <div class="top">
      <div>
        <div class="title">Tax summary</div>
        <div class="muted" style="margin-top: 6px;">For the ${taxYear} tax year</div>
      </div>
      <div style="text-align: right;">
        <div style="font-size: 14px; font-weight: 600;">${escape(businessName)}</div>
        ${businessAddress ? `<div class="muted" style="white-space: pre-line;">${escape(businessAddress)}</div>` : ''}
        ${province ? `<div class="muted">Province: ${escape(province)}</div>` : ''}
        ${gstNumber ? `<div class="muted">GST/HST ${escape(gstNumber)}</div>` : ''}
        <div class="faint" style="margin-top: 6px; font-size: 11px;">Prepared ${generated}</div>
      </div>
    </div>

    <div class="section">
      <div class="label">Income</div>
      <div class="row"><span>Invoiced, ${overview.invoice_count} invoice${overview.invoice_count === 1 ? '' : 's'}</span><span>${fmt(overview.total_invoiced)}</span></div>
      <div class="row"><span>Paid, ${overview.paid_count} invoice${overview.paid_count === 1 ? '' : 's'}</span><span>${fmt(overview.total_paid)}</span></div>
      <div class="row"><span>Outstanding</span><span>${fmt(overview.total_outstanding)}</span></div>
      <div class="row"><span>GST/HST collected on paid invoices</span><span>${fmt(overview.gst_collected_paid)}</span></div>
      <div class="row total"><span>Net business income</span><span>${fmt(netIncome)}</span></div>
      <div class="faint" style="font-size: 11px; margin-top: 4px;">Paid invoices, less GST/HST collected, less deductible expenses.</div>
    </div>

    <div class="section">
      <div class="label">Expenses by category</div>
      <table>
        <tbody>${expenseRows}</tbody>
        <tfoot><tr><td>Total deductible</td><td class="num">${fmt(overview.total_expenses)}</td></tr></tfoot>
      </table>
    </div>

    <div class="setaside">
      <div style="font-weight: 600; margin-bottom: 4px;">Suggested set-aside</div>
      <div class="row"><span>GST/HST to remit</span><span>${fmt(overview.gst_collected_paid)}</span></div>
      <div class="row"><span>Income tax at ${incomeTaxBracket}%</span><span>${fmt(incomeTaxEstimate)}</span></div>
      <div class="row total" style="border-top-color: #18181b;"><span>Total to set aside</span><span>${fmt(overview.gst_collected_paid + incomeTaxEstimate)}</span></div>
    </div>

    <p class="note">
      Prepared from the records kept in Billable to help with bookkeeping. These are estimates, not tax, legal, or financial advice. Check with an accountant before filing or remitting.
    </p>
  </div>
</body>
</html>`
}

function escape(str: string): string {
  return String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
