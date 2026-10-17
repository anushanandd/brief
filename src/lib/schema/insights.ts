/** Everything Rust derives from the snapshot for the renderer to phrase: expectations, benefits, taxes, lots, credit, trips, and the rest. The types are generated from Rust's (`src/lib/bindings`). */
import type { Insights } from '../bindings/Insights'

export type { CardBenefits } from '../bindings/CardBenefits'
export type { ChartWindow as ChangeWindow } from '../bindings/ChartWindow'
export type { Documents as DocumentInsights } from '../bindings/Documents'
export type { Insights }
export type { Owner as SpendingOwner } from '../bindings/Owner'
export type { PeriodView as SpendingPeriodView } from '../bindings/PeriodView'
export type { Retirement } from '../bindings/Retirement'
export type { Rollup } from '../bindings/Rollup'
export type { Spent as SpendingFigures } from '../bindings/Spent'
export type { SpendingPeriod as SpendingPeriodJudgment } from '../bindings/SpendingPeriod'
export type { SpendingPeriodDetail } from '../bindings/SpendingPeriodDetail'
export type { TaxYear } from '../bindings/TaxYear'

/** Each value chart's change over each window. */
export type ChartChanges = Insights['chartChanges']
export type AgendaItem = Insights['agenda'][number]
export type ExpectedEvent = Insights['expected'][number]
export type BenefitWindow = NonNullable<Insights['benefits']>['windows'][number]
export type BenefitActivity = BenefitWindow['activity'][number]
export type BenefitHistory = NonNullable<Insights['benefits']>['history'][number]
export type BenefitPeriod = BenefitHistory['periods'][number]
export type BenefitReview = NonNullable<Insights['benefits']>['review'][number]
export type AttentionItem = Insights['attention'][number]
export type Allocation = NonNullable<Insights['allocation']>
export type Trip = Insights['trips']['trips'][number]
export type Trips = Insights['trips']
export type Taxes = Insights['taxes']
export type Subscription = Insights['subscriptions']['items'][number]
