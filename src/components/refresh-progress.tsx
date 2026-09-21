import { useEffect, useState } from 'react'

export type RefreshProgressTask = {
  id: string
  label: string
  status: string
  state: 'active' | 'cached' | 'complete' | 'waiting' | 'warning'
}

export function RefreshProgress({
  detail,
  startedAt,
  tasks = [],
  finished = false,
}: {
  detail?: string
  startedAt: number
  tasks?: RefreshProgressTask[]
  finished?: boolean
}) {
  const [elapsed, setElapsed] = useState(() => Math.floor((Date.now() - startedAt) / 1_000))

  useEffect(() => {
    if (finished) return undefined
    const timer = window.setInterval(
      () => setElapsed(Math.floor((Date.now() - startedAt) / 1_000)),
      1_000,
    )
    return () => window.clearInterval(timer)
  }, [finished, startedAt])

  return (
    <div className="refresh-progress">
      {detail ? <span>{detail}</span> : null}
      {tasks.length ? (
        <ul className="refresh-progress-tasks">
          {tasks.map((task) => (
            <li key={task.id}>
              <span className={`refresh-task-state ${task.state}`} aria-hidden="true" />
              <span>{task.label}</span>
              <small>{task.status}</small>
            </li>
          ))}
        </ul>
      ) : null}
      {finished ? null : (
        <span
          className="refresh-progress-track"
          role="progressbar"
          aria-label="Refresh in progress"
        >
          <span />
        </span>
      )}
      <small>
        {finished
          ? `${elapsed}s total`
          : `${elapsed}s elapsed · Your saved data remains available until this refresh is validated.`}
      </small>
    </div>
  )
}
