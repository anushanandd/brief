import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { useRefreshFinance } from '../../hooks/use-refresh-finance'
import { percent, secureWebsite } from '../../lib/account-details'
import { accountDisplayName } from '../../lib/account-names'
import { accountStartDate } from '../../lib/account-start-dates'
import { saveAccountLink } from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { defaultFinanceSettings, financeSettings, type Settings } from '../../lib/finance-settings'
import { formatCurrency } from '../../lib/format'
import { getExternalLogosEnabled } from '../../lib/logos'
import { manualAccountKinds, type FinanceSettings, type FinanceSnapshot } from '../../lib/schema'
import { AccountMark } from '../account-mark'
import { FilterSelect } from '../filter-select'
import { Link2, Plus, Trash2, Unlink } from '../icons'
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
  const { updateSettings } = useFinanceEdits()
  const settings = financeSettings(query.data)
  const accountDisplayNames = settings.accountNames
  const save: SaveSettings = (change) =>
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
          calendarSync: current.calendarSync,
          calendarId: current.calendarId,
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
  return (
    <SettingsSection id="accounts" title="Accounts" action={reset}>
      <SettingsCard>
        <CardPreferences data={data} settings={settings} save={save} />
      </SettingsCard>
      <SettingsCard>
        <ManualAccounts data={data} settings={settings} save={save} />
      </SettingsCard>
      <SettingsCard>
        <AccountDetails data={data} settings={settings} save={save} />
      </SettingsCard>
      <SettingsCard>
        <StatementImports data={data} accountNames={accountDisplayNames} />
      </SettingsCard>
      <SettingsCard>
        <LinkedAccounts data={data} accountNames={accountDisplayNames} />
      </SettingsCard>
    </SettingsSection>
  )
}

type SaveSettings = (change: (current: Settings) => FinanceSettings) => void
type SectionProps = { data: FinanceSnapshot; settings: Settings; save: SaveSettings }

/** The spending card, other cards' benefit products, and which Platinum benefits are tracked. */
function CardPreferences({ data, settings, save }: SectionProps) {
  const accountDisplayNames = settings.accountNames
  const spendingAccountId = settings.spendingAccountId ?? ''
  const hiddenBenefits = settings.hiddenBenefitIds
  const externalLogosEnabled = getExternalLogosEnabled()
  const creditAccounts = data.accounts.filter(({ type }) => type === 'credit')
  const selectedSpendingAccount = creditAccounts.some(({ id }) => id === spendingAccountId)
    ? spendingAccountId
    : ''
  // Cards other than the spending card can be tracked under a card product's benefit rules.
  const otherCards = creditAccounts.filter(({ id }) => id !== selectedSpendingAccount)
  const products = data.insights?.benefitProducts ?? []
  return (
    <>
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
      {otherCards.length ? (
        <SettingsBlock
          id="card-benefits"
          title="Other cards"
          description="A spending card is listed in Accounts but left out of its totals, like the spending account."
        >
          <ul className="settings-list">
            {otherCards.map((account) => {
              const name = accountDisplayName(account.id, account.name, accountDisplayNames)
              const spending = settings.spendingCards?.includes(account.id) ?? false
              return (
                <li className="settings-list-row" key={account.id}>
                  <span className="settings-list-copy">
                    <strong>{name}</strong>
                    <small>{account.institution}</small>
                  </span>
                  <label className="settings-inline-check">
                    <input
                      type="checkbox"
                      checked={spending}
                      aria-label={`${name} is a spending card`}
                      onChange={(event) => {
                        const checked = event.target.checked
                        save((current) => {
                          const others = (current.spendingCards ?? []).filter(
                            (id) => id !== account.id,
                          )
                          return {
                            ...current,
                            spendingCards: checked ? [...others, account.id] : others,
                          }
                        })
                      }}
                    />
                    Spending card
                  </label>
                  {products.length ? (
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
                  ) : null}
                </li>
              )
            })}
          </ul>
        </SettingsBlock>
      ) : null}
      <SettingsBlock
        id="platinum-benefits"
        title="Platinum benefits"
        description="Unchecked benefits are hidden from Benefits and reminders; their credits still count toward the fee."
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
    </>
  )
}

