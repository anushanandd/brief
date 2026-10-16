import { useState } from 'react'

import { downloadActivityCsv } from '../lib/activity-csv'

/** Saves activity as CSV, one export at a time, remembering whether the last one failed. */
export function useCsvExport() {
  const [exporting, setExporting] = useState(false)
  const [failed, setFailed] = useState(false)
  const run = async (...args: Parameters<typeof downloadActivityCsv>) => {
    if (exporting) return
    setExporting(true)
    setFailed(false)
    try {
      await downloadActivityCsv(...args)
    } catch {
      setFailed(true)
    } finally {
      setExporting(false)
    }
  }
  return { exporting, failed, run }
}
