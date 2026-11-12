/**
 * Rust's account totals and reports over the route scenarios, read from the fixture Rust
 * writes (`route_reports_match_the_renderer_fixture`), so no route test sums money itself.
 * Each scenario's saved data (the snapshot and settings, without the insights the harness
 * writes out) with the test's clock, and each report request, are keyed by their SHA-256.
 * A key the fixture lacks
 * fails the test; `BRIEF_RECORD_ROUTE_REPORTS=1` records it in `route-report-inputs.json` for
 * Rust to answer instead (see the regen-fixtures skill).
 */
import { createHash } from 'node:crypto'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'

import routeReports from '../data/fixtures/route-reports.json'
import type { ReportRequest } from '../lib/api'
import { fromRust } from '../lib/rust-json'
import type { FinanceSnapshot, Reports, Rollup } from '../lib/schema'

type Inputs = {
  snapshots: Record<string, unknown>
  requests: Record<string, { snapshot: string; request: ReportRequest }>
}

const inputsPath = new URL('../data/fixtures/route-report-inputs.json', import.meta.url)
const answered = fromRust<{
  rollup: Record<string, Rollup>
  reports: Record<string, Reports>
}>(routeReports)
const digest = (value: unknown) =>
  createHash('sha256').update(JSON.stringify(value)).digest('hex').slice(0, 16)

/** Adds a scenario, and a request over it, to the inputs Rust answers. */
function record(snapshot: string, saved: unknown, key: string, request?: ReportRequest) {
  const inputs: Inputs = existsSync(inputsPath)
    ? JSON.parse(readFileSync(inputsPath, 'utf8'))
    : { snapshots: {}, requests: {} }
  inputs.snapshots[snapshot] = saved
  if (request) inputs.requests[key] = { snapshot, request }
  writeFileSync(inputsPath, `${JSON.stringify(inputs)}\n`)
}

/** A scenario's finance as the harness serves it: a snapshot with its settings and insights. */
type Scenario = Pick<FinanceSnapshot, 'insights'> | { insights?: unknown }

export function routeFixture(kind: 'rollup', data: Scenario): Rollup
export function routeFixture(kind: 'reports', data: Scenario, request: ReportRequest): Reports
export function routeFixture(
  kind: 'rollup' | 'reports',
  data: Scenario,
  request?: ReportRequest,
): Rollup | Reports {
  // Rust reads the scenario as of the test's clock, which some tests set.
  const { insights: _insights, ...rest } = data
  const saved = { now: new Date().toISOString(), data: rest }
  const snapshot = digest(saved)
  const key = request ? `${snapshot}:${digest(request)}` : snapshot
  const found = answered[kind][key]
  if (found) return found
  if (process.env.BRIEF_RECORD_ROUTE_REPORTS === '1') {
    record(snapshot, saved, key, request)
    throw new Error(`Recorded the route scenario ${key}; let Rust answer it, then run again`)
  }
  throw new Error(
    `Rust has no ${kind} for this route scenario (${key}); record it as the regen-fixtures skill says`,
  )
}
