import { useQuery } from '@tanstack/react-query'
import { useState, type MouseEvent, type ReactNode } from 'react'
import { sessionQuery } from '../api/session'
import { legacyUrl } from '../lib/format'
import { legacyBridgeUrl } from '../lib/legacy'

/**
 * Link to a page of the classic interface. A plain click goes through the
 * connection bridge, so the user does not log in again; opening in a new
 * tab keeps the plain URL.
 */
export function LegacyLink({
  path,
  className,
  children,
}: {
  path: string
  className?: string
  children: ReactNode
}) {
  const { data: session } = useQuery(sessionQuery)
  const [pending, setPending] = useState(false)

  const onClick = async (event: MouseEvent<HTMLAnchorElement>) => {
    const modified = event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.button !== 0
    if (!session?.capabilities.bridge || modified) {
      return
    }
    event.preventDefault()
    setPending(true)
    window.location.assign(await legacyBridgeUrl(path))
  }

  return (
    <a href={legacyUrl(path)} onClick={(event) => void onClick(event)} aria-busy={pending} className={className}>
      {children}
    </a>
  )
}
