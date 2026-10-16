import { Settings } from './icons'
import { SettingsLink } from './settings-dialog'
import { Card, EmptyState } from './ui'

export function SpendingAccountEmptyState() {
  return (
    <Card className="spending-account-empty">
      <EmptyState
        title="No spending card"
        action={
          <SettingsLink
            className="button-base button-secondary button-compact"
            target="spending-account"
          >
            <Settings size={16} aria-hidden="true" />
            Choose card
          </SettingsLink>
        }
      >
        Statements and benefits need one.
      </EmptyState>
    </Card>
  )
}
