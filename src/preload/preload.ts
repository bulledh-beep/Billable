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
  // Events from main process
  on: (channel: string, callback: (...args: any[]) => void) => {
    const subscription = (_event: any, ...args: any[]) => callback(...args)
    ipcRenderer.on(channel, subscription)
    return () => ipcRenderer.removeListener(channel, subscription)
  },
}

contextBridge.exposeInMainWorld('api', api)

export type BillableAPI = typeof api
