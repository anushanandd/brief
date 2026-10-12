import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { useRefreshFinance } from '../../hooks/use-refresh-finance'
import { accountDisplayName } from '../../lib/account-names'
import { accountStartDate } from '../../lib/account-start-dates'
import { saveAccountLink } from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { defaultFinanceSettings, financeSettings } from '../../lib/finance-settings'
import { getExternalLogosEnabled } from '../../lib/logos'
import type { FinanceSettings } from '../../lib/schema'
import { AccountMark } from '../account-mark'
import { FilterSelect } from '../filter-select'
import { Link2, Unlink } from '../icons'
import { BenefitMark } from '../platinum-benefits'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
} from '../settings-ui'
import { StatementImports } from '../statement-imports'
import { Button, EmptyState } from '../ui'

/** A text field that saves once, on blur or Return, instead of on every keystroke. */
function CommittedTextInput({
  value,
  onCommit,
  ...props
}: Omit<React.ComponentProps<'input'>, 'value' | 'onChange'> & {
  value: string
  onCommit: (value: string) => void
}) {
  const [draft, setDraft] = useState<string>()
  const commit = () => {
    if (draft !== undefined && draft.trim() !== value) onCommit(draft.trim())
    setDraft(undefined)
  }
  return (
    <input
      {...props}
      value={draft ?? value}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commit()
        if (event.key === 'Escape' && draft !== undefined) {
          event.stopPropagation()
          setDraft(undefined)
        }
      }}
    />
  )
}

