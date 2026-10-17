import { useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { InsightsStrip } from '../components/insights-strip'
import { WorkspaceHeader, type WorkspaceBreadcrumb } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { financeSettings } from '../lib/finance-settings'
import { insightsViews, insightsViewSearch, type InsightsView } from '../lib/insights-views'
import { getExternalLogosEnabled } from '../lib/logos'
import { personFromSearch } from '../lib/people'
import type { FinanceSnapshot } from '../lib/schema'
import { tripTitle } from '../lib/trips'
import { ChaptersView } from './chapters'
import { PeopleView } from './people'
import { RulesView } from './rules'
import { tripFromSearch, TripsView } from './trips'

/**
 * Insights: what Rust worked out from the saved activity that no ledger shows directly. One
 * strip of cards heads it, as Money's does; each card is a view: Trips, the default, then
 * People, Chapters, and Rules. Everything here is derived natively; this page only lays it out.
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
  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const names = financeSettings(data).accountNames
  const activities = useMemo(
    () =>
      new Map(
        buildActivities(data, names, getExternalLogosEnabled()).map((item) => [item.id, item]),
      ),
    [data, names],
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
  const chapter =
    view === 'chapters'
      ? data.insights?.evidence.chapters.find((candidate) => candidate.id === search.chapter)
      : undefined
  if (chapter)
    breadcrumbs.push({
      label: chapter.title,
      to: '/insights',
      search: { view: 'chapters', chapter: chapter.id },
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
      <WorkspaceHeader title="Insights" breadcrumbs={breadcrumbs} />
      <InsightsStrip data={data} current={view} today={today} />
      {view === 'trips' ? <TripsView data={data} activities={activities} /> : null}
      {view === 'people' ? <PeopleView data={data} activities={activities} /> : null}
      {view === 'chapters' ? <ChaptersView data={data} /> : null}
      {view === 'rules' ? <RulesView data={data} /> : null}
    </div>
  )
}
