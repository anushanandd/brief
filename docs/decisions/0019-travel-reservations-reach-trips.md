# 0019: Travel reservations from your own calendar reach trips

2026-11-13 · Accepted

## Context

Calendar events were read for the Calendar page and a trip's page, and never reached insights. But booking emails put flights and stays in the calendar ("Flight to Chicago (UA 1811)", "Stay at Example Hotel"). They show trips purchases never see, such as one where someone else paid, and they date a trip exactly.

Shared calendars carry other people's travel too: a family calendar holds trips the user didn't take.

## Decision

Insights read one kind of event: flights and stays, by their titles, from calendars named for an email account, which is how each of the user's own accounts names its main calendar. Shared calendars are left out, even when an event names the user.

- They are read through EventKit when the view is derived, only once Calendar access is granted (never asking), at most hourly, and kept only in memory.
- `trips()` takes them as an argument, so tests pass synthetic ones and the browser preview has none.
- The trips built from them are saved like any trip: dates, cities, and whether the user flew, including in the data folder. Event titles, notes, and attendees never are.

Nothing else from Calendar reaches insights.

## Consequences

- Results now also depend on the user's calendar, not only saved data. A trip can change when a reservation is added, and the view is derived again within the hour.
- A flight titled only with airport codes is skipped rather than adding an airport table.
- Days are the Mac's local days, so a long flight east can land a day early.
- Someone else's main calendar shared under its email address would count; that is unlikely enough to accept.
