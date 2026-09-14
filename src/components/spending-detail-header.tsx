import { Link } from '@tanstack/react-router'

import { WorkspaceHeader } from './workspace-header'

export function ActivityDetailHeader({
  title,
  accountName,
}: {
  title: string
  accountName?: string
}) {
  return (
    <>
      <WorkspaceHeader title={title} parent={{ label: 'Spending', to: '/spending' }} />
      {!accountName ? (
        <p className="spending-detail-note">
          Choose a spending account in <Link to="/settings">Settings</Link> to see its history.
        </p>
      ) : null}
    </>
  )
}
