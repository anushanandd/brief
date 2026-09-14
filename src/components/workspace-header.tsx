import { Link } from '@tanstack/react-router'

import { useFinance } from '../hooks/use-finance'
import { formatUpdatedAt } from '../lib/format'
import { RefreshButton } from './data-state'
import { StatusDot } from './ui'

type WorkspaceParent = {
  label: string
  to: '/accounts' | '/spending'
}

export function WorkspaceHeader({ title, parent }: { title: string; parent?: WorkspaceParent }) {
  const updatedAt = useFinance().data?.updatedAt

  return (
    <header className="page-header workspace-header">
      <h1 className={parent ? 'page-route' : undefined}>
        {parent ? (
          <>
            <Link to={parent.to}>{parent.label}</Link>
            <span className="page-route-separator">/</span>
            <span aria-current="page">{title}</span>
          </>
        ) : (
          title
        )}
      </h1>
      <div className="dashboard-actions">
        {updatedAt ? (
          <span className="freshness">
            <StatusDot /> Updated {formatUpdatedAt(updatedAt)}
          </span>
        ) : null}
        <RefreshButton />
      </div>
    </header>
  )
}
