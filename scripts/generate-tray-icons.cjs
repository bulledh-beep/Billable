// Generate macOS tray template icons as PNG files
// Template images must be black-on-transparent (macOS auto-themes them)

const fs = require('fs')
const path = require('path')

// Simple PNG encoder for RGBA pixel data
function createPNG(width, height, pixels) {
  // PNG signature
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])

  // IHDR chunk
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8  // bit depth
  ihdr[9] = 6  // color type: RGBA
  ihdr[10] = 0 // compression
  ihdr[11] = 0 // filter
  ihdr[12] = 0 // interlace
  const ihdrChunk = makeChunk('IHDR', ihdr)

  // IDAT chunk - raw pixel data with filter bytes
  const rawData = Buffer.alloc(height * (1 + width * 4))
  for (let y = 0; y < height; y++) {
    rawData[y * (1 + width * 4)] = 0 // filter: none
    for (let x = 0; x < width; x++) {
      const srcIdx = (y * width + x) * 4
      const dstIdx = y * (1 + width * 4) + 1 + x * 4
      rawData[dstIdx] = pixels[srcIdx]     // R
      rawData[dstIdx + 1] = pixels[srcIdx + 1] // G
      rawData[dstIdx + 2] = pixels[srcIdx + 2] // B
      rawData[dstIdx + 3] = pixels[srcIdx + 3] // A
    }
  }

  const zlib = require('zlib')
  const compressed = zlib.deflateSync(rawData)
  const idatChunk = makeChunk('IDAT', compressed)

  // IEND chunk
  const iendChunk = makeChunk('IEND', Buffer.alloc(0))

  return Buffer.concat([signature, ihdrChunk, idatChunk, iendChunk])
}

function makeChunk(type, data) {
  const length = Buffer.alloc(4)
  length.writeUInt32BE(data.length, 0)
  const typeBuffer = Buffer.from(type, 'ascii')
  const crcData = Buffer.concat([typeBuffer, data])

  // CRC32
  let crc = crc32(crcData)
  const crcBuffer = Buffer.alloc(4)
  crcBuffer.writeUInt32BE(crc >>> 0, 0)

  return Buffer.concat([length, typeBuffer, data, crcBuffer])
}

function crc32(buf) {
  let table = []
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) {
      if (c & 1) c = 0xEDB88320 ^ (c >>> 1)
      else c = c >>> 1
    }
    table[n] = c
  }
  let crc = 0xFFFFFFFF
  for (let i = 0; i < buf.length; i++) {
    crc = table[(crc ^ buf[i]) & 0xFF] ^ (crc >>> 8)
  }
  return crc ^ 0xFFFFFFFF
}

// Draw a circle using midpoint circle algorithm with anti-aliasing approximation
function drawCircle(pixels, width, cx, cy, radius, thickness, r, g, b) {
  for (let y = 0; y < width; y++) {
    for (let x = 0; x < width; x++) {
      const dist = Math.sqrt((x - cx) ** 2 + (y - cy) ** 2)
      const halfThick = thickness / 2
      const distFromRing = Math.abs(dist - radius)
      if (distFromRing < halfThick + 0.8) {
        const alpha = Math.max(0, Math.min(1, (halfThick + 0.8 - distFromRing) / 0.8))
        const idx = (y * width + x) * 4
        const existing = pixels[idx + 3] / 255
        const blended = Math.min(1, existing + alpha * (1 - existing))
        pixels[idx] = r
        pixels[idx + 1] = g
        pixels[idx + 2] = b
        pixels[idx + 3] = Math.round(blended * 255)
      }
    }
  }
}

