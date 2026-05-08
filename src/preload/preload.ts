import { contextBridge, ipcRenderer } from 'electron'

const api = {
  // Clients
  clients: {
    list: () => ipcRenderer.invoke('clients:list'),
    get: (id: number) => ipcRenderer.invoke('clients:get', id),
    create: (data: any) => ipcRenderer.invoke('clients:create', data),
    update: (id: number, data: any) => ipcRenderer.invoke('clients:update', id, data),
    delete: (id: number) => ipcRenderer.invoke('clients:delete', id),
  },
  // Projects
  projects: {
    list: (clientId?: number) => ipcRenderer.invoke('projects:list', clientId),
    get: (id: number) => ipcRenderer.invoke('projects:get', id),
    create: (data: any) => ipcRenderer.invoke('projects:create', data),
    update: (id: number, data: any) => ipcRenderer.invoke('projects:update', id, data),
    delete: (id: number) => ipcRenderer.invoke('projects:delete', id),
  },
  // Time entries
  time: {
    list: (projectId?: number) => ipcRenderer.invoke('time:list', projectId),
    get: (id: number) => ipcRenderer.invoke('time:get', id),
    create: (data: any) => ipcRenderer.invoke('time:create', data),
    update: (id: number, data: any) => ipcRenderer.invoke('time:update', id, data),
    delete: (id: number) => ipcRenderer.invoke('time:delete', id),
    start: (projectId: number, description?: string) => ipcRenderer.invoke('time:start', projectId, description),
    stop: (id: number) => ipcRenderer.invoke('time:stop', id),
    active: () => ipcRenderer.invoke('time:active'),
    unbilled: (projectId: number) => ipcRenderer.invoke('time:unbilled', projectId),
    unbilledMulti: (projectIds: number[]) => ipcRenderer.invoke('time:unbilled-multi', projectIds),
    unbilledByClient: (clientId: number) => ipcRenderer.invoke('time:unbilled-by-client', clientId),
  },
  // Invoices
  invoices: {
    list: (status?: string) => ipcRenderer.invoke('invoices:list', status),
    get: (id: number) => ipcRenderer.invoke('invoices:get', id),
    create: (data: any) => ipcRenderer.invoke('invoices:create', data),
    update: (id: number, data: any) => ipcRenderer.invoke('invoices:update', id, data),
    delete: (id: number) => ipcRenderer.invoke('invoices:delete', id),
    exportPDF: (id: number) => ipcRenderer.invoke('invoices:export-pdf', id),
  },
  // Dashboard
  dashboard: {
    stats: () => ipcRenderer.invoke('dashboard:stats'),
    recent: () => ipcRenderer.invoke('dashboard:recent'),
  },
  // Settings
  settings: {
    get: () => ipcRenderer.invoke('settings:get'),
    update: (data: any) => ipcRenderer.invoke('settings:update', data),
    exportDB: () => ipcRenderer.invoke('settings:export-db'),
    importDB: () => ipcRenderer.invoke('settings:import-db'),
  },
  // Reports
  reports: {
    hoursByProject: (start: string, end: string) => ipcRenderer.invoke('reports:hours-by-project', start, end),
    hoursByClient: (start: string, end: string) => ipcRenderer.invoke('reports:hours-by-client', start, end),
    earningsByMonth: (start: string, end: string) => ipcRenderer.invoke('reports:earnings-by-month', start, end),
    exportCSV: (data: any[], filename: string) => ipcRenderer.invoke('reports:export-csv', data, filename),
  },
  // Dialog
  dialog: {
    openFile: (options: any) => ipcRenderer.invoke('dialog:open-file', options),
    saveFile: (options: any) => ipcRenderer.invoke('dialog:save-file', options),
  },
  // Updater
  updater: {
    currentVersion: () => ipcRenderer.invoke('updater:current-version'),
    cached: () => ipcRenderer.invoke('updater:cached'),
    check: (force?: boolean) => ipcRenderer.invoke('updater:check', force ?? false),
    download: (url: string) => ipcRenderer.invoke('updater:download', url),
    canInstall: () => ipcRenderer.invoke('updater:can-install'),
    install: (url: string) => ipcRenderer.invoke('updater:install', url),
    releaseNotes: (version: string) => ipcRenderer.invoke('updater:release-notes', version),
  },
  // Profiles
  profile: {
    list: () => ipcRenderer.invoke('profile:list'),
    active: () => ipcRenderer.invoke('profile:active'),
    create: (name: string, color?: string) => ipcRenderer.invoke('profile:create', name, color),
    rename: (id: string, name: string) => ipcRenderer.invoke('profile:rename', id, name),
    setColor: (id: string, color: string) => ipcRenderer.invoke('profile:set-color', id, color),
    switch: (id: string) => ipcRenderer.invoke('profile:switch', id),
    delete: (id: string) => ipcRenderer.invoke('profile:delete', id),
  },
  // Tax settings + overview
  tax: {
    getSettings: () => ipcRenderer.invoke('tax:get-settings'),
    saveSettings: (data: any) => ipcRenderer.invoke('tax:save-settings', data),
    getOverview: (taxYear: number) => ipcRenderer.invoke('tax:get-overview', taxYear),
    exportSummaryPDF: (taxYear: number) => ipcRenderer.invoke('tax:export-summary-pdf', taxYear),
    exportInvoicesCSV: (taxYear: number) => ipcRenderer.invoke('tax:export-invoices-csv', taxYear),
    exportExpensesCSV: (taxYear: number) => ipcRenderer.invoke('tax:export-expenses-csv', taxYear),
  },
  // Expenses
  expenses: {
    list: (taxYear?: number) => ipcRenderer.invoke('expense:list', taxYear),
    get: (id: number) => ipcRenderer.invoke('expense:get', id),
    create: (data: any) => ipcRenderer.invoke('expense:create', data),
    update: (id: number, data: any) => ipcRenderer.invoke('expense:update', id, data),
    delete: (id: number) => ipcRenderer.invoke('expense:delete', id),
  },
  // Events from main process
  on: (channel: string, callback: (...args: any[]) => void) => {
    const subscription = (_event: any, ...args: any[]) => callback(...args)
    ipcRenderer.on(channel, subscription)
    return () => ipcRenderer.removeListener(channel, subscription)
  },
}

contextBridge.exposeInMainWorld('api', api)

export type BillableAPI = typeof api
