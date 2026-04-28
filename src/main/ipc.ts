import { ipcMain, dialog, BrowserWindow } from 'electron'
import { app } from 'electron'
import fs from 'fs'
import path from 'path'
import * as db from './database'
import { generateInvoicePDF } from './pdf'
import { TimerManager } from './timer-manager'

export function registerIpcHandlers(timerManager: TimerManager) {
  // ========== Clients ==========
  ipcMain.handle('clients:list', () => db.listClients())
  ipcMain.handle('clients:get', (_, id: number) => db.getClient(id))
  ipcMain.handle('clients:create', (_, data) => db.createClient(data))
  ipcMain.handle('clients:update', (_, id: number, data) => db.updateClient(id, data))
  ipcMain.handle('clients:delete', (_, id: number) => db.deleteClient(id))

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
  ipcMain.handle('time:active', () => timerManager.getActive())

  // ========== Invoices ==========
  ipcMain.handle('invoices:list', (_, status?: string) => db.listInvoices(status))
  ipcMain.handle('invoices:get', (_, id: number) => db.getInvoice(id))
  ipcMain.handle('invoices:create', (_, data) => db.createInvoice(data))
  ipcMain.handle('invoices:update', (_, id: number, data) => db.updateInvoice(id, data))
  ipcMain.handle('invoices:delete', (_, id: number) => db.deleteInvoice(id))
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

  ipcMain.handle('settings:export-db', async () => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showSaveDialog(win, {
      title: 'Export Database',
      defaultPath: `billable-backup-${new Date().toISOString().split('T')[0]}.db`,
      filters: [{ name: 'SQLite Database', extensions: ['db'] }],
    })
    if (result.canceled || !result.filePath) return null
    const dbPath = path.join(app.getPath('userData'), 'billable.db')
    fs.copyFileSync(dbPath, result.filePath)
    return result.filePath
  })

  ipcMain.handle('settings:import-db', async () => {
    const win = BrowserWindow.getFocusedWindow()
    if (!win) return null
    const result = await dialog.showOpenDialog(win, {
      title: 'Import Database',
      filters: [{ name: 'SQLite Database', extensions: ['db'] }],
      properties: ['openFile'],
    })
    if (result.canceled || result.filePaths.length === 0) return null
    const dbPath = path.join(app.getPath('userData'), 'billable.db')
    fs.copyFileSync(result.filePaths[0], dbPath)
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

  // ========== Expenses ==========
  ipcMain.handle('expense:list', (_, taxYear?: number) => db.listExpenses(taxYear))
  ipcMain.handle('expense:get', (_, id: number) => db.getExpense(id))
  ipcMain.handle('expense:create', (_, data) => db.createExpense(data))
  ipcMain.handle('expense:update', (_, id: number, data) => db.updateExpense(id, data))
  ipcMain.handle('expense:delete', (_, id: number) => db.deleteExpense(id))

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
