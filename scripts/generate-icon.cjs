const { app, BrowserWindow } = require('electron')
const path = require('path')
const fs = require('fs')

app.disableHardwareAcceleration()

app.whenReady().then(async () => {
  const win = new BrowserWindow({
    width: 1024,
    height: 1024,
    show: false,
    frame: false,
    transparent: false,
    webPreferences: {
      offscreen: true,
    },
  })

  const htmlPath = path.join(__dirname, 'icon.html')
  await win.loadFile(htmlPath)

  // Wait for render
  await new Promise(r => setTimeout(r, 1000))

  const image = await win.capturePage()
  const png = image.toPNG()

  const outDir = path.join(__dirname, '..', 'resources')
  if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true })

  const outPath = path.join(outDir, 'icon-1024.png')
  fs.writeFileSync(outPath, png)
  console.log('Icon saved to', outPath)

  app.quit()
})