const manualKindLabels: Record<string, string> = {
  cash: 'Cash',
  brokerage: 'Investments',
  retirement: 'Retirement',
  property: 'Property',
  vehicle: 'Vehicle',
  loan: 'Loan',
}

type ManualKind = (typeof manualAccountKinds)[number]
const manualKind = (value: string): ManualKind | undefined =>
  manualAccountKinds.find((kind) => kind === value)

/** A labeled field in the manual account forms. */
const field = (label: string, input: React.ReactNode) => (
  <label>
    {label}
    {input}
  </label>
)

/** A dollar amount typed by hand, as a number, or nothing when it isn't one. */
function parseAmount(value: string) {
  const amount = Number(value.replaceAll(/[,$\s]/g, ''))
  return Number.isFinite(amount) ? Math.round(amount * 100) / 100 : undefined
}

/**
 * Accounts no provider reports, each a name, a kind, and dated balances. The latest balance
 * counts toward net worth; a loan counts against it.
 */
function ManualAccounts({ data, settings, save }: SectionProps) {
  const today = data.updatedAt.slice(0, 10)
  const [adding, setAdding] = useState(false)
  const [draft, setDraft] = useState({
    name: '',
    kind: 'cash' as ManualKind,
    value: '',
    date: today,
  })
  const [updating, setUpdating] = useState<{ id: string; value: string; date: string }>()
  const accounts = settings.manualAccounts
  // A balance is optional: an account left blank is valued by the statements imported into it.
  const draftValue = draft.value.trim() ? parseAmount(draft.value) : null
  const addAccount = () => {
    const name = draft.name.trim()
    if (!name || draftValue === undefined || !draft.date) return
    save((current) => ({
      ...current,
      manualAccounts: [
        ...current.manualAccounts,
        {
          id: `manual:${crypto.randomUUID()}`,
          name,
          kind: draft.kind,
          entries: draftValue === null ? [] : [{ date: draft.date, value: Math.abs(draftValue) }],
        },
      ],
    }))
    setDraft({ name: '', kind: 'cash', value: '', date: today })
    setAdding(false)
  }
  const addEntry = () => {
    if (!updating) return
    const value = parseAmount(updating.value)
    if (value === undefined || !updating.date) return
    const { id, date } = updating
    save((current) => ({
      ...current,
      manualAccounts: current.manualAccounts.map((account) =>
        account.id === id
          ? {
              ...account,
              entries: [
                ...account.entries.filter((entry) => entry.date !== date),
                { date, value: Math.abs(value) },
              ].toSorted((left, right) => left.date.localeCompare(right.date)),
            }
          : account,
      ),
    }))
    setUpdating(undefined)
  }
  const remove = (id: string) =>
    save((current) => ({
      ...current,
      manualAccounts: current.manualAccounts.filter((account) => account.id !== id),
    }))
  const kindSelect = (value: ManualKind, onChange: (kind: ManualKind) => void) => (
    <FilterSelect
      label="Kind"
      value={value}
      options={manualAccountKinds.map((kind) => ({ value: kind, label: manualKindLabels[kind] }))}
      onValueChange={(kind) => {
        const known = manualKind(kind)
        if (known) onChange(known)
      }}
    />
  )
  return (
    <SettingsBlock
      id="manual-accounts"
      title="Manual accounts"
      description="Balances you enter yourself count toward net worth from their first entry."
      action={
        adding ? null : (
          <Button icon={Plus} size="compact" onClick={() => setAdding(true)}>
            Add account
          </Button>
        )
      }
    >
      {accounts.length ? (
        <ul className="settings-list">
          {accounts.map((account) => {
            const latest = account.entries.at(-1)
            return (
              <li className="settings-list-row manual-account-row" key={account.id}>
                <span className="settings-list-copy">
                  <strong>{account.name}</strong>
                  <small>
                    {manualKindLabels[account.kind] ?? account.kind} ·{' '}
                    {latest ? (
                      <>
                        {account.kind === 'loan' ? 'owed ' : ''}
                        <span className="sensitive-value">{formatCurrency(latest.value)}</span> as
                        of {latest.date}
                      </>
                    ) : (
                      'valued by its statements'
                    )}
                  </small>
                </span>
                {updating?.id === account.id ? (
                  <form
                    className="manual-account-form"
                    onSubmit={(event) => {
                      event.preventDefault()
                      addEntry()
                    }}
                  >
                    {field(
                      'Balance',
                      <input
                        inputMode="decimal"
                        autoFocus
                        aria-label={`New balance for ${account.name}`}
                        value={updating.value}
                        onChange={(event) =>
                          setUpdating({ ...updating, value: event.target.value })
                        }
                      />,
                    )}
                    {field(
                      'As of',
                      <input
                        type="date"
                        max={today}
                        aria-label={`Date of the new balance for ${account.name}`}
                        value={updating.date}
                        onChange={(event) => setUpdating({ ...updating, date: event.target.value })}
                      />,
                    )}
                    <span className="settings-button-group">
                      <Button size="compact" variant="ghost" onClick={() => setUpdating(undefined)}>
                        Cancel
                      </Button>
                      <Button size="compact" type="submit">
                        Save balance
                      </Button>
                    </span>
                  </form>
                ) : (
                  <span className="settings-button-group">
                    {account.kind === 'brokerage' || account.kind === 'retirement' ? (
                      <label className="settings-inline-check">
                        <input
                          type="checkbox"
                          checked={Boolean(account.tracksSp500)}
                          aria-label={`Estimate ${account.name} from the S&P 500`}
                          onChange={(event) => {
                            const tracksSp500 = event.target.checked
                            save((current) => ({
                              ...current,
                              manualAccounts: current.manualAccounts.map((manual) =>
                                manual.id === account.id ? { ...manual, tracksSp500 } : manual,
                              ),
                            }))
                          }}
                        />
                        Track the S&amp;P 500
                      </label>
                    ) : null}
                    <Button
                      size="compact"
                      onClick={() => setUpdating({ id: account.id, value: '', date: today })}
                    >
                      Update balance
                    </Button>
                    <Button
                      icon={Trash2}
                      size="compact"
                      variant="ghost"
                      aria-label={`Remove ${account.name}`}
                      onClick={() => remove(account.id)}
                    >
                      Remove
                    </Button>
                  </span>
                )}
              </li>
            )
          })}
        </ul>
      ) : adding ? null : (
        <EmptyState>Nothing entered by hand. A 401(k), a car, or a mortgage goes here.</EmptyState>
      )}
      {adding ? (
        <form
          className="manual-account-form"
          aria-label="New manual account"
          onSubmit={(event) => {
            event.preventDefault()
            addAccount()
          }}
        >
          {field(
            'Name',
            <input
              autoFocus
              maxLength={80}
              aria-label="Account name"
              value={draft.name}
              onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            />,
          )}
          {field(
            'Kind',
            kindSelect(draft.kind, (kind) => setDraft({ ...draft, kind })),
          )}
          {field(
            draft.kind === 'loan' ? 'Amount owed' : 'Balance',
            <input
              inputMode="decimal"
              aria-label={draft.kind === 'loan' ? 'Amount owed' : 'Balance'}
              placeholder={draft.kind === 'loan' ? undefined : 'From statements'}
              value={draft.value}
              onChange={(event) => setDraft({ ...draft, value: event.target.value })}
            />,
          )}
          {field(
            'As of',
            <input
              type="date"
              max={today}
              aria-label="Balance date"
              value={draft.date}
              onChange={(event) => setDraft({ ...draft, date: event.target.value })}
            />,
          )}
          <span className="settings-button-group">
            <Button size="compact" variant="ghost" onClick={() => setAdding(false)}>
              Cancel
            </Button>
            <Button
              size="compact"
              type="submit"
              disabled={!draft.name.trim() || draftValue === undefined}
            >
              Add
            </Button>
          </span>
        </form>
      ) : null}
    </SettingsBlock>
  )
}

