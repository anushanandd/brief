/** Reports Rust computes on request over chosen days and accounts. The types are generated from Rust's (`src/lib/bindings`). */
import type { Reports } from '../bindings/Reports'

export type { Reports }
export type Report = NonNullable<Reports['report']>
export type ReportSummary = Reports['summaries'][number]
export type ReportEntry = Report['entries'][number]
export type CashFlow = NonNullable<Reports['flow']>
export type CashFlowItem = CashFlow['sources'][number]
