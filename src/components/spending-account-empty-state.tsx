import { Link } from '@tanstack/react-router'
import { Settings } from 'lucide-react'

import { Card, EmptyState } from './ui'

export function SpendingAccountEmptyState() {
  return (
    <Card>
      <EmptyState
        title="Choose a spending account"
        action={
          <Link className="button-base button-secondary button-default" to="/settings">
            <Settings size={16} aria-hidden="true" />
            <span className="sr-only">Open Settings</span>
          </Link>
        }
      >
        Brief uses one credit account for spending and benefit views.
      </EmptyState>
    </Card>
  )
}
