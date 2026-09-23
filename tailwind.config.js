/** @type {import('tailwindcss').Config} */

// Every color reads a CSS variable holding an "R G B" triplet, so the theme can
// swap by class on <html> and Tailwind opacity modifiers (bg-accent/10) still work.
const v = name => `rgb(var(--${name}) / <alpha-value>)`

export default {
  content: [
    './index.html',
    './src/renderer/**/*.{js,ts,jsx,tsx}',
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // ---- Surfaces ----
        bg: v('bg'),                 // content canvas
        sidebar: v('sidebar'),       // fallback when the sidebar material is off
        panel: { DEFAULT: v('panel'), 2: v('panel-2') }, // grouped boxes, raised rows
        field: v('field'),           // inputs
        line: { DEFAULT: v('line'), strong: v('line-strong') },

        // ---- Text ----
        fg: { DEFAULT: v('fg'), 2: v('fg-2'), 3: v('fg-3'), 4: v('fg-4') },

        // ---- Brand ----
        accent: {
          DEFAULT: v('accent'),
          hover: v('accent-hover'),
          text: v('accent-text'),
          fg: v('accent-fg'),
        },

        // ---- Meaning (text color; use /10 or /12 for tinted backgrounds) ----
        green: v('green'),
        blue: v('blue'),
        red: v('red'),
        amber: v('amber'),
        violet: v('violet'),
        gray: v('gray'),

        // ---- Older names, mapped onto the new palette ----
        surface: {
          DEFAULT: v('bg'),
          50: v('panel'),
          100: v('panel'),
          200: v('panel-2'),
          300: v('line'),
          400: v('line-strong'),
        },
        text: { primary: v('fg'), secondary: v('fg-2'), tertiary: v('fg-3') },
        rim: v('rim'),
        status: {
          active: v('green'),
          paused: v('amber'),
          complete: v('blue'),
          archived: v('gray'),
          paid: v('green'),
          sent: v('blue'),
          overdue: v('red'),
          draft: v('gray'),
        },
      },
      fontFamily: {
        // San Francisco everywhere. -apple-system picks the Text or Display cut by size.
        sans: ['-apple-system', 'BlinkMacSystemFont', '"SF Pro Text"', '"Helvetica Neue"', 'Arial', 'sans-serif'],
        display: ['-apple-system', 'BlinkMacSystemFont', '"SF Pro Display"', '"Helvetica Neue"', 'Arial', 'sans-serif'],
        mono: ['ui-monospace', '"SF Mono"', 'Menlo', 'monospace'],
        // Meter-style numerals for the figures that matter (money, running time)
        figures: ['"Billable Figures"', '-apple-system', 'BlinkMacSystemFont', 'sans-serif'],
        // Condensed caps for stamps and the wordmark
        stamp: ['"Billable Stamp"', '-apple-system', 'BlinkMacSystemFont', 'sans-serif'],
      },
      fontSize: {
        '2xs': ['11px', { lineHeight: '14px' }],
        xs: ['12px', { lineHeight: '16px' }],
        sm: ['13px', { lineHeight: '18px' }],
        base: ['14px', { lineHeight: '20px' }],
        lg: ['16px', { lineHeight: '22px' }],
        xl: ['18px', { lineHeight: '24px' }],
        '2xl': ['22px', { lineHeight: '28px' }],
        '3xl': ['28px', { lineHeight: '34px' }],
      },
      // Hairlines: one device pixel on Retina, one pixel elsewhere
      borderWidth: {
        DEFAULT: '0.5px',
        hair: '0.5px',
      },
      borderRadius: {
        card: '10px',
        tile: '6px',
      },
      opacity: {
        12: '0.12',
        35: '0.35',
        45: '0.45',
        55: '0.55',
        65: '0.65',
        85: '0.85',
      },
      boxShadow: {
        card: 'var(--shadow-card)',
        pop: 'var(--shadow-pop)',
        focus: '0 0 0 3px rgb(var(--accent) / 0.35)',
      },
      keyframes: {
        'fade-in': { from: { opacity: '0' }, to: { opacity: '1' } },
      },
      animation: {
        'fade-in': 'fade-in 120ms ease-out',
      },
    },
  },
  plugins: [],
}
