// Bundles the Electron main-process data code into dist/billable.cjs for Node.
import { build } from 'esbuild'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const here = path.dirname(fileURLToPath(import.meta.url))
await build({
  entryPoints: [path.join(here, 'entry.ts')],
  outfile: path.join(here, 'dist', 'billable.cjs'),
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['better-sqlite3'],
  alias: { electron: path.join(here, 'electron-stub.js') },
  logLevel: 'warning',
})
console.log('built dist/billable.cjs')
