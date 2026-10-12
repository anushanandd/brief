import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo } from 'react'

import { PageError, HomeLoading } from '../components/data-state'
import { SecurityMark } from '../components/security-mark'
import { SettingsLink } from '../components/settings-dialog'
import { Card, ChartRangeSelect, EmptyState, Metric, SectionHeading } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { financeSettings } from '../lib/finance-settings'
import { formatCurrency, formatSignedCurrency, valueTone } from '../lib/format'
import type { TaxYear } from '../lib/schema'
import { formatActivityDate, formatListDate } from '../lib/spending'
import { estimateRange, taxYear } from '../lib/taxes'

const money = (value: number) => <span className="sensitive-value">{formatCurrency(value)}</span>

/**
 * Taxes on investment and bank income: what the year's 1099s will roughly say and, with a
 * tax profile, an estimate. The tax year is the page's one period. Rust computes every figure.
 */
export function TaxesPage() {
  const query = useFinance()
  const search = useSearch({ from: '/taxes' })
  const navigate = useNavigate({ from: '/taxes' })
  const taxes = query.data?.insights?.taxes
  const years = useMemo(() => taxes?.years.map(({ year }) => year) ?? [], [taxes])
  const yearActions = useMemo<AppAction[]>(
    () =>
      years.map((year) => ({
        id: `taxes.year.${year}`,
        title: `Show ${year} taxes`,
        search: `tax year ${year}`,
        group: 'View',
        scope: 'page' as const,
        run: () => void navigate({ search: { year }, replace: true }),
      })),
    [navigate, years],
  )
  useRegisterActions(yearActions)

  if (query.isLoading) return <HomeLoading />
  if (query.isError || !query.data) return <PageError />
  const data = query.data
  const selected = taxYear(taxes, search.year)
  const names = financeSettings(data).accountNames
  const day = (date: string) => formatListDate(date, data.updatedAt)
  const accountName = (id: string) =>
    accountDisplayName(id, data.accounts.find((account) => account.id === id)?.name ?? id, names)

  return (
    <div className="page taxes-page">
      <WorkspaceHeader
        title="Investments"
        breadcrumbs={[
          { label: 'Investments', to: '/holdings', search: {} },
          { label: 'Taxes', to: '/taxes', search: {} },
        ]}
        actions={
          years.length > 1 && selected ? (
            <ChartRangeSelect
              label="Tax year"
              command="taxes.year."
              options={years.map((year) => ({
                value: year,
                label: String(year),
                accessibleLabel: String(year),
              }))}
              value={selected.year}
              onValueChange={(year) => void navigate({ search: { year }, replace: true })}
            />
          ) : undefined
        }
      />
      {!selected || !taxes ? (
        <Card className="taxes-card">
          <EmptyState title="No taxable activity yet" />
        </Card>
      ) : (
        <>
          <div className="taxes-grid">
            <Card className="taxes-card">
              <SectionHeading title="Wash sales" />
              {selected.washSales.length || taxes.washWindows.length ? (
                <ul className="taxes-list">
                  {taxes.washWindows.map((window) => (
                    <li key={window.saleId}>
                      <SecurityMark ticker={window.ticker} />
                      <span>
                        <strong>
                          Don’t buy {window.ticker} through {day(window.until)}
                        </strong>
                        <small>
                          Loss on {day(window.soldOn)} · {accountName(window.accountId)}
                        </small>
                      </span>
                      <span className="negative">{money(window.loss)}</span>
                    </li>
                  ))}
                  {selected.washSales.map((wash) => (
                    <li key={wash.saleId}>
                      <SecurityMark ticker={wash.ticker} />
                      <span>
                        <strong>{wash.ticker}</strong>
                        <small>
                          Sold {day(wash.date)} · {accountName(wash.accountId)} ·{' '}
                          {wash.permanent
                            ? 'lost for good (bought back in an IRA)'
                            : 'added to new shares’ cost'}
                        </small>
                      </span>
                      <span>{money(wash.disallowed)} disallowed</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <EmptyState>No wash sales in {selected.year}.</EmptyState>
              )}
            </Card>
            <Card className="taxes-card">
              <SectionHeading title="Losses to harvest" />
              {taxes.harvest.length ? (
                <ul className="taxes-list">
                  {taxes.harvest.map((item) => (
                    <li key={`${item.accountId}:${item.ticker}`}>
                      <Link
                        className="taxes-list-link"
                        to="/holdings/$ticker"
                        params={{ ticker: item.ticker }}
                        data-keyboard-row
                        data-keyboard-open
                      >
                        <SecurityMark ticker={item.ticker} />
                        <span>
                          <strong>{item.ticker}</strong>
                          <small>
                            {accountName(item.accountId)}
                            {item.washUntil ? ` · sell after ${day(item.washUntil)}` : ''}
                          </small>
                        </span>
                        <span>
                          {money(item.shortTermLoss + item.longTermLoss)}
                          {item.taxSavings != null ? (
                            <small>saves ~{money(item.taxSavings)}</small>
                          ) : null}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              ) : (
                <EmptyState>Nothing to harvest.</EmptyState>
              )}
            </Card>
          </div>
          <p className="portfolio-note taxes-note">
            Estimates using FIFO lots. Your broker’s forms are the record. Not tax advice.{' '}
            <span title={taxes.sources.join('\n')}>
              Tax tables from {formatActivityDate(taxes.tablesVersion, data.updatedAt)}.
            </span>
          </p>
          {/* The estimate and forms describe the year; what can still change it comes first. */}
          <div className="taxes-grid">
            <EstimateCard year={selected} hasProfile={taxes.hasProfile} />
            <FormsCard year={selected} />
          </div>
        </>
      )}
    </div>
  )
}

function EstimateCard({ year, hasProfile }: { year: TaxYear; hasProfile: boolean }) {
  const estimate = year.estimate
  const range = estimate ? estimateRange(estimate) : undefined
  return (
    <Card className="taxes-card taxes-estimate-card">
      {range ? (
        <header className="home-balance-header">
          <div className="home-balance-main">
            <h2 className="balance-label">Estimated tax · {year.year}</h2>
            <div className="home-balance-value">
              <strong className="hero-number sensitive-value">
                {'single' in range
                  ? formatCurrency(range.single)
                  : `${formatCurrency(range.low)} – ${formatCurrency(range.high)}`}
              </strong>
            </div>
          </div>
        </header>
      ) : (
        <SectionHeading title={`Estimated tax · ${year.year}`} />
      )}
      {estimate ? (
        <>
          <p className="taxes-copy">
            Next dollar: {estimate.marginalRate}% on interest, {estimate.capitalGainsRate}% on
            long-term gains.
            {'low' in range! ? ' A range until dividends are classified.' : ''}
          </p>
          <div className="account-summary-metrics">
            <Metric label="Income tax" value={money(estimate.ordinary)} />
            <Metric label="Capital gains tax" value={money(estimate.capitalGains)} />
            {estimate.niit ? (
              <Metric label="Investment income tax" value={money(estimate.niit)} />
            ) : null}
            {estimate.state ? <Metric label="State tax" value={money(estimate.state)} /> : null}
            {estimate.lossDeduction ? (
              <Metric label="Loss deducted" value={money(estimate.lossDeduction)} />
            ) : null}
            {estimate.lossCarryover ? (
              <Metric label="Carryover" value={money(estimate.lossCarryover)} />
            ) : null}
          </div>
        </>
      ) : !year.tablesAvailable ? (
        <EmptyState>No {year.year} tax rates yet.</EmptyState>
      ) : hasProfile ? (
        <EmptyState>Estimate unavailable.</EmptyState>
      ) : (
        <EmptyState
          title="No estimate yet"
          action={
            <SettingsLink
              target="tax-filing-status"
              className="button-base button-secondary button-compact"
            >
              Add tax profile
            </SettingsLink>
          }
        >
          Needs your filing status and income.
        </EmptyState>
      )}
      {year.partial ? (
        <p className="portfolio-note">Partial year: history starts after Jan 1.</p>
      ) : null}
    </Card>
  )
}

/** What the year's 1099 forms will roughly report, from taxable accounts only. */
function FormsCard({ year }: { year: TaxYear }) {
  const { dividends, realized } = year
  return (
    <Card className="taxes-card">
      <SectionHeading title="1099 preview" />
      <dl className="taxes-forms">
        <div>
          <dt>Interest</dt>
          <dd>{money(year.interest)}</dd>
        </div>
        <div>
          <dt>Dividends</dt>
          <dd>
            {money(dividends.total)}
            {dividends.total ? (
              <small>
                {[
                  dividends.qualified ? ['qualified', dividends.qualified] : null,
                  dividends.nonQualified ? ['ordinary', dividends.nonQualified] : null,
                  dividends.unclassified ? ['unclassified', dividends.unclassified] : null,
                ]
                  .filter((part): part is [string, number] => part !== null)
                  .map(([label, value], index) => (
                    <span key={label}>
                      {index ? ' · ' : ''}
                      {money(value)} {label}
                    </span>
                  ))}
              </small>
            ) : null}
          </dd>
        </div>
        <div>
          <dt>Short-term gains</dt>
          <dd className={valueTone(realized.shortTerm)}>
            <span className="sensitive-value">{formatSignedCurrency(realized.shortTerm)}</span>
          </dd>
        </div>
        <div>
          <dt>Long-term gains</dt>
          <dd className={valueTone(realized.longTerm)}>
            <span className="sensitive-value">{formatSignedCurrency(realized.longTerm)}</span>
          </dd>
        </div>
        {realized.washDisallowed ? (
          <div>
            <dt>Wash sales disallowed</dt>
            <dd>{money(realized.washDisallowed)}</dd>
          </div>
        ) : null}
        {realized.unestimatedSales ? (
          <div>
            <dt>Unestimated sales</dt>
            <dd>
              {realized.unestimatedSales}
              <small>{money(realized.unestimatedProceeds)} proceeds</small>
            </dd>
          </div>
        ) : null}
      </dl>
    </Card>
  )
}
