import { Search, X } from 'lucide-react'

export default function SearchInput({ value, onChange, placeholder = 'Search', className = '' }: {
  value: string
  onChange: (v: string) => void
  placeholder?: string
  className?: string
}) {
  return (
    <div className={`relative ${className}`}>
      <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-[14px] h-[14px] text-fg-4 pointer-events-none" />
      <input
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder={placeholder}
        className="input pl-8 pr-7"
        onKeyDown={e => { if (e.key === 'Escape' && value) { e.stopPropagation(); onChange('') } }}
      />
      {value && (
        <button
          onClick={() => onChange('')}
          className="absolute right-1.5 top-1/2 -translate-y-1/2 p-1 rounded text-fg-4 hover:text-fg"
          aria-label="Clear search"
        >
          <X className="w-3 h-3" />
        </button>
      )}
    </div>
  )
}
