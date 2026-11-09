// Stands in for `electron` when database.ts / profiles.ts / timer-manager.ts
// run under plain Node for the parity harness.
module.exports = {
  app: {
    getPath: () => process.env.BILLABLE_USER_DATA,
    setPath: () => {},
  },
  BrowserWindow: { getAllWindows: () => [] },
  nativeImage: {
    createFromPath: () => { throw new Error('nativeImage is not available in the parity harness') },
  },
}