export function AccountsSection() {
  const query = useFinance()
  const refresh = useRefreshFinance()
  const { updateSettings } = useFinanceEdits()
  const settings = financeSettings(query.data)
  const accountDisplayNames = settings.accountNames
  const accountStartDates = settings.accountStartDates
  const spendingAccountId = settings.spendingAccountId ?? ''
  const hiddenBenefits = settings.hiddenBenefitIds
  const externalLogosEnabled = getExternalLogosEnabled()
  const save = (change: (current: FinanceSettings) => FinanceSettings) =>
    void updateSettings(change).catch((error) =>
      toast.error('Could not save account settings', { description: errorMessage(error) }),
    )

  const reset = (
    <SettingsResetButton
      section="account"
      onReset={() =>
        updateSettings((current) => ({
          ...defaultFinanceSettings,
          plaidLiabilities: current.plaidLiabilities,
          plaidInvestmentTransactions: current.plaidInvestmentTransactions,
          plaidRecurring: current.plaidRecurring,
          backgroundRefresh: current.backgroundRefresh,
          notifications: current.notifications,
          taxProfile: current.taxProfile,
          accountTaxTreatments: current.accountTaxTreatments,
        }))
      }
      detail="Clear names, start dates, card, and benefits?"
    />
  )

  if (!query.data)
    return (
      <SettingsSection id="accounts" title="Accounts" action={reset}>
        <SettingsCard>
          <SettingsBlock id="account-details" title="Account preferences">
            <EmptyState title={query.isLoading ? 'Loading accounts…' : 'Accounts unavailable'}>
              {query.isLoading ? null : 'Couldn’t load accounts'}
            </EmptyState>
          </SettingsBlock>
        </SettingsCard>
      </SettingsSection>
    )

  const data = query.data
  const creditAccounts = data.accounts.filter(({ type }) => type === 'credit')
  const selectedSpendingAccount = creditAccounts.some(({ id }) => id === spendingAccountId)
    ? spendingAccountId
    : ''
  // Cards other than the spending card can be tracked under a card product's benefit rules.
  const otherCards = creditAccounts.filter(({ id }) => id !== selectedSpendingAccount)
  const products = data.insights?.benefitProducts ?? []
  const accounts = data.accounts
    .filter(({ id }) => id !== 'all')
    .toSorted(
      (left, right) =>
        left.institution.localeCompare(right.institution) || left.name.localeCompare(right.name),
    )
  const links = Object.entries(data.accountLinks ?? {})
  const linkedName = (id: string) => {
    const account = data.accounts.find(
      (candidate) => candidate.id === id || candidate.id.endsWith(`:${id}`),
    )
    return account ? accountDisplayName(account.id, account.name, accountDisplayNames) : id
  }
  const duplicates = data.possibleDuplicateAccounts ?? []
  const updateName = (accountId: string, name: string) =>
    save((current) => {
      const accountNames = { ...current.accountNames }
      if (name) accountNames[accountId] = name
      else delete accountNames[accountId]
      return { ...current, accountNames }
    })
  const updateStartDate = (accountId: string, date: string) =>
    save((current) => {
      const dates = { ...current.accountStartDates }
      if (date) dates[accountId] = date
      else delete dates[accountId]
      return { ...current, accountStartDates: dates }
    })
  const changeLink = (plaidAccountId: string, snaptradeAccountId?: string) =>
    void saveAccountLink(plaidAccountId, snaptradeAccountId)
      .then(() => refresh())
      .then(() =>
        toast.success(
          snaptradeAccountId
            ? 'Accounts linked after provider confirmation'
            : 'Account link removed',
        ),
      )
      .catch((error) => toast.error(errorMessage(error)))

  return (
    <SettingsSection id="accounts" title="Accounts" action={reset}>
      <SettingsCard>
        <SettingsRow
          id="spending-account"
          title="Spending account"
          description="Used for statements and benefits."
        >
          <FilterSelect
            label="Spending account"
            value={selectedSpendingAccount}
            disabled={!creditAccounts.length}
            options={[
              {
                value: '',
                label: creditAccounts.length ? 'Choose an account' : 'No credit accounts',
              },
              ...creditAccounts.map((account) => ({
                value: account.id,
                label: `${accountDisplayName(account.id, account.name, accountDisplayNames)} · ${account.institution}`,
              })),
            ]}
            onValueChange={(accountId) =>
              save((current) => ({ ...current, spendingAccountId: accountId || undefined }))
            }
          />
        </SettingsRow>
        {otherCards.length && products.length ? (
          <SettingsBlock
            id="card-benefits"
            title="Other card benefits"
            description="Track another card’s credits."
          >
            <ul className="settings-list">
              {otherCards.map((account) => {
                const name = accountDisplayName(account.id, account.name, accountDisplayNames)
                return (
                  <li className="settings-list-row" key={account.id}>
                    <span className="settings-list-copy">
                      <strong>{name}</strong>
                      <small>{account.institution}</small>
                    </span>
                    <FilterSelect
                      label={`Card product for ${name}`}
                      value={settings.benefitCards?.[account.id] ?? ''}
                      options={[
                        { value: '', label: 'Not tracked' },
                        ...products.map(({ id, name: label }) => ({ value: id, label })),
                      ]}
                      onValueChange={(product) =>
                        save((current) => {
                          const { [account.id]: _removed, ...rest } = current.benefitCards ?? {}
                          return {
                            ...current,
                            benefitCards: product ? { ...rest, [account.id]: product } : rest,
                          }
                        })
                      }
                    />
                  </li>
                )
              })}
            </ul>
          </SettingsBlock>
        ) : null}
        <SettingsBlock
          id="platinum-benefits"
          title="Platinum benefits"
          description="Checked benefits are tracked."
        >
          <fieldset className="benefit-visibility-settings">
            <legend className="sr-only">Visible Platinum benefits</legend>
            <div className="benefit-visibility-options">
              {(data.insights?.benefitOptions ?? []).map((benefit) => (
                <label key={benefit.id}>
                  {externalLogosEnabled ? (
                    <BenefitMark id={benefit.id} name={benefit.name} externalLogosEnabled />
                  ) : null}
                  <input
                    type="checkbox"
                    checked={!hiddenBenefits.includes(benefit.id)}
                    onChange={(event) => {
                      const visible = event.target.checked
                      save((current) => ({
                        ...current,
                        hiddenBenefitIds: visible
                          ? current.hiddenBenefitIds.filter((id) => id !== benefit.id)
                          : [...current.hiddenBenefitIds, benefit.id],
                      }))
                    }}
                  />
                  <span>{benefit.name}</span>
                </label>
              ))}
            </div>
          </fieldset>
        </SettingsBlock>
      </SettingsCard>

      <SettingsCard>
        <SettingsBlock
          id="account-details"
          title="Names and start dates"
          description="Names and history start dates."
        >
          {accounts.length ? (
            <div className="settings-table-scroll">
              <table className="settings-account-table">
                <thead>
                  <tr>
                    <th scope="col">Account</th>
                    <th scope="col">Display name</th>
                    <th scope="col">Start date</th>
                  </tr>
                </thead>
                <tbody>
                  {accounts.map((account) => (
                    <tr key={account.id}>
                      <th scope="row">
                        <span className="settings-account-name">
                          <AccountMark
                            type={account.type}
                            institution={account.institution}
                            domain={account.domain}
                          />
                          <span>
                            <strong>{account.name}</strong>
                            <small>{account.institution}</small>
                          </span>
                        </span>
                      </th>
                      <td>
                        <CommittedTextInput
                          type="text"
                          maxLength={80}
                          value={accountDisplayNames[account.id] ?? ''}
                          placeholder={account.name}
                          aria-label={`Display name for ${account.name}`}
                          onCommit={(name) => updateName(account.id, name)}
                        />
                      </td>
                      <td>
                        <input
                          type="date"
                          max={data.updatedAt.slice(0, 10)}
                          value={accountStartDate(data, account.id, accountStartDates) ?? ''}
                          aria-label={`Start date for ${account.name}`}
                          onChange={(event) => updateStartDate(account.id, event.target.value)}
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState>No connected accounts to edit.</EmptyState>
          )}
        </SettingsBlock>
      </SettingsCard>

      <SettingsCard>
        <StatementImports data={data} accountNames={accountDisplayNames} />
      </SettingsCard>

      <SettingsCard>
        <SettingsBlock
          id="linked-accounts"
          title="Linked accounts"
          description="Count an account reported by two providers once."
        >
          {duplicates.length || links.length ? (
            <ul className="settings-list">
              {duplicates.map((candidate) => (
                <li className="settings-list-row" key={candidate.plaidAccountId}>
                  <span className="settings-list-copy">
                    <strong>Possible duplicate</strong>
                    <small>{candidate.description}</small>
                  </span>
                  <Button
                    icon={Link2}
                    size="compact"
                    onClick={() =>
                      changeLink(candidate.plaidAccountId, candidate.snaptradeAccountId)
                    }
                  >
                    Confirm same account
                  </Button>
                </li>
              ))}
              {links.map(([plaidAccountId]) => (
                <li className="settings-list-row" key={plaidAccountId}>
                  <span className="settings-list-copy">
                    <strong>{linkedName(plaidAccountId)}</strong>
                    <small>Plaid and SnapTrade · counted once</small>
                  </span>
                  <Button
                    icon={Unlink}
                    size="compact"
                    variant="destructive"
                    onClick={() => changeLink(plaidAccountId)}
                  >
                    Unlink
                  </Button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="settings-copy">No duplicates found.</p>
          )}
        </SettingsBlock>
      </SettingsCard>
    </SettingsSection>
  )
}
