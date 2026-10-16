/** Refresh runs and the data health report. */
import { z } from 'zod'

const providerDiagnosticSchema = z.object({
  provider: z.string().max(32),
  endpoint: z.string().max(96),
  kind: z.string().max(32),
  httpStatus: z.number().int().min(100).max(599).optional(),
  errorType: z.string().max(64).optional(),
  errorCode: z.string().max(64).optional(),
  requestRef: z.string().length(12).optional(),
  attempts: z.number().int().min(0).max(3),
  retryable: z.boolean(),
  retryAt: z.string().optional(),
})

export const syncRunSchema = z.object({
  id: z.string(),
  startedAt: z.string(),
  finishedAt: z.string(),
  outcome: z.enum(['committed', 'failed']),
  warnings: z.array(z.string()),
  errorCode: z.string().optional(),
  details: z.object({
    phase: z.string().max(32),
    providers: z.array(providerDiagnosticSchema).max(24),
    warningCount: z.number().int().nonnegative().max(999),
    /** What each provider used: counts and timings only. */
    runs: z
      .array(
        z.object({
          provider: z.string().max(32),
          requests: z.number().int().nonnegative(),
          retries: z.number().int().nonnegative(),
          bytes: z.number().int().nonnegative(),
          durationMs: z.number().int().nonnegative().optional(),
        }),
      )
      .max(12)
      .optional(),
    stages: z
      .object({
        fetchMs: z.number().int().nonnegative().optional(),
        projectMs: z.number().int().nonnegative().optional(),
        commitMs: z.number().int().nonnegative().optional(),
      })
      .optional(),
    snapshotBytes: z.number().int().nonnegative().optional(),
    /** How many saved transactions the refresh added, settled, removed, or amended. */
    changes: z
      .object({
        added: z.number().int().nonnegative(),
        settled: z.number().int().nonnegative(),
        removed: z.number().int().nonnegative(),
        amended: z.number().int().nonnegative(),
        backfilled: z.number().int().nonnegative(),
      })
      .optional(),
  }),
})
export type SyncRun = z.infer<typeof syncRunSchema>

// One investment summary run, stage by stage: counts and symbols, never amounts.
export const summaryRunSchema = z.object({
  id: z.string(),
  kind: z.enum(['summary', 'explanation', 'close']),
  scope: z.string(),
  start: z.string(),
  end: z.string(),
  startedAt: z.string(),
  finishedAt: z.string(),
  outcome: z.enum(['ok', 'partial', 'failed']),
  error: z.string().nullable(),
  stages: z.array(
    z.object({
      name: z.string(),
      millis: z.number().int().nonnegative(),
      outcome: z.enum(['ok', 'partial', 'failed', 'skipped']),
      detail: z.string().nullable(),
    }),
  ),
  providers: z.array(providerDiagnosticSchema).max(24),
})
export type SummaryRun = z.infer<typeof summaryRunSchema>

export const healthReportSchema = z.object({
  computedAt: z.string(),
  revision: z.number().int().nonnegative(),
  overallStatus: z.enum(['healthy', 'info', 'warning', 'error', 'critical']),
  counts: z.object({
    critical: z.number().int().nonnegative(),
    error: z.number().int().nonnegative(),
    warning: z.number().int().nonnegative(),
    info: z.number().int().nonnegative(),
  }),
  issues: z.array(
    z.object({
      id: z.string(),
      severity: z.enum(['info', 'warning', 'error', 'critical']),
      category: z.string(),
      title: z.string(),
      explanation: z.string(),
      evidence: z.array(z.string()),
      affectedItems: z.array(z.object({ id: z.string(), name: z.string() })),
      action: z.object({ label: z.string(), route: z.string() }),
    }),
  ),
})
export type HealthReport = z.infer<typeof healthReportSchema>

// How often the layered model's thresholds fired over saved sessions, before tuning them.
export const calibrationSchema = z.object({
  sessions: z.number().int().nonnegative(),
  first: z.string().nullable(),
  last: z.string().nullable(),
  fundGroups: z.number(),
  looseGroups: z.number(),
  notable: z.number(),
  perFund: z.array(
    z.object({
      fund: z.string(),
      flags: z.number().int().nonnegative(),
      sessions: z.number().int().nonnegative(),
      perMonth: z.number(),
    }),
  ),
  // Sessions that were quiet for the portfolio.
  quiet: z.number().int().nonnegative(),
})
export type Calibration = z.infer<typeof calibrationSchema>
