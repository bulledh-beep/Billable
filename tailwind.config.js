/** @type {import('tailwindcss').Config} */
export default {
  content: [
    './index.html',
    './src/renderer/**/*.{js,ts,jsx,tsx}',
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        surface: {
          DEFAULT: '#0F0F11',
          50: '#1A1A1E',
          100: '#222226',
          200: '#2A2A2F',
          300: '#333338',
          400: '#3D3D42',
        },
        accent: {
          DEFAULT: '#F5A623',
          light: '#F7B94D',
          dark: '#D4901E',
          glow: 'rgba(245, 166, 35, 0.15)',
        },
        text: {
          primary: '#F0EDE8',
          secondary: '#9B9A97',
          tertiary: '#6B6A67',
        },
        status: {
          active: '#34D399',
          paused: '#FBBF24',
          complete: '#60A5FA',
          archived: '#9B9A97',
          paid: '#34D399',
          sent: '#60A5FA',
          overdue: '#F87171',
          draft: '#9B9A97',
        },
      },
      fontFamily: {
        sans: ['Outfit', 'Geist', '-apple-system', 'BlinkMacSystemFont', 'sans-serif'],
        mono: ['DM Mono', 'SF Mono', 'monospace'],
      },
      boxShadow: {
        'inner-soft': 'inset 0 1px 2px rgba(0,0,0,0.3), inset 0 -1px 1px rgba(255,255,255,0.03)',
        'card': '0 2px 8px rgba(0,0,0,0.3), 0 1px 3px rgba(0,0,0,0.2)',
        'card-hover': '0 4px 16px rgba(0,0,0,0.4), 0 2px 6px rgba(0,0,0,0.3)',
        'glow': '0 0 20px rgba(245, 166, 35, 0.15)',
      },
      animation: {
        'pulse-slow': 'pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        'count-up': 'countUp 0.5s ease-out',
      },
    },
  },
  plugins: [],
}
