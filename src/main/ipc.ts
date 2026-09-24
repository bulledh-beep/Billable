import { ipcMain, dialog, BrowserWindow, nativeTheme } from 'electron'
import { app } from 'electron'
import fs from 'fs'
import path from 'path'
import * as db from './database'
import { generateInvoicePDF } from './pdf'
import { generateTaxSummaryPDF } from './tax-pdf'
import { generateCommissionInvoicePDF } from './commission-pdf'
import { TimerManager } from './timer-manager'
import { signOutOfContentHq, openContentUrlInBrowser } from './contenthq'
import {
  getPhoneSyncStatus, confirmConnect, disconnectPhone, syncPhoneNow, listPhoneActivity,
  getOpenAtLogin, setOpenAtLogin,
} from './phone-sync'
import {
  listProfiles, getActiveProfile, createProfile, renameProfile,
  updateProfileColor, deleteProfile, setActiveProfileId,
  setProfileAvatar, clearProfileAvatar,
} from './profiles'
import { checkForUpdates, downloadAndOpenUpdate, getCachedStatus, installUpdate, getReleaseNotesForTag, getAppBundlePath } from './updater'
import { getProfileDbPath, getActiveProfileId as activeProfileId } from './profiles'

export function registerIpcHandlers(timerManager: TimerManager) {
  // ========== Content HQ ==========
  ipcMain.handle('contenthq:sign-out', () => signOutOfContentHq())
  ipcMain.handle('contenthq:open-in-browser', (_, url: string) => openContentUrlInBrowser(url))

  // ========== Phone sync (through Content HQ) ==========
  ipcMain.handle('phone:status', () => getPhoneSyncStatus())
  ipcMain.handle('phone:confirm-connect', (_, requestId: string, accept: boolean) => confirmConnect(String(requestId), !!accept))
  ipcMain.handle('phone:disconnect', () => disconnectPhone())
  ipcMain.handle('phone:sync-now', () => syncPhoneNow())
  ipcMain.handle('phone:activity', (_, limit?: number) => listPhoneActivity(Number(limit) || 20))
  ipcMain.handle('phone:open-at-login', () => getOpenAtLogin())
  ipcMain.handle('phone:set-open-at-login', (_, enabled: boolean) => setOpenAtLogin(!!enabled))

  // ========== Appearance ==========
  // Keeps native pieces (sidebar material, menus, dialogs) in step with the
  // theme chosen in Settings.
  ipcMain.handle('appearance:set', (_, pref: string) => {
    nativeTheme.themeSource = pref === 'light' || pref === 'dark' ? pref : 'system'
  })

  // ========== Clients ==========
  ipcMain.handle('clients:list', () => db.listClients())
  ipcMain.handle('clients:get', (_, id: number) => db.getClient(id))
  ipcMain.handle('clients:create', (_, data) => db.createClient(data))
  ipcMain.handle('clients:update', (_, id: number, data) => db.updateClient(id, data))
  ipcMain.handle('clients:delete', (_, id: number) => db.deleteClient(id))
  ipcMain.handle('clients:merge', (_, sourceId: number, targetId: number) => db.mergeClients(sourceId, targetId))

  // ========== Projects ==========
  ipcMain.handle('projects:list', (_, clientId?: number) => db.listProjects(clientId))
  ipcMain.handle('projects:get', (_, id: number) => db.getProject(id))
  ipcMain.handle('projects:create', (_, data) => db.createProject(data))
  ipcMain.handle('projects:update', (_, id: number, data) => db.updateProject(id, data))
  ipcMain.handle('projects:delete', (_, id: number) => db.deleteProject(id))

  // ========== Time Entries ==========
  ipcMain.handle('time:list', (_, projectId?: number) => db.listTimeEntries(projectId))
  ipcMain.handle('time:get', (_, id: number) => db.getTimeEntry(id))
  ipcMain.handle('time:create', (_, data) => db.createTimeEntry(data))
  ipcMain.handle('time:update', (_, id: number, data) => db.updateTimeEntry(id, data))
  ipcMain.handle('time:delete', (_, id: number) => db.deleteTimeEntry(id))
  ipcMain.handle('time:start', (_, projectId: number, description?: string) => timerManager.start(projectId, description))
  ipcMain.handle('time:stop', () => timerManager.stop())
  ipcMain.handle('time:pause', () => timerManager.pause())
  ipcMain.handle('time:resume', () => timerManager.resume())
  ipcMain.handle('time:active', () => timerManager.getActive())
  ipcMain.handle('time:state', () => ({ active: timerManager.getActive(), paused: timerManager.getPaused() }))
  ipcMain.handle('time:set-billable', (_, ids: number[], billable: boolean) => db.setEntriesBillable(ids, billable))
  ipcMain.handle('time:invoiceable', (_, clientId: number, invoiceId?: number | null) => db.listInvoiceableEntries(clientId, invoiceId))

  // ========== Invoices ==========
  ipcMain.handle('invoices:list', (_, status?: string) => db.listInvoices(status))
  ipcMain.handle('invoices:get', (_, id: number) => db.getInvoice(id))
  ipcMain.handle('invoices:create', (_, data) => db.createInvoice(data))
  ipcMain.handle('invoices:update', (_, id: number, data) => db.updateInvoice(id, data))
  ipcMain.handle('invoices:delete', (_, id: number) => db.deleteInvoice(id))
  ipcMain.handle('invoices:mark-sent', (_, ids: number[]) => db.markInvoicesSent(ids))
  ipcMain.handle('invoices:mark-paid', (_, ids: number[], date?: string, method?: string | null) => db.markInvoicesPaid(ids, date, method))
  ipcMain.handle('invoices:mark-unpaid', (_, id: number) => db.markInvoiceUnpaid(id))

  // ========== Billing ==========
  ipcMain.handle('billing:overview', () => db.getBillingOverview())
  ipcMain.handle('billing:dismiss', (_, key: string) => db.dismissAttention(key))
  ipcMain.handle('invoices:export-pdf', async (_, id: number) => {
    return await generateInvoicePDF(id)
  })

  // ========== Unbilled Entries ==========
  ipcMain.handle('time:unbilled', (_, projectId: number) => db.getUnbilledEntries(projectId))
  ipcMain.handle('time:unbilled-multi', (_, projectIds: number[]) => db.getUnbilledEntriesForProjects(projectIds))
  ipcMain.handle('time:unbilled-by-client', (_, clientId: number) => db.getUnbilledEntriesByClient(clientId))

  // ========== Dashboard ==========
  ipcMain.handle('dashboard:stats', () => db.getDashboardStats())
  ipcMain.handle('dashboard:recent', () => db.getRecentEntries())

  // ========== Settings ==========
  ipcMain.handle('settings:get', () => db.getSettings())
  ipcMain.handle('settings:update', (_, data) => db.updateSettings(data))

  // Export the active profile's database as a single consistent file
  ipcMain.handle('settings:export-db', async () => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showSaveDialog(win, {
      title: 'Export Database',
      defaultPath: `billable-backup-${db.localToday()}.db`,
      filters: [{ name: 'SQLite Database', extensions: ['db'] }],
    })
    if (result.canceled || !result.filePath) return null
    if (fs.existsSync(result.filePath)) fs.unlinkSync(result.filePath)
    db.getDatabase().exec(`VACUUM INTO '${result.filePath.replace(/'/g, "''")}'`)
    return result.filePath
  })

  // Replace the active profile's database with a backup, keeping a copy of the current one
  ipcMain.handle('settings:import-db', async () => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showOpenDialog(win, {
      title: 'Import Database',
      filters: [{ name: 'SQLite Database', extensions: ['db'] }],
      properties: ['openFile'],
    })
    if (result.canceled || result.filePaths.length === 0) return null

    const source = result.filePaths[0]
    const header = Buffer.alloc(16)
    const fd = fs.openSync(source, 'r')
    fs.readSync(fd, header, 0, 16, 0)
    fs.closeSync(fd)
    if (header.toString('utf8', 0, 15) !== 'SQLite format 3') {
      throw new Error('That file is not a Billable database')
    }

    if (timerManager.getActive()) timerManager.stop()
    if (!db.backupDatabase('before-import')) {
      throw new Error("Couldn't back up your current data first, so nothing was replaced.")
    }
    db.closeDatabase()
    const target = getProfileDbPath(activeProfileId())
    for (const ext of ['-wal', '-shm']) {
      if (fs.existsSync(target + ext)) fs.unlinkSync(target + ext)
    }
    fs.copyFileSync(source, target)
    app.relaunch()
    app.exit()
    return true
  })

  // ========== Reports ==========
  ipcMain.handle('reports:hours-by-project', (_, startDate: string, endDate: string) =>
    db.hoursByProject(startDate, endDate)
  )
  ipcMain.handle('reports:hours-by-client', (_, startDate: string, endDate: string) =>
    db.hoursByClient(startDate, endDate)
  )
  ipcMain.handle('reports:earnings-by-month', (_, startDate: string, endDate: string) =>
    db.earningsByMonth(startDate, endDate)
  )
  ipcMain.handle('reports:export-csv', async (_, data: any[], filename: string) => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showSaveDialog(win, {
      title: 'Export CSV',
      defaultPath: filename,
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    })
    if (result.canceled || !result.filePath) return null
    const headers = Object.keys(data[0] || {})
    const csv = [
      headers.join(','),
      ...data.map(row => headers.map(h => `"${String(row[h] ?? '').replace(/"/g, '""')}"`).join(','))
    ].join('\n')
    fs.writeFileSync(result.filePath, csv)
    return result.filePath
  })

  // ========== Tax Settings ==========
  ipcMain.handle('tax:get-settings', () => db.getTaxSettings())
  ipcMain.handle('tax:save-settings', (_, data) => db.saveTaxSettings(data))

  // ========== Tax Overview & Exports ==========
  ipcMain.handle('tax:get-overview', (_, taxYear: number) => db.getTaxOverview(taxYear))
  ipcMain.handle('tax:export-summary-pdf', async (_, taxYear: number) => {
    return await generateTaxSummaryPDF(taxYear)
  })
  ipcMain.handle('tax:export-invoices-csv', async (_, taxYear: number) => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const data = db.listInvoicesByYear(taxYear) as any[]
    if (!data.length) return null
    const result = await dialog.showSaveDialog(win, {
      title: 'Export Invoice CSV',
      defaultPath: `invoices-${taxYear}.csv`,
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    })
    if (result.canceled || !result.filePath) return null
    const cols = [
      'invoice_number', 'issue_date', 'due_date', 'tax_year', 'client_name',
      'status', 'subtotal', 'gst_hst_applicable', 'gst_hst_rate', 'gst_hst_amount',
      'tax_rate', 'total', 'currency', 'payment_date', 'payment_method',
    ]
    const csv = [
      cols.join(','),
      ...data.map((r: any) => cols.map(c =>
        `"${String(r[c] ?? '').replace(/"/g, '""')}"`
      ).join(',')),
    ].join('\n')
    fs.writeFileSync(result.filePath, csv)
    return result.filePath
  })
  ipcMain.handle('tax:export-expenses-csv', async (_, taxYear: number) => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const data = db.listExpenses(taxYear) as any[]
    if (!data.length) return null
    const result = await dialog.showSaveDialog(win, {
      title: 'Export Expense CSV',
      defaultPath: `expenses-${taxYear}.csv`,
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    })
    if (result.canceled || !result.filePath) return null
    const cols = ['date', 'category', 'description', 'amount', 'tax_year', 'receipt_note']
    const csv = [
      cols.join(','),
      ...data.map((r: any) => cols.map(c =>
        `"${String(r[c] ?? '').replace(/"/g, '""')}"`
      ).join(',')),
    ].join('\n')
    fs.writeFileSync(result.filePath, csv)
    return result.filePath
  })

  // ========== Expenses ==========
  ipcMain.handle('expense:list', (_, taxYear?: number) => db.listExpenses(taxYear))
  ipcMain.handle('expense:get', (_, id: number) => db.getExpense(id))
  ipcMain.handle('expense:create', (_, data) => db.createExpense(data))
  ipcMain.handle('expense:update', (_, id: number, data) => db.updateExpense(id, data))
  ipcMain.handle('expense:delete', (_, id: number) => db.deleteExpense(id))

  // ========== Commissions ==========
  ipcMain.handle('commissions:list', () => db.listCommissions())
  ipcMain.handle('commissions:get', (_, id: number) => db.getCommission(id))
  ipcMain.handle('commissions:create', (_, data) => db.createCommission(data))
  ipcMain.handle('commissions:update', (_, id: number, data) => db.updateCommission(id, data))
  ipcMain.handle('commissions:patch', (_, id: number, patch) => db.patchCommission(id, patch))
  ipcMain.handle('commissions:bulk-patch', (_, ids: number[], patch) => db.bulkPatchCommissions(ids, patch))
  ipcMain.handle('commissions:delete', (_, id: number) => db.deleteCommission(id))

  // ========== Commission Invoices ==========
  ipcMain.handle('commission-invoices:list', () => db.listCommissionInvoices())
  ipcMain.handle('commission-invoices:get', (_, id: number) => db.getCommissionInvoice(id))
  ipcMain.handle('commission-invoices:create', (_, data) => db.createCommissionInvoice(data))
  ipcMain.handle('commission-invoices:update-status', (_, id: number, status: string) => db.updateCommissionInvoiceStatus(id, status))
  ipcMain.handle('commission-invoices:delete', (_, id: number) => db.deleteCommissionInvoice(id))
  ipcMain.handle('commission-invoices:export-pdf', async (_, id: number) => {
    return await generateCommissionInvoicePDF(id)
  })

  // ========== Profiles ==========
  ipcMain.handle('profile:list', () => ({
    profiles: listProfiles(),
    active: getActiveProfile(),
  }))
  ipcMain.handle('profile:active', () => getActiveProfile())
  ipcMain.handle('profile:create', (_, name: string, color?: string) => createProfile(name, color))
  ipcMain.handle('profile:rename', (_, id: string, name: string) => renameProfile(id, name))
  ipcMain.handle('profile:set-color', (_, id: string, color: string) => updateProfileColor(id, color))
  ipcMain.handle('profile:switch', (_, id: string) => {
    // Stop any running timer in the current profile (its DB will be closed)
    if (timerManager.getActive()) {
      timerManager.stop()
    }
    setActiveProfileId(id)
    db.initDatabase(id) // reopens against the new profile's DB
    timerManager.syncFromDatabase() // pick up any timer in the new DB
    for (const win of BrowserWindow.getAllWindows()) {
      win.webContents.reload()
    }
    return getActiveProfile()
  })
  ipcMain.handle('profile:delete', (_, id: string) => {
    deleteProfile(id)
    return { profiles: listProfiles(), active: getActiveProfile() }
  })
  ipcMain.handle('profile:set-avatar', (_, id: string, sourcePath: string) =>
    setProfileAvatar(id, sourcePath)
  )
  ipcMain.handle('profile:pick-avatar', async (event, id: string) => {
    const win = BrowserWindow.fromWebContents(event.sender) || undefined
    const result = await dialog.showOpenDialog(win!, {
      title: 'Choose Profile Picture',
      filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'heic', 'webp', 'gif', 'tiff', 'bmp'] }],
      properties: ['openFile'],
    })
    if (result.canceled || result.filePaths.length === 0) return null
    return setProfileAvatar(id, result.filePaths[0])
  })
  ipcMain.handle('profile:clear-avatar', (_, id: string) => clearProfileAvatar(id))

  // ========== Updater ==========
  ipcMain.handle('updater:current-version', () => app.getVersion())
  ipcMain.handle('updater:cached', () => getCachedStatus())
  ipcMain.handle('updater:check', async (_, force: boolean = false) => {
    return await checkForUpdates(force)
  })
  ipcMain.handle('updater:download', async (event, url: string) => {
    const win = BrowserWindow.fromWebContents(event.sender) || undefined
    return await downloadAndOpenUpdate(url, win)
  })
  ipcMain.handle('updater:can-install', () => !!getAppBundlePath())
  ipcMain.handle('updater:install', async (event, url: string) => {
    const win = BrowserWindow.fromWebContents(event.sender) || undefined
    await installUpdate(url, win)
    // Give the renderer a beat to show the "installing — quitting" state,
    // then quit so the detached helper can take over.
    setTimeout(() => app.quit(), 500)
    return true
  })
  ipcMain.handle('updater:release-notes', async (_, version: string) => {
    return await getReleaseNotesForTag(version)
  })

  // ========== Dialogs ==========
  ipcMain.handle('dialog:open-file', async (_, options) => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showOpenDialog(win, options)
    return result.canceled ? null : result.filePaths[0]
  })

  ipcMain.handle('dialog:save-file', async (_, options) => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showSaveDialog(win, options)
    return result.canceled ? null : result.filePath
  })
}
