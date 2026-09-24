// A small, quick pop of colored bits from one spot, for moments like getting
// paid. It stays near where you clicked instead of filling the screen.

const COLORS = ['#FF9600', '#FFC800', '#58CC02', '#1CB0F6', '#CE82FF', '#FF4B4B']

export function burst(x: number, y: number, count = 26) {
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return
  const dpr = window.devicePixelRatio || 1
  const canvas = document.createElement('canvas')
  canvas.style.cssText = 'position:fixed;inset:0;width:100vw;height:100vh;pointer-events:none;z-index:9999'
  canvas.width = window.innerWidth * dpr
  canvas.height = window.innerHeight * dpr
  document.body.appendChild(canvas)
  const ctx = canvas.getContext('2d')
  if (!ctx) { canvas.remove(); return }
  ctx.scale(dpr, dpr)

  const bits = Array.from({ length: count }, (_, i) => {
    const angle = -Math.PI / 2 + (Math.random() - 0.5) * Math.PI * 1.3
    const speed = 3.5 + Math.random() * 4.5
    return {
      x, y,
      vx: Math.cos(angle) * speed,
      vy: Math.sin(angle) * speed,
      size: 4 + Math.random() * 4,
      rot: Math.random() * Math.PI,
      vr: (Math.random() - 0.5) * 0.4,
      color: COLORS[i % COLORS.length],
      round: i % 3 === 0,
    }
  })

  const start = performance.now()
  const duration = 1100
  const frame = (now: number) => {
    const t = now - start
    ctx.clearRect(0, 0, window.innerWidth, window.innerHeight)
    ctx.globalAlpha = Math.max(0, 1 - Math.max(0, t - duration * 0.55) / (duration * 0.45))
    for (const b of bits) {
      b.vy += 0.22
      b.vx *= 0.98
      b.x += b.vx
      b.y += b.vy
      b.rot += b.vr
      ctx.save()
      ctx.translate(b.x, b.y)
      ctx.rotate(b.rot)
      ctx.fillStyle = b.color
      if (b.round) {
        ctx.beginPath()
        ctx.arc(0, 0, b.size / 2.2, 0, Math.PI * 2)
        ctx.fill()
      } else {
        ctx.fillRect(-b.size / 2, -b.size / 4, b.size, b.size / 2)
      }
      ctx.restore()
    }
    if (t < duration) requestAnimationFrame(frame)
    else canvas.remove()
  }
  requestAnimationFrame(frame)
}
