import type { BillableAPI } from '../../preload/preload'

declare global {
  interface Window {
    api: BillableAPI
  }
}
