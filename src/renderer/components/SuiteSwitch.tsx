import { ProductMark } from './ProductMarks'

export type SuiteApp = 'billable' | 'content'

/**
 * The switch between the two apps in the suite, the same as Content HQ's own:
 * the two apps' icons in a slim track, the app showing raised on a thumb with
 * its name, the other its icon alone, quieter until the pointer comes to it.
 * It sits in the same spot, beside the window buttons, whichever app is
 * showing. Choosing the other app hands the icon's box on, so the move can
 * start from it (App.tsx).
 */
export default function SuiteSwitch({ value, onChange }: { value: SuiteApp; onChange: (app: SuiteApp, from: DOMRect | null) => void }) {
  const options: { value: SuiteApp; label: string }[] = [
    { value: 'billable', label: 'Billable' },
    { value: 'content', label: 'Content HQ' },
  ]
  return (
    <div
      className="no-drag relative inline-flex items-center p-[3px] rounded-full bg-fg/[0.055] shadow-[inset_0_0_0_0.5px_rgb(var(--fg)/0.08)]"
      role="radiogroup"
      aria-label="Switch app"
    >
      {options.map(o => {
        const active = o.value === value
        return (
          <button
            key={o.value}
            role="radio"
            aria-checked={active}
            aria-label={o.label}
            title={active ? undefined : `Switch to ${o.label}`}
            data-suite-option={o.value}
            onClick={e => !active && onChange(o.value, e.currentTarget.querySelector('[data-suite-icon]')?.getBoundingClientRect() ?? null)}
            className={`group relative h-[26px] flex items-center gap-1.5 rounded-full transition-transform duration-150 active:scale-[0.94] ${active ? 'pl-[4px] pr-[10px]' : 'px-[7px]'}`}
          >
            {active && (
              <span
                aria-hidden="true"
                className="absolute inset-0 rounded-full bg-panel dark:bg-panel-2 shadow-[0_1px_2px_rgb(20_20_40/0.14),0_3px_10px_-4px_rgb(20_20_40/0.2),0_0_0_0.5px_rgb(var(--fg)/0.08)] dark:shadow-[inset_0_0.5px_0_rgb(255_255_255/0.1),0_1px_3px_rgb(0_0_0/0.45)]"
              />
            )}
            <span
              data-suite-icon={o.value}
              className={`relative transition-[opacity,filter,transform] duration-150 ${active ? '' : 'opacity-50 saturate-[0.55] dark:opacity-70 group-hover:opacity-100 group-hover:saturate-100 group-hover:scale-[1.08]'}`}
            >
              <ProductMark app={o.value} size={18} />
            </span>
            {active && <span className="relative text-[12px] font-semibold text-fg whitespace-nowrap">{o.label}</span>}
          </button>
        )
      })}
    </div>
  )
}
