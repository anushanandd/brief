import type { AnchorHTMLAttributes } from 'react'
import { toast } from 'sonner'

import { isTauri, openExternalUrl } from '../lib/api'

export function ExternalLink(
  props: Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'onClick'> & { href: string },
) {
  return (
    <a
      {...props}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(event) => {
        if (!isTauri()) return
        event.preventDefault()
        void openExternalUrl(props.href).catch(() => toast.error('Could not open this website'))
      }}
    />
  )
}
