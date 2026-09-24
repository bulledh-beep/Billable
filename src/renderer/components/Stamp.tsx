// A rubber stamp for the invoice page: PAID, OVERDUE or DRAFT, pressed a
// little crooked, with worn ink.

const INK: Record<StampTone, string> = {
  green: '#1E7F3C',
  red: '#C4302B',
  gray: '#6E6E73',
}

export type StampTone = 'green' | 'red' | 'gray'

// Speckled mask so the ink looks pressed rather than printed
const WEAR = `url("data:image/svg+xml,${encodeURIComponent(
  "<svg xmlns='http://www.w3.org/2000/svg' width='240' height='120'><filter id='w'><feTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2' seed='7'/><feColorMatrix values='0 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 -2.6 2.35'/></filter><rect width='100%' height='100%' filter='url(%23w)'/></svg>",
)}")`

export default function Stamp({ label, detail, tone, className = '' }: {
  label: string
  detail?: string
  tone: StampTone
  className?: string
}) {
  const ink = INK[tone]
  return (
    <div
      aria-label={detail ? `${label}, ${detail}` : label}
      className={`pointer-events-none select-none inline-flex flex-col items-center px-3.5 pt-[7px] pb-[5px] rounded-[7px] font-stamp uppercase ${className}`}
      style={{
        color: ink,
        border: `2.5px solid ${ink}`,
        boxShadow: `inset 0 0 0 2px #fff, inset 0 0 0 3px ${ink}`,
        transform: 'rotate(-8deg)',
        opacity: 0.9,
        // Pressed down onto the paper a beat after the page appears
        animation: 'stamp-thunk 560ms cubic-bezier(0.3, 1.1, 0.5, 1) 280ms both',
        mixBlendMode: 'multiply',
        WebkitMaskImage: WEAR,
        maskImage: WEAR,
        WebkitMaskSize: '240px 120px',
        maskSize: '240px 120px',
      }}
    >
      <span className="text-[34px] leading-[30px] tracking-[0.1em] pl-[0.1em]">{label}</span>
      {detail && <span className="text-[11px] leading-3 tracking-[0.16em] pl-[0.16em] mt-[3px]">{detail}</span>}
    </div>
  )
}
