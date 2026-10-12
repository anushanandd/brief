import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { errorMessage } from '../../lib/errors'
import { formatActivityName } from '../../lib/format'
import { Trash2 } from '../icons'
import { SettingsBlock, SettingsCard, SettingsResetButton, SettingsSection } from '../settings-ui'
import { Button, EmptyState } from '../ui'

export function RulesSection() {
  const finance = useFinance()
  const { updateRules } = useFinanceEdits()
  const rules = finance.data?.rules ?? []
  const removeRule = (id: string) =>
    void updateRules((current) => current.filter((rule) => rule.id !== id)).catch((error) =>
      toast.error('Could not remove the rule', { description: errorMessage(error) }),
    )

  return (
    <SettingsSection
      id="rules"
      title="Rules"
      action={
        <SettingsResetButton
          section="rules"
          onReset={() => updateRules(() => [])}
          detail="Remove all category rules?"
        />
      }
    >
      <SettingsCard>
        <SettingsBlock
          id="category-rules"
          title="Category rules"
          description="Recategorize merchants everywhere. Manual choices win."
        >
          {rules.length ? (
            <ul className="settings-list">
              {rules.map((rule) => (
                <li className="settings-list-row" key={rule.id}>
                  <span className="settings-list-copy">
                    <strong>
                      {formatActivityName(rule.pattern)} → {rule.category}
                    </strong>
                    <small>
                      {rule.match === 'exact' ? 'Merchant is' : 'Merchant or description contains'}{' '}
                      “{rule.pattern}”
                    </small>
                  </span>
                  <Button
                    icon={Trash2}
                    size="compact"
                    variant="ghost"
                    onClick={() => removeRule(rule.id)}
                    aria-label={`Remove rule for ${rule.pattern}`}
                  >
                    Remove
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <EmptyState>No category rules yet.</EmptyState>
          )}
        </SettingsBlock>
      </SettingsCard>
    </SettingsSection>
  )
}