/** Each account's display name, the day its history starts, and the day it closed. */
function AccountDetails({ data, settings, save }: SectionProps) {
  const accountDisplayNames = settings.accountNames
  const accountStartDates = settings.accountStartDates
  const closedDates = settings.accountClosedDates
  const accounts = [
    ...data.accounts.filter(({ id }) => id !== 'all'),
    ...(data.closedAccounts ?? []),
  ].toSorted(
    (left, right) =>
      left.institution.localeCompare(right.institution) || left.name.localeCompare(right.name),
  )
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
  const details = settings.accountDetails ?? {}
  const updateDetails = (accountId: string, change: { website?: string; apy?: number }) =>
    save((current) => {
      const all = { ...current.accountDetails }
      const next = { ...all[accountId], ...change }
      if (!next.website) delete next.website
      if (next.apy == null) delete next.apy
      if (Object.keys(next).length) all[accountId] = next
      else delete all[accountId]
      return { ...current, accountDetails: all }
    })
  const updateClosedDate = (accountId: string, date: string) =>
    save((current) => {
      const dates = { ...current.accountClosedDates }
      if (date) dates[accountId] = date
      else delete dates[accountId]
      return { ...current, accountClosedDates: dates }
    })
  return (
    <SettingsBlock
      id="account-details"
      title="Account details"
      description="A closed account leaves today's totals; its history and activity stay. A website opens from the account's page; an APY shows beside what it earns."
    >
      {accounts.length ? (
        <div className="settings-table-scroll">
          <table className="settings-account-table">
            <thead>
              <tr>
                <th scope="col">Account</th>
                <th scope="col">Display name</th>
                <th scope="col">Start date</th>
                <th scope="col">Closed</th>
                <th scope="col">Website</th>
                <th scope="col">APY %</th>
              </tr>
            </thead>
            <tbody>
              {accounts.map((account) => (
                <tr key={account.id} data-closed={closedDates[account.id] ? true : undefined}>
                  <th scope="row">
                    <span className="settings-account-name">
                      <AccountMark
                        type={account.type}
                        institution={account.institution}
                        domain={'domain' in account ? account.domain : undefined}
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
                  <td>
                    <input
                      type="date"
                      max={data.updatedAt.slice(0, 10)}
                      value={closedDates[account.id] ?? ''}
                      aria-label={`Closed date for ${account.name}`}
                      onChange={(event) => updateClosedDate(account.id, event.target.value)}
                    />
                  </td>
                  <td>
                    <CommittedTextInput
                      type="url"
                      maxLength={500}
                      value={details[account.id]?.website ?? ''}
                      placeholder="https://"
                      aria-label={`Website for ${account.name}`}
                      onCommit={(website) =>
                        updateDetails(account.id, { website: secureWebsite(website) })
                      }
                    />
                  </td>
                  <td>
                    <CommittedTextInput
                      type="text"
                      inputMode="decimal"
                      maxLength={8}
                      value={details[account.id]?.apy?.toString() ?? ''}
                      aria-label={`APY for ${account.name}`}
                      onCommit={(apy) => updateDetails(account.id, { apy: percent(apy) })}
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
  )
}

/** Accounts two providers report: suggested duplicates to confirm, and confirmed links. */
function LinkedAccounts({
  data,
  accountNames,
}: {
  data: FinanceSnapshot
  accountNames: FinanceSettings['accountNames']
}) {
  const refresh = useRefreshFinance()
  const links = Object.entries(data.accountLinks ?? {})
  const linkedName = (id: string) => {
    const account = data.accounts.find(
      (candidate) => candidate.id === id || candidate.id.endsWith(`:${id}`),
    )
    return account ? accountDisplayName(account.id, account.name, accountNames) : id
  }
  const duplicates = data.possibleDuplicateAccounts ?? []
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
                onClick={() => changeLink(candidate.plaidAccountId, candidate.snaptradeAccountId)}
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
  )
}
