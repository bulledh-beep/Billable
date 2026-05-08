import type { Profile } from '@shared/types'

type Size = 'xs' | 'sm' | 'md' | 'lg'

interface ProfileAvatarProps {
  profile: Profile
  size?: Size
  className?: string
}

const SIZE_CLASSES: Record<Size, { container: string; text: string }> = {
  xs: { container: 'w-4 h-4', text: 'text-[7px]' },
  sm: { container: 'w-6 h-6', text: 'text-[10px]' },
  md: { container: 'w-9 h-9', text: 'text-sm' },
  lg: { container: 'w-14 h-14', text: 'text-lg' },
}

/**
 * Renders a profile's photo if set, otherwise a colored circle showing the
 * first letter of the profile's name. Always a perfect circle, always the
 * same size for the requested variant.
 */
export default function ProfileAvatar({ profile, size = 'sm', className = '' }: ProfileAvatarProps) {
  const { container, text } = SIZE_CLASSES[size]
  const initial = (profile.name || '?').trim().charAt(0).toUpperCase()

  if (profile.avatar) {
    return (
      <img
        src={`data:image/png;base64,${profile.avatar}`}
        alt=""
        draggable={false}
        className={`${container} rounded-full object-cover flex-shrink-0 ring-1 ring-rim/[0.06] ${className}`}
      />
    )
  }

  return (
    <div
      className={`${container} ${text} rounded-full flex items-center justify-center font-semibold flex-shrink-0 select-none ring-1 ring-rim/[0.06] ${className}`}
      style={{
        backgroundColor: profile.color,
        // Use dark text on the gold/cream-ish swatches, white text on darker ones — quick contrast call
        color: '#1a1a1a',
      }}
    >
      {initial}
    </div>
  )
}
