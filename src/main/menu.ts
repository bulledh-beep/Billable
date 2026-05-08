import { Menu, BrowserWindow, app } from 'electron'

export function createMenu(mainWindow: BrowserWindow) {
  const template: Electron.MenuItemConstructorOptions[] = [
    {
      label: app.name,
      submenu: [
        { role: 'about' },
        { type: 'separator' },
        {
          label: 'Check for Updates…',
          click: () => mainWindow.webContents.send('menu:check-updates'),
        },
        { type: 'separator' },
        {
          label: 'Settings',
          accelerator: 'Cmd+,',
          click: () => mainWindow.webContents.send('navigate', '/settings'),
        },
        { type: 'separator' },
        { role: 'hide' },
        { role: 'hideOthers' },
        { role: 'unhide' },
        { type: 'separator' },
        { role: 'quit' },
      ],
    },
    {
      label: 'File',
      submenu: [
        {
          label: 'New Client',
          accelerator: 'Cmd+N',
          click: () => mainWindow.webContents.send('navigate', '/clients?action=new'),
        },
        {
          label: 'New Project',
          accelerator: 'Cmd+Shift+N',
          click: () => mainWindow.webContents.send('navigate', '/projects?action=new'),
        },
        { type: 'separator' },
        {
          label: 'New Time Entry',
          accelerator: 'Cmd+T',
          click: () => mainWindow.webContents.send('navigate', '/time?action=new'),
        },
        { type: 'separator' },
        { role: 'close' },
      ],
    },
    {
      label: 'Edit',
      submenu: [
        { role: 'undo' },
        { role: 'redo' },
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' },
      ],
    },
    {
      label: 'View',
      submenu: [
        {
          label: 'Dashboard',
          accelerator: 'Cmd+1',
          click: () => mainWindow.webContents.send('navigate', '/'),
        },
        {
          label: 'Clients',
          accelerator: 'Cmd+2',
          click: () => mainWindow.webContents.send('navigate', '/clients'),
        },
        {
          label: 'Projects',
          accelerator: 'Cmd+3',
          click: () => mainWindow.webContents.send('navigate', '/projects'),
        },
        {
          label: 'Time Tracking',
          accelerator: 'Cmd+4',
          click: () => mainWindow.webContents.send('navigate', '/time'),
        },
        {
          label: 'Invoices',
          accelerator: 'Cmd+5',
          click: () => mainWindow.webContents.send('navigate', '/invoices'),
        },
        {
          label: 'Reports',
          accelerator: 'Cmd+6',
          click: () => mainWindow.webContents.send('navigate', '/reports'),
        },
        { type: 'separator' },
        { role: 'reload' },
        { role: 'forceReload' },
        { role: 'toggleDevTools' },
        { type: 'separator' },
        { role: 'togglefullscreen' },
      ],
    },
    {
      label: 'Window',
      submenu: [
        { role: 'minimize' },
        { role: 'zoom' },
        { type: 'separator' },
        { role: 'front' },
      ],
    },
  ]

  const menu = Menu.buildFromTemplate(template)
  Menu.setApplicationMenu(menu)
}
