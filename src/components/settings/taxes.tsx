import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { accountDisplayName } from '../../lib/account-names'
import { errorMessage } from '../../lib/errors'
import { financeSettings } from '../../lib/finance-settings'
import { formatProseCurrency, formatShortDay } from '../../lib/format'
import type { FinanceSettings } from '../../lib/schema'
import { parseAmount, taxTreatmentLabels } from '../../lib/taxes'
import { AccountMark } from '../account-mark'
import { FilterSelect } from '../filter-select'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
} from '../settings-ui'
import { EmptyState } from '../ui'

type FilingStatus = NonNullable<FinanceSettings['taxProfile']>['filingStatus']
type TaxTreatment = keyof typeof taxTreatmentLabels

const treatments: TaxTreatment[] = ['taxable', 'taxDeferred', 'taxFree']

const filingStatuses: { value: FilingStatus; label: string }[] = [
  { value: 'single', label: 'Single' },
  { value: 'marriedJoint', label: 'Married filing jointly' },
  { value: 'marriedSeparate', label: 'Married filing separately' },
  { value: 'headOfHousehold', label: 'Head of household' },
]

/** A number field that saves once, on blur or Return; an invalid entry is discarded. */
function CommittedNumberInput({
  value,
  onCommit,
  ...props
}: Omit<React.ComponentProps<'input'>, 'value' | 'onChange'> & {
  value: string
  onCommit: (text: string) => void
}) {
  const [draft, setDraft] = useState<string>()
  const commit = () => {
    if (draft !== undefined && draft.trim() !== value) onCommit(draft.trim())
    setDraft(undefined)
  }
  return (
    <input
      {...props}
      type="text"
      inputMode="decimal"
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

export function TaxesSection() {
  const query = useFinance()
  const { updateSettings } = useFinanceEdits()
  const settings = financeSettings(query.data)
  const profile = settings.taxProfile
  const save = (change: (current: FinanceSettings) => FinanceSettings) =>
    void updateSettings(change).catch((error) =>
      toast.error('Could not save tax settings', { description: errorMessage(error) }),
    )
  const updateProfile = (change: Partial<NonNullable<FinanceSettings['taxProfile']>>) =>
    save((current) =>
      current.taxProfile
        ? { ...current, taxProfile: { ...current.taxProfile, ...change } }
        : current,
    )
  const investmentAccounts = (query.data?.accounts ?? []).filter(
    ({ type }) => type === 'brokerage' || type === 'retirement',
  )
  const retirement = settings.retirement ?? {}
  const updateRetirement = (
    change: (current: NonNullable<FinanceSettings['retirement']>) => FinanceSettings['retirement'],
  ) => save((current) => ({ ...current, retirement: change(current.retirement ?? {}) }))
  const rothAccounts = query.data?.insights?.retirement.rothBasis ?? []

  return (
    <SettingsSection
      id="taxes"
      title="Taxes"
      action={
        <SettingsResetButton
          section="tax"
          onReset={() =>
            updateSettings(
              ({
                taxProfile: _profile,
                accountTaxTreatments: _treatments,
                retirement: _retirement,
                ...rest
              }) => rest,
            )
          }
          detail="Clear tax profile, treatments, and retirement details?"
        />
      }
    >
      <SettingsCard>
        <SettingsRow
          id="tax-filing-status"
          title="Filing status"
          description="Needed for estimates."
        >
          <FilterSelect
            label="Filing status"
            value={profile?.filingStatus ?? ''}
            options={[{ value: '', label: 'Not set' }, ...filingStatuses]}
            onValueChange={(value) =>
              save((current) => {
                const status = filingStatuses.find((option) => option.value === value)?.value
                if (!status) {
                  const { taxProfile: _removed, ...rest } = current
                  return rest
                }
                return {
                  ...current,
                  taxProfile: {
                    taxableIncome: 0,
                    ...current.taxProfile,
                    filingStatus: status,
                  },
                }
              })
            }
          />
        </SettingsRow>
        <SettingsRow
          id="tax-income"
          title="Other taxable income"
          description="After deductions. Start from last year’s Form 1040, line 15."
        >
          <CommittedNumberInput
            className="settings-number-input"
            disabled={!profile}
            value={profile ? formatProseCurrency(profile.taxableIncome) : ''}
            placeholder="$0"
            aria-label="Other taxable income"
            onCommit={(text) => {
              const amount = parseAmount(text)
              if (amount === undefined) toast.error('Enter an amount in dollars')
              else updateProfile({ taxableIncome: amount })
            }}
          />
        </SettingsRow>
        <SettingsRow
          id="tax-state-rate"
          title="State income tax"
          description="Flat rate. Optional."
        >
          <CommittedNumberInput
            className="settings-number-input"
            disabled={!profile}
            value={profile?.stateRate != null ? `${profile.stateRate}%` : ''}
            placeholder="None"
            aria-label="State income tax rate"
            onCommit={(text) => {
              const rate = parseAmount(text)
              if (text && (rate === undefined || rate > 20)) toast.error('Enter a rate up to 20%')
              else updateProfile({ stateRate: rate })
            }}
          />
        </SettingsRow>
      </SettingsCard>
      <SettingsCard>
        <SettingsBlock
          id="account-tax-treatment"
          title="Account tax treatment"
          description="Automatic uses the provider’s account type."
        >
          {investmentAccounts.length ? (
            <ul className="settings-list">
              {investmentAccounts.map((account) => {
                const name = accountDisplayName(account.id, account.name, settings.accountNames)
                const chosen = settings.accountTaxTreatments?.[account.id]
                const derived: TaxTreatment =
                  account.taxTreatment ??
                  (account.type === 'retirement' ? 'taxDeferred' : 'taxable')
                return (
                  <li className="settings-list-row" key={account.id}>
                    <span className="settings-list-copy settings-list-copy-inline">
                      <AccountMark
                        type={account.type}
                        institution={account.institution}
                        domain={account.domain}
                      />
                      <span className="settings-list-copy">
                        <strong>{name}</strong>
                        <small>
                          {account.institution}
                          {account.subtype ? ` · ${account.subtype}` : ''}
                        </small>
                      </span>
                    </span>
                    <FilterSelect
                      label={`Tax treatment for ${name}`}
                      value={chosen ?? ''}
                      options={[
                        {
                          value: '',
                          label: chosen
                            ? 'Automatic'
                            : `Automatic · ${taxTreatmentLabels[derived]}`,
                        },
                        ...treatments.map((value) => ({ value, label: taxTreatmentLabels[value] })),
                      ]}
                      onValueChange={(value) =>
                        save((current) => {
                          const treatment = treatments.find((option) => option === value)
                          const { [account.id]: _removed, ...rest } =
                            current.accountTaxTreatments ?? {}
                          return {
                            ...current,
                            accountTaxTreatments: treatment
                              ? { ...rest, [account.id]: treatment }
                              : rest,
                          }
                        })
                      }
                    />
                  </li>
                )
              })}
            </ul>
          ) : (
            <EmptyState>No investment accounts.</EmptyState>
          )}
        </SettingsBlock>
      </SettingsCard>
      <SettingsCard>
        <SettingsRow
          id="retirement-birth-year"
          title="Birth year"
          description="Raises contribution limits from 50."
        >
          <CommittedNumberInput
            className="settings-number-input"
            value={retirement.birthYear?.toString() ?? ''}
            placeholder="Not set"
            aria-label="Birth year"
            onCommit={(text) => {
              const year = Number(text)
              if (text && !(Number.isInteger(year) && year >= 1900 && year <= 2200))
                toast.error('Enter a year, such as 2004')
              else
                updateRetirement((current) => ({ ...current, birthYear: text ? year : undefined }))
            }}
          />
        </SettingsRow>
        <SettingsRow
          id="retirement-planned-costs"
          title="Costs you’ll add"
          description="Monthly, on top of today’s spending: rent once you move out, health coverage."
        >
          <CommittedNumberInput
            className="settings-number-input"
            value={
              retirement.plannedMonthly != null
                ? formatProseCurrency(retirement.plannedMonthly)
                : ''
            }
            placeholder="$0"
            aria-label="Monthly costs you’ll add"
            onCommit={(text) => {
              const amount = parseAmount(text)
              if (text && amount === undefined) toast.error('Enter an amount in dollars')
              else updateRetirement((current) => ({ ...current, plannedMonthly: amount }))
            }}
          />
        </SettingsRow>
        <SettingsBlock
          id="retirement-earlier-contributions"
          title="Earlier Roth contributions"
          description="Contributions made before Brief’s history of the account. Your custodian’s Form 5498s list them by year."
        >
          {rothAccounts.length ? (
            <ul className="settings-list">
              {rothAccounts.map(({ accountId, since, earlier }) => {
                const account = query.data?.accounts.find(({ id }) => id === accountId)
                const name = accountDisplayName(
                  accountId,
                  account?.name ?? 'Roth IRA',
                  settings.accountNames,
                )
                return (
                  <li className="settings-list-row" key={accountId}>
                    <span className="settings-list-copy">
                      <strong>{name}</strong>
                      <small>{since ? `Before ${formatShortDay(since)}` : 'No activity yet'}</small>
                    </span>
                    <CommittedNumberInput
                      className="settings-number-input"
                      value={earlier != null ? formatProseCurrency(earlier) : ''}
                      placeholder="Not set"
                      aria-label={`Contributions to ${name} before its history`}
                      onCommit={(text) => {
                        const amount = parseAmount(text)
                        if (text && amount === undefined) toast.error('Enter an amount in dollars')
                        else
                          updateRetirement(({ earlierContributions = {}, ...current }) => {
                            const { [accountId]: _removed, ...rest } = earlierContributions
                            return {
                              ...current,
                              earlierContributions:
                                amount === undefined ? rest : { ...rest, [accountId]: amount },
                            }
                          })
                      }}
                    />
                  </li>
                )
              })}
            </ul>
          ) : (
            <EmptyState>No Roth IRA.</EmptyState>
          )}
        </SettingsBlock>
      </SettingsCard>
    </SettingsSection>
  )
}
