import { useId } from 'react'

/**
 * The two apps' marks, the same drawings Content HQ uses
 * (src/components/brand in Content HQ): Billable's white alarm clock on an
 * orange tile, and Content HQ's calm face with a clapperboard cap on a black
 * one. They sit side by side in the app switch. Decorative: the switch names
 * each app in words.
 */

const uidOf = (id: string) => id.replace(/:/g, '')

export function BillableMark({ size = 18 }: { size?: number }) {
  const uid = uidOf(useId())
  return (
    <svg viewBox="0 0 512 512" width={size} height={size} aria-hidden="true" focusable="false" style={{ display: 'block' }}>
      <defs>
        <linearGradient id={`${uid}-body`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#FFB13D" />
          <stop offset="1" stopColor="#FF8A00" />
        </linearGradient>
        <linearGradient id={`${uid}-rim`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.45" />
          <stop offset="0.5" stopColor="#ffffff" stopOpacity="0.06" />
          <stop offset="1" stopColor="#ffffff" stopOpacity="0.12" />
        </linearGradient>
        <mask id={`${uid}-apart`} maskUnits="userSpaceOnUse" x="0" y="0" width="512" height="512">
          <rect width="512" height="512" fill="#ffffff" />
          <circle cx="256" cy="290" r="158" fill="#000000" />
        </mask>
      </defs>
      <rect width="512" height="512" rx="120" fill={`url(#${uid}-body)`} />
      <rect x="2.5" y="2.5" width="507" height="507" rx="117.5" fill="none" stroke={`url(#${uid}-rim)`} strokeWidth="5" />
      <g mask={`url(#${uid}-apart)`} fill="#ffffff">
        <rect x="236" y="96" width="40" height="44" rx="12" />
        <g transform="rotate(-32 160 150)"><path d="M106 164a54 54 0 0 1 108 0z" /></g>
        <g transform="rotate(32 352 150)"><path d="M298 164a54 54 0 0 1 108 0z" /></g>
        <path d="M180 392l-30 52M332 392l30 52" fill="none" stroke="#ffffff" strokeWidth="30" strokeLinecap="round" />
      </g>
      <circle cx="256" cy="290" r="138" fill="#ffffff" />
      <g fill="#FFC98A">
        <rect x="248" y="170" width="16" height="30" rx="8" />
        <rect x="248" y="380" width="16" height="30" rx="8" />
        <rect x="136" y="282" width="30" height="16" rx="8" />
        <rect x="346" y="282" width="30" height="16" rx="8" />
      </g>
      <path d="M256 290L202 248" stroke="#F27C00" strokeWidth="28" strokeLinecap="round" />
      <path d="M256 290L338 222" stroke="#F27C00" strokeWidth="24" strokeLinecap="round" />
      <circle cx="256" cy="290" r="20" fill="#D96A00" />
    </svg>
  )
}

export function ContentHQMark({ size = 18 }: { size?: number }) {
  const uid = uidOf(useId())
  return (
    <svg viewBox="0 0 512 512" width={size} height={size} aria-hidden="true" focusable="false" style={{ display: 'block' }}>
      <defs>
        <linearGradient id={`${uid}-body`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#232327" />
          <stop offset="1" stopColor="#0a0a0b" />
        </linearGradient>
        <linearGradient id={`${uid}-rim`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.22" />
          <stop offset="0.5" stopColor="#ffffff" stopOpacity="0.03" />
          <stop offset="1" stopColor="#ffffff" stopOpacity="0.06" />
        </linearGradient>
        <clipPath id={`${uid}-slate`}>
          <rect x="118" y="92" width="276" height="54" rx="14" />
        </clipPath>
      </defs>
      <rect width="512" height="512" rx="120" fill={`url(#${uid}-body)`} />
      <rect x="2.5" y="2.5" width="507" height="507" rx="117.5" fill="none" stroke={`url(#${uid}-rim)`} strokeWidth="5" />
      <g transform="translate(0 22)">
        <g transform="rotate(-8 256 120)">
          <rect x="118" y="92" width="276" height="54" rx="14" fill="#ffffff" />
          <g clipPath={`url(#${uid}-slate)`} fill="#1c1c20">
            <path d="M150 92h34l-30 54h-34z" />
            <path d="M218 92h34l-30 54h-34z" />
            <path d="M286 92h34l-30 54h-34z" />
            <path d="M354 92h34l-30 54h-34z" />
          </g>
        </g>
        <path d="M162 246a32 32 0 0 1 64 0v30q-32-16-64 0z" fill="#ffffff" />
        <path d="M286 246a32 32 0 0 1 64 0v30q-32-16-64 0z" fill="#ffffff" />
        <path d="M216 344q40 26 80 0" fill="none" stroke="#ffffff" strokeWidth="15" strokeLinecap="round" />
      </g>
    </svg>
  )
}

export function ProductMark({ app, size }: { app: 'billable' | 'content'; size?: number }) {
  return app === 'billable' ? <BillableMark size={size} /> : <ContentHQMark size={size} />
}
