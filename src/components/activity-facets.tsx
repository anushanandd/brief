import type { ActivityView } from '../lib/activity-views'
import { CategoryMark } from './category-mark'
import { AlertCircle, Bookmark, X } from './icons'

/**
 * The shortcuts under Activity's search: the review queue, the categories that make up the
 * results, and saved views.
 */
export function ActivityFacets({
  review,
  reviewCount,
  onReview,
  facets,
  onFacet,
  savedViews,
  onOpenView,
  onDeleteView,
}: {
  review: boolean
  reviewCount: number
  onReview: () => void
  /** Categories in the results with their counts, largest first; empty to hide them. */
  facets: Array<[category: string, count: number]>
  onFacet: (category: string) => void
  savedViews: ActivityView[]
  onOpenView: (view: ActivityView) => void
  onDeleteView: (view: ActivityView) => void
}) {
  return (
    <div className="activity-facets">
      {reviewCount || review ? (
        <button
          type="button"
          className="activity-review-view"
          aria-pressed={review}
          data-command="activity.review"
          onClick={onReview}
        >
          <AlertCircle size={12} aria-hidden="true" />
          To review
          <span>{reviewCount}</span>
        </button>
      ) : null}
      {facets.length ? (
        <div
          className="activity-facet-list"
          role="group"
          aria-label="Narrow by category"
          data-keyboard-region
        >
          {facets.map(([category, count]) => (
            <button
              type="button"
              key={category}
              data-keyboard-row
              data-keyboard-open
              onClick={() => onFacet(category)}
            >
              <CategoryMark category={category} />
              {category}
              <span>{count}</span>
            </button>
          ))}
        </div>
      ) : null}
      {savedViews.length ? (
        <div className="activity-saved-views" role="group" aria-label="Saved views">
          {savedViews.map((view) => (
            <span className="activity-saved-view" key={view.name}>
              <button type="button" onClick={() => onOpenView(view)}>
                <Bookmark size={12} aria-hidden="true" />
                {view.name}
              </button>
              <button
                type="button"
                aria-label={`Delete saved view ${view.name}`}
                onClick={() => onDeleteView(view)}
              >
                <X size={12} aria-hidden="true" />
              </button>
            </span>
          ))}
        </div>
      ) : null}
    </div>
  )
}
