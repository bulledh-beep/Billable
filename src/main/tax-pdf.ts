import { BrowserWindow, dialog, shell, app } from 'electron'
import path from 'path'
import fs from 'fs'
import { getTaxOverview, getTaxSettings, getSettings } from './database'

const CATEGORY_LABELS: Record<string, string> = {
  equipment: 'Equipment / Hardware',
  software: 'Software / Subscriptions',
  home_office: 'Home Office',
  phone_internet: 'Phone & Internet',
  travel: 'Travel',
  meals: 'Meals & Entertainment',
  professional_development: 'Professional Development',
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
  const accent = '#F5A623'
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
        <td style="padding: 10px 14px; border-bottom: 1px solid #e5e7eb; font-size: 13px;">
          ${escape(CATEGORY_LABELS[e.category] || e.category)}
          <span style="color: #9ca3af; font-size: 11px; margin-left: 6px;">(${e.count})</span>
        </td>
        <td style="padding: 10px 14px; border-bottom: 1px solid #e5e7eb; font-size: 13px; font-family: 'SF Mono', monospace; text-align: right;">${fmt(e.total)}</td>
      </tr>
    `).join('')
    : `<tr><td colspan="2" style="padding: 14px; text-align: center; color: #9ca3af; font-size: 13px;">No expenses logged for ${taxYear}.</td></tr>`

  return `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Helvetica Neue', Helvetica, Arial, sans-serif;
      color: #1a1a1a;
      background: white;
      -webkit-print-color-adjust: exact;
      print-color-adjust: exact;
    }
    .row { display: flex; justify-content: space-between; padding: 8px 0; font-size: 14px; }
    .row .label { color: #4b5563; }
    .row .num { font-family: 'SF Mono', monospace; }
    h1 { font-size: 28px; font-weight: 700; color: ${accent}; letter-spacing: 1px; }
    h2 { font-size: 12px; text-transform: uppercase; letter-spacing: 1.5px; color: #6b7280; margin-bottom: 12px; }
    .panel { padding: 20px; border: 1px solid #e5e7eb; border-radius: 8px; margin-bottom: 20px; background: #fafafa; }
    .total-row { border-top: 2px solid #1a1a1a; padding-top: 12px; margin-top: 8px; font-size: 16px; font-weight: 700; }
  </style>
</head>
<body>
  <div style="padding: 0;">
    <div style="height: 4px; background: ${accent};"></div>
    <div style="padding: 48px;">
      <!-- Header -->
      <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 32px;">
        <div>
          <h1>TAX SUMMARY</h1>
          <p style="font-size: 14px; color: #6b7280; margin-top: 6px;">For tax year ${taxYear}</p>
        </div>
        <div style="text-align: right;">
          <p style="font-size: 16px; font-weight: 600;">${escape(businessName)}</p>
          ${businessAddress ? `<p style="font-size: 12px; color: #6b7280; white-space: pre-line; margin-top: 4px;">${escape(businessAddress)}</p>` : ''}
          ${province ? `<p style="font-size: 12px; color: #6b7280;">Province: ${escape(province)}</p>` : ''}
          ${gstNumber ? `<p style="font-size: 12px; color: #6b7280; font-family: 'SF Mono', monospace;">GST/HST: ${escape(gstNumber)}</p>` : ''}
          <p style="font-size: 11px; color: #9ca3af; margin-top: 6px;">Generated ${new Date().toLocaleDateString('en-CA')}</p>
        </div>
      </div>

      <!-- Income -->
      <div class="panel">
        <h2>Income</h2>
        <div class="row">
          <span class="label">Total invoiced (${overview.invoice_count} invoice${overview.invoice_count === 1 ? '' : 's'})</span>
          <span class="num">${fmt(overview.total_invoiced)}</span>
        </div>
        <div class="row">
          <span class="label">Paid (${overview.paid_count})</span>
          <span class="num">${fmt(overview.total_paid)}</span>
        </div>
        <div class="row">
          <span class="label">Outstanding</span>
          <span class="num" style="color: #b45309;">${fmt(overview.total_outstanding)}</span>
        </div>
        <div class="row">
          <span class="label">GST/HST collected (paid invoices)</span>
          <span class="num">${fmt(overview.gst_collected_paid)}</span>
        </div>
        <div class="row total-row">
          <span>Net business income (paid − GST − expenses)</span>
          <span class="num" style="color: ${accent};">${fmt(netIncome)}</span>
        </div>
      </div>

      <!-- Expenses -->
      <div class="panel" style="padding: 0; overflow: hidden;">
        <div style="padding: 20px 20px 0;">
          <h2>Expenses by Category</h2>
        </div>
        <table style="width: 100%; border-collapse: collapse; margin-top: 4px;">
          <tbody>${expenseRows}</tbody>
          <tfoot>
            <tr>
              <td style="padding: 14px; font-size: 14px; font-weight: 600;">Total deductible</td>
              <td style="padding: 14px; font-size: 14px; font-weight: 700; font-family: 'SF Mono', monospace; text-align: right;">${fmt(overview.total_expenses)}</td>
            </tr>
          </tfoot>
        </table>
      </div>

      <!-- Estimates -->
      <div class="panel" style="background: #fff8eb; border-color: ${accent};">
        <h2>Set-Aside Estimates</h2>
        <div class="row">
          <span class="label">GST/HST collected to remit</span>
          <span class="num">${fmt(overview.gst_collected_paid)}</span>
        </div>
        <div class="row">
          <span class="label">Estimated income tax @ ${incomeTaxBracket}%</span>
          <span class="num">${fmt(incomeTaxEstimate)}</span>
        </div>
        <div class="row total-row">
          <span>Total to set aside (estimate)</span>
          <span class="num" style="color: ${accent};">${fmt(overview.gst_collected_paid + incomeTaxEstimate)}</span>
        </div>
      </div>

      <!-- Disclaimer -->
      <div style="margin-top: 32px; padding: 16px; background: #f3f4f6; border-radius: 8px;">
        <p style="font-size: 11px; color: #6b7280; line-height: 1.6;">
          <strong style="color: #4b5563;">Disclaimer.</strong>
          This summary is generated from the records you keep in Billable and is intended to help with bookkeeping. It is an estimate only and does not constitute tax, legal, or financial advice. Please consult a CPA or qualified tax professional before filing or remitting.
        </p>
      </div>
    </div>
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