// Draw a line with anti-aliasing
function drawLine(pixels, width, x1, y1, x2, y2, thickness, r, g, b) {
  const len = Math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2)
  const steps = Math.ceil(len * 4)
  for (let i = 0; i <= steps; i++) {
    const t = i / steps
    const px = x1 + (x2 - x1) * t
    const py = y1 + (y2 - y1) * t

    // Draw anti-aliased dot at this position
    const rad = thickness / 2
    const minX = Math.max(0, Math.floor(px - rad - 1))
    const maxX = Math.min(width - 1, Math.ceil(px + rad + 1))
    const minY = Math.max(0, Math.floor(py - rad - 1))
    const maxY = Math.min(width - 1, Math.ceil(py + rad + 1))

    for (let y = minY; y <= maxY; y++) {
      for (let x = minX; x <= maxX; x++) {
        const dist = Math.sqrt((x - px) ** 2 + (y - py) ** 2)
        if (dist < rad + 0.6) {
          const alpha = Math.max(0, Math.min(1, (rad + 0.6 - dist) / 0.6))
          const idx = (y * width + x) * 4
          const existing = pixels[idx + 3] / 255
          const blended = Math.min(1, existing + alpha * (1 - existing))
          pixels[idx] = r
          pixels[idx + 1] = g
          pixels[idx + 2] = b
          pixels[idx + 3] = Math.round(blended * 255)
        }
      }
    }
  }
}

// Draw a filled circle
function drawFilledCircle(pixels, width, cx, cy, radius, r, g, b) {
  for (let y = 0; y < width; y++) {
    for (let x = 0; x < width; x++) {
      const dist = Math.sqrt((x - cx) ** 2 + (y - cy) ** 2)
      if (dist < radius + 0.6) {
        const alpha = Math.max(0, Math.min(1, (radius + 0.6 - dist) / 0.6))
        const idx = (y * width + x) * 4
        const existing = pixels[idx + 3] / 255
        const blended = Math.min(1, existing + alpha * (1 - existing))
        pixels[idx] = r
        pixels[idx + 1] = g
        pixels[idx + 2] = b
        pixels[idx + 3] = Math.round(blended * 255)
      }
    }
  }
}

function generateIcon(size, active) {
  const pixels = Buffer.alloc(size * size * 4, 0) // transparent

  const cx = size / 2
  const cy = size / 2
  const scale = size / 16

  // Clock circle
  const radius = 6.2 * scale
  const thickness = 1.4 * scale
  drawCircle(pixels, size, cx, cy, radius, thickness, 0, 0, 0)

  // Hour hand (pointing to ~10 o'clock position)
  const hourAngle = -Math.PI / 3 // 10 o'clock
  const hourLen = 3.2 * scale
  const hx = cx + Math.sin(hourAngle) * hourLen
  const hy = cy - Math.cos(hourAngle) * hourLen
  drawLine(pixels, size, cx, cy, hx, hy, 1.5 * scale, 0, 0, 0)

  // Minute hand (pointing to ~2 o'clock position)
  const minAngle = Math.PI / 3 // 2 o'clock
  const minLen = 4.5 * scale
  const mx = cx + Math.sin(minAngle) * minLen
  const my = cy - Math.cos(minAngle) * minLen
  drawLine(pixels, size, cx, cy, mx, my, 1.1 * scale, 0, 0, 0)

  // Active indicator: filled dot at center
  if (active) {
    drawFilledCircle(pixels, size, cx, cy, 1.8 * scale, 0, 0, 0)
  }

  return createPNG(size, size, pixels)
}

// Generate all 4 icons
const resourceDir = path.join(__dirname, '..', 'resources')

const icons = [
  { name: 'trayIconTemplate.png', size: 16, active: false },
  { name: 'trayIconTemplate@2x.png', size: 32, active: false },
  { name: 'trayIconActiveTemplate.png', size: 16, active: true },
  { name: 'trayIconActiveTemplate@2x.png', size: 32, active: true },
]

for (const icon of icons) {
  const png = generateIcon(icon.size, icon.active)
  const outPath = path.join(resourceDir, icon.name)
  fs.writeFileSync(outPath, png)
  console.log(`Generated ${icon.name} (${icon.size}x${icon.size})`)
}

console.log('Done! All tray icons generated.')
