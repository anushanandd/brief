import { Link } from '@tanstack/react-router'

import { ActivityWorkspaceHeader } from './activity-workspace-header'

export function ActivityDetailHeader({
  title,
  accountName,
}: {
  title: string
  accountName?: string
}) {
  return (
    <>
      <ActivityWorkspaceHeader title={title} />
      {!accountName ? (
        <p className="spending-detail-note">
          Choose a spending account in <Link to="/settings">Settings</Link> to see its history.
        </p>
      ) : null}
    </>
  )
}
