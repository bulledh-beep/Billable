import { useEffect, useRef, useState } from 'react'
import { motion, useSpring, useTransform } from 'framer-motion'

interface AnimatedNumberProps {
  value: number
  format?: (n: number) => string
  className?: string
}

export default function AnimatedNumber({ value, format, className }: AnimatedNumberProps) {
  const spring = useSpring(0, { stiffness: 50, damping: 20 })
  const [display, setDisplay] = useState('0')

  useEffect(() => {
    spring.set(value)
    const unsub = spring.on('change', (v) => {
      setDisplay(format ? format(v) : v.toFixed(1))
    })
    return unsub
  }, [value, format, spring])

  return <span className={className}>{display}</span>
}
