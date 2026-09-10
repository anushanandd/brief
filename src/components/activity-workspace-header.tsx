import { Link } from '@tanstack/react-router'

import { useFinance } from '../hooks/use-finance'
import { formatUpdatedAt } from '../lib/format'
import { RefreshButton } from './data-state'
import { StatusDot } from './ui'

export function ActivityWorkspaceHeader({ title = 'Activity' }: { title?: string }) {
  const updatedAt = useFinance().data?.updatedAt

  return (
    <header className="page-header workspace-header">
      <h1 className={title === 'Activity' ? undefined : 'page-route'}>
        {title === 'Activity' ? (
          title
        ) : (
          <>
            <Link to="/activities">Activity</Link>
            <span className="page-route-separator">/</span>
            <span aria-current="page">{title}</span>
          </>
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
