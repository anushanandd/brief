import { useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { WorkspaceHeader, type WorkspaceBreadcrumb } from '../components/workspace-header'
import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useFinance } from '../hooks/use-finance'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { financeSettings } from '../lib/finance-settings'
import { insightsViews, insightsViewSearch, type InsightsView } from '../lib/insights-views'
import { useExternalLogosEnabled } from '../lib/logos'
import { personFromSearch } from '../lib/people'
import type { FinanceSnapshot } from '../lib/schema'
import { tripTitle } from '../lib/trips'
import { PeopleView } from './people'
import { tripFromSearch, TripsView } from './trips'

/**
 * Insights' Trips and People: what Rust worked out from the saved activity that no ledger shows
 * directly. Trips is the default view; Taxes and Documents, Insights' other pages, have routes
 * of their own. Everything here is derived natively; this page only lays it out.
 */
export function InsightsPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  return <Insights data={query.data} />
}

function Insights({ data }: { data: FinanceSnapshot }) {
  const search = useSearch({ from: '/insights' })
  const navigate = useNavigate({ from: '/insights' })
  const view: InsightsView = search.view ?? 'trips'
  const names = financeSettings(data).accountNames
  const valuesHidden = useAmountsHidden()
  const externalLogosEnabled = useExternalLogosEnabled()
  // Activities carry formatted amounts, which screen privacy masks.
  const activities = useMemo(
    () =>
      new Map(buildActivities(data, names, externalLogosEnabled).map((item) => [item.id, item])),
    [data, names, externalLogosEnabled, valuesHidden],
  )
  const label = insightsViews.find(({ id }) => id === view)?.label ?? 'Trips'
  const trip = view === 'trips' ? tripFromSearch(data, search.trip) : undefined
  const breadcrumbs: WorkspaceBreadcrumb[] = [
    { label: 'Insights', to: '/insights', search: {} },
    {
      label,
      to: '/insights',
      search: view === 'trips' && search.year ? { year: search.year } : insightsViewSearch(view),
    },
  ]
  if (trip) breadcrumbs.push({ label: tripTitle(trip), to: '/insights', search: { trip: trip.id } })
  const person = view === 'people' ? personFromSearch(data, search.person) : undefined
  if (person)
    breadcrumbs.push({
      label: person.name,
      to: '/insights',
      search: { view: 'people', person: person.id },
    })

  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        insightsViews.map(({ id, label: title, icon }) => ({
          id: `insights.view.${id}`,
          title: `Show ${title}`,
          search: `insights ${title}`,
          group: 'Here',
          icon,
          scope: 'page' as const,
          disabled: id === view,
          run: () => void navigate({ search: insightsViewSearch(id) }),
        })),
      [navigate, view],
    ),
  )

  return (
    <div className="page insights-page">
      <WorkspaceHeader title={label} breadcrumbs={breadcrumbs} />
      {view === 'trips' ? <TripsView data={data} activities={activities} /> : null}
      {view === 'people' ? <PeopleView data={data} activities={activities} /> : null}
    </div>
  )
}
