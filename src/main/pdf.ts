import { BrowserWindow, dialog, shell } from 'electron'
import path from 'path'
import { app } from 'electron'
import fs from 'fs'
import { getInvoice, getSettings, updateInvoice } from './database'

export async function generateInvoicePDF(invoiceId: number): Promise<string> {
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

function generateInvoiceHTML(invoice: any, settings: any): string {
  const items = invoice.items || []
  const accentColor = '#F5A623'

  const formatMoney = (amount: number) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: settings.default_currency || 'USD',
    }).format(amount)
  }

  const itemRows = items.map((item: any) => `
    <tr>
      <td style="padding: 12px 16px; border-bottom: 1px solid #e5e7eb; text-align: left; font-size: 14px;">${escapeHtml(item.description)}</td>
      <td style="padding: 12px 16px; border-bottom: 1px solid #e5e7eb; text-align: center; font-size: 14px; font-family: 'SF Mono', monospace;">${item.quantity.toFixed(2)}</td>
      <td style="padding: 12px 16px; border-bottom: 1px solid #e5e7eb; text-align: right; font-size: 14px; font-family: 'SF Mono', monospace;">${formatMoney(item.unit_price)}</td>
      <td style="padding: 12px 16px; border-bottom: 1px solid #e5e7eb; text-align: right; font-size: 14px; font-family: 'SF Mono', monospace; font-weight: 500;">${formatMoney(item.total)}</td>
    </tr>
  `).join('')

  const taxAmount = invoice.subtotal * (invoice.tax_rate / 100)

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
  </style>
</head>
<body>
  <div style="padding: 0;">
    <!-- Accent Bar -->
    <div style="height: 4px; background: ${accentColor};"></div>

    <div style="padding: 48px;">
      <!-- Header -->
      <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 48px;">
        <div>
          ${settings.business_logo ? `<img src="${settings.business_logo}" alt="Logo" style="max-height: 48px; margin-bottom: 12px;">` : ''}
          <h2 style="font-size: 18px; font-weight: 600; margin-bottom: 4px;">${escapeHtml(settings.business_name || 'Your Business')}</h2>
          <p style="font-size: 13px; color: #6b7280; white-space: pre-line;">${escapeHtml(settings.business_address || '')}</p>
          ${settings.business_email ? `<p style="font-size: 13px; color: #6b7280;">${escapeHtml(settings.business_email)}</p>` : ''}
          ${settings.tax_id ? `<p style="font-size: 13px; color: #6b7280;">Tax ID: ${escapeHtml(settings.tax_id)}</p>` : ''}
        </div>
        <div style="text-align: right;">
          <h1 style="font-size: 36px; font-weight: 700; color: ${accentColor}; letter-spacing: 2px;">INVOICE</h1>
          <p style="font-size: 14px; color: #6b7280; margin-top: 8px;">${escapeHtml(invoice.invoice_number)}</p>
        </div>
      </div>

      <!-- Invoice Details & Client -->
      <div style="display: flex; justify-content: space-between; margin-bottom: 40px;">
        <div>
          <h3 style="font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #9ca3af; margin-bottom: 8px;">Bill To</h3>
          <p style="font-size: 15px; font-weight: 600;">${escapeHtml(invoice.client_name || '')}</p>
          ${invoice.client_company ? `<p style="font-size: 13px; color: #6b7280;">${escapeHtml(invoice.client_company)}</p>` : ''}
          ${invoice.client_address ? `<p style="font-size: 13px; color: #6b7280; white-space: pre-line;">${escapeHtml(invoice.client_address)}</p>` : ''}
          ${invoice.client_email ? `<p style="font-size: 13px; color: #6b7280;">${escapeHtml(invoice.client_email)}</p>` : ''}
        </div>
        <div style="text-align: right;">
          <div style="margin-bottom: 12px;">
            <span style="font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #9ca3af;">Issue Date</span>
            <p style="font-size: 14px; font-weight: 500;">${invoice.issue_date}</p>
          </div>
          <div style="margin-bottom: 12px;">
            <span style="font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #9ca3af;">Due Date</span>
            <p style="font-size: 14px; font-weight: 500;">${invoice.due_date}</p>
          </div>
          <div>
            <span style="font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #9ca3af;">Amount Due</span>
            <p style="font-size: 22px; font-weight: 700; color: ${accentColor};">${formatMoney(invoice.total)}</p>
          </div>
        </div>
      </div>

      <!-- Line Items Table -->
      <table style="width: 100%; border-collapse: collapse; margin-bottom: 32px;">
        <thead>
          <tr style="background: #f9fafb;">
            <th style="padding: 12px 16px; text-align: left; font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #6b7280; border-bottom: 2px solid #e5e7eb;">Description</th>
            <th style="padding: 12px 16px; text-align: center; font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #6b7280; border-bottom: 2px solid #e5e7eb;">Hours</th>
            <th style="padding: 12px 16px; text-align: right; font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #6b7280; border-bottom: 2px solid #e5e7eb;">Rate</th>
            <th style="padding: 12px 16px; text-align: right; font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #6b7280; border-bottom: 2px solid #e5e7eb;">Amount</th>
          </tr>
        </thead>
        <tbody>
          ${itemRows}
        </tbody>
      </table>

      <!-- Totals -->
      <div style="display: flex; justify-content: flex-end;">
        <div style="width: 280px;">
          <div style="display: flex; justify-content: space-between; padding: 8px 0; font-size: 14px;">
            <span style="color: #6b7280;">Subtotal</span>
            <span style="font-family: 'SF Mono', monospace;">${formatMoney(invoice.subtotal)}</span>
          </div>
          ${invoice.tax_rate > 0 ? `
          <div style="display: flex; justify-content: space-between; padding: 8px 0; font-size: 14px;">
            <span style="color: #6b7280;">Tax (${invoice.tax_rate}%)</span>
            <span style="font-family: 'SF Mono', monospace;">${formatMoney(taxAmount)}</span>
          </div>
          ` : ''}
          <div style="display: flex; justify-content: space-between; padding: 12px 0; font-size: 18px; font-weight: 700; border-top: 2px solid #1a1a1a; margin-top: 8px;">
            <span>Total</span>
            <span style="font-family: 'SF Mono', monospace;">${formatMoney(invoice.total)}</span>
          </div>
        </div>
      </div>

      <!-- Notes -->
      ${invoice.notes ? `
      <div style="margin-top: 48px; padding: 20px; background: #f9fafb; border-radius: 8px;">
        <h3 style="font-size: 11px; text-transform: uppercase; letter-spacing: 1px; color: #9ca3af; margin-bottom: 8px;">Notes & Payment Instructions</h3>
        <p style="font-size: 13px; color: #4b5563; white-space: pre-line;">${escapeHtml(invoice.notes)}</p>
      </div>
      ` : ''}
    </div>
  </div>
</body>
</html>`
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
