import Contacts
import EventKit
import Foundation

// macOS Calendar through EventKit, with full access: Brief writes only to its own calendar,
// and reads the others for the Calendar page and trips. Every function takes and returns JSON
// and runs on one serial queue, because an event store isn't safe to share across threads.

public typealias BriefCalendarCallback = @convention(c) (
  UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?
) -> Void

private let queue = DispatchQueue(label: "brief.calendar")
// One long-lived store, as EventKit recommends; made again after access changes, since a store
// made before the grant sees no calendars.
nonisolated(unsafe) private var sharedStore: EKEventStore?
private let maxEvents = 2_000

private func store() -> EKEventStore {
  if let sharedStore { return sharedStore }
  let made = EKEventStore()
  sharedStore = made
  return made
}

private func json(_ value: Any) -> UnsafeMutablePointer<CChar>? {
  guard let data = try? JSONSerialization.data(withJSONObject: value),
    let text = String(data: data, encoding: .utf8)
  else { return nil }
  return strdup(text)
}

private func input(_ text: UnsafePointer<CChar>?) -> [String: Any]? {
  guard let text, let data = String(cString: text).data(using: .utf8) else { return nil }
  return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
}

private let calendar = Calendar.current

private func day(_ text: Any?) -> Date? {
  guard let text = text as? String else { return nil }
  let parts = text.split(separator: "-").compactMap { Int($0) }
  guard parts.count == 3 else { return nil }
  return calendar.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2]))
}

private func dayText(_ date: Date) -> String {
  let parts = calendar.dateComponents([.year, .month, .day], from: date)
  return String(format: "%04d-%02d-%02d", parts.year ?? 0, parts.month ?? 0, parts.day ?? 0)
}

private let instantFormatter: ISO8601DateFormatter = {
  let formatter = ISO8601DateFormatter()
  formatter.timeZone = TimeZone.current
  return formatter
}()

private func sourceKind(_ source: EKSource) -> String {
  switch source.sourceType {
  case .local: return "local"
  case .calDAV: return source.title.localizedCaseInsensitiveContains("icloud") ? "icloud" : "caldav"
  case .exchange: return "exchange"
  default: return "other"
  }
}

private func describe(_ calendar: EKCalendar) -> [String: Any] {
  ["id": calendar.calendarIdentifier, "title": calendar.title, "source": calendar.source.title]
}

/// 0 not asked, 1 restricted, 2 denied, 3 full access, 4 write-only.
@_cdecl("brief_calendar_status")
public func briefCalendarStatus() -> Int32 {
  switch EKEventStore.authorizationStatus(for: .event) {
  case .notDetermined: return 0
  case .restricted: return 1
  case .denied: return 2
  case .fullAccess: return 3
  case .writeOnly: return 4
  @unknown default: return 2
  }
}

@_cdecl("brief_calendar_request")
public func briefCalendarRequest(
  _ completion: UnsafeMutableRawPointer?, _ callback: @escaping BriefCalendarCallback
) {
  // The pointer is Rust's and only handed back; carry it as an integer across threads.
  let context = Int(bitPattern: completion)
  queue.async {
    store().requestFullAccessToEvents { granted, error in
      queue.async { sharedStore = nil }
      let pointer = UnsafeMutableRawPointer(bitPattern: context)
      if let error, !granted {
        error.localizedDescription.withCString { callback(pointer, 0, $0) }
      } else {
        callback(pointer, granted ? 1 : 0, nil)
      }
    }
  }
}

/// Accounts a calendar can be made in, the one holding the default calendar first, as
/// `[{id, title, kind}]`.
@_cdecl("brief_calendar_sources")
public func briefCalendarSources() -> UnsafeMutablePointer<CChar>? {
  queue.sync {
    let store = store()
    let preferred = store.defaultCalendarForNewEvents?.source.sourceIdentifier
    let usable = store.sources.filter { [.local, .calDAV, .exchange].contains($0.sourceType) }
    let sources =
      usable.filter { $0.sourceIdentifier == preferred }
      + usable.filter { $0.sourceIdentifier != preferred }
    return json(
      sources.map { ["id": $0.sourceIdentifier, "title": $0.title, "kind": sourceKind($0)] })
  }
}

/// `{id?, sourceId?}`: the saved calendar when it still exists. Given `sourceId`, else a
/// calendar the user already named "Brief" that Brief can write to (Google accounts don't let
/// apps make calendars, so one made in Google Calendar is used), else a new "Brief" calendar
/// in `sourceId`. Answers `{id, title, source}`, `{missing: true}`, or `{error}`.
@_cdecl("brief_calendar_ensure")
public func briefCalendarEnsure(_ request: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  queue.sync {
    let store = store()
    let request = input(request) ?? [:]
    if let id = request["id"] as? String, let found = store.calendar(withIdentifier: id) {
      return json(describe(found))
    }
    guard let sourceId = request["sourceId"] as? String else { return json(["missing": true]) }
    let named = store.calendars(for: .event).filter {
      $0.title.trimmingCharacters(in: .whitespaces).caseInsensitiveCompare("Brief") == .orderedSame
        && $0.allowsContentModifications
    }
    if let found = named.first(where: { $0.source.sourceIdentifier == sourceId }) ?? named.first {
      return json(describe(found))
    }
    guard let source = store.sources.first(where: { $0.sourceIdentifier == sourceId }) else {
      return json(["error": "That calendar account is no longer available"])
    }
    let made = EKCalendar(for: .event, eventStore: store)
    made.title = "Brief"
    made.source = source
    do {
      try store.saveCalendar(made, commit: true)
      return json(describe(made))
    } catch {
      return json(["error": error.localizedDescription])
    }
  }
}

/// Every event calendar, as `[{id, title, source, color?}]`, by account then title.
@_cdecl("brief_calendar_list")
public func briefCalendarList() -> UnsafeMutablePointer<CChar>? {
  queue.sync {
    let calendars = store().calendars(for: .event).sorted {
      ($0.source.title, $0.title) < ($1.source.title, $1.title)
    }
    return json(
      calendars.map { found -> [String: Any] in
        var row = describe(found)
        if let color = hex(found) { row["color"] = color }
        return row
      })
  }
}

private func hex(_ found: EKCalendar) -> String? {
  guard let color = found.cgColor?.components, color.count >= 3 else { return nil }
  return String(
    format: "#%02X%02X%02X", Int(color[0] * 255), Int(color[1] * 255), Int(color[2] * 255))
}

/// `{start, end, calendarIds, notes?}`: events on those days in those calendars, leaving out
/// events declined. Notes are read only when asked for, to match Brief's own events.
@_cdecl("brief_calendar_events")
public func briefCalendarEvents(_ request: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  queue.sync {
    let store = store()
    guard let request = input(request), let start = day(request["start"]),
      let last = day(request["end"]),
      let end = calendar.date(byAdding: .day, value: 1, to: last)
    else { return nil }
    let calendars = (request["calendarIds"] as? [String] ?? []).compactMap {
      store.calendar(withIdentifier: $0)
    }
    if calendars.isEmpty { return json([]) }
    let contacts =
      CNContactStore.authorizationStatus(for: .contacts) == .authorized ? CNContactStore() : nil
    var found = [[String: Any]]()
    // A predicate spans at most four years; the renderer asks for a month or a trip.
    for event in store.events(matching: store.predicateForEvents(
      withStart: start, end: end, calendars: calendars))
    {
      if found.count >= maxEvents { break }
      let me = event.attendees?.first(where: \.isCurrentUser)
      if me?.participantStatus == .declined { continue }
      var row: [String: Any] = [
        "id": event.eventIdentifier ?? "",
        "calendar": event.calendar.title,
        "title": event.title ?? "",
        "allDay": event.isAllDay,
      ]
      if event.isAllDay {
        row["start"] = dayText(event.startDate)
        row["end"] = dayText(max(event.startDate, event.endDate.addingTimeInterval(-1)))
      } else {
        row["start"] = instantFormatter.string(from: event.startDate)
        row["end"] = instantFormatter.string(from: event.endDate)
      }
      if let color = hex(event.calendar) { row["color"] = color }
      if let url = event.url?.absoluteString { row["url"] = url }
      if let notes = event.notes, request["notes"] as? Bool == true { row["notes"] = notes }
      if let location = event.location, !location.isEmpty { row["location"] = location }
      let attendees = (event.attendees ?? []).filter { !$0.isCurrentUser }.map {
        participant -> [String: Any] in
        var attendee: [String: Any] = ["name": participant.name ?? ""]
        if let contacts,
          let match = try? contacts.unifiedContacts(
            matching: participant.contactPredicate, keysToFetch: []
          ).first
        {
          attendee["contactId"] = match.identifier
        }
        return attendee
      }
      if !attendees.isEmpty { row["attendees"] = attendees }
      found.append(row)
    }
    return json(found)
  }
}

/// `{calendarId, add: [{url, title, notes, start, end}], update: [{id, …}], remove: [id]}`,
/// saved in one commit. Dated all day, free, and floating, so a date stays its day anywhere.
/// Answers `{saved, failed}`.
@_cdecl("brief_calendar_apply")
public func briefCalendarApply(_ request: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  queue.sync {
    let store = store()
    guard let request = input(request), let id = request["calendarId"] as? String,
      let target = store.calendar(withIdentifier: id)
    else { return json(["error": "The Brief calendar is missing"]) }
    var saved = 0
    var failed = 0
    func fill(_ event: EKEvent, _ item: [String: Any]) -> Bool {
      guard let start = day(item["start"]), let end = day(item["end"] ?? item["start"]) else {
        return false
      }
      event.calendar = target
      event.title = item["title"] as? String
      event.notes = item["notes"] as? String
      event.url = (item["url"] as? String).flatMap(URL.init(string:))
      event.isAllDay = true
      event.timeZone = nil
      event.startDate = start
      event.endDate = end
      event.availability = .free
      return true
    }
    for item in request["add"] as? [[String: Any]] ?? [] {
      let event = EKEvent(eventStore: store)
      if fill(event, item), (try? store.save(event, span: .thisEvent, commit: false)) != nil {
        saved += 1
      } else {
        failed += 1
      }
    }
    for item in request["update"] as? [[String: Any]] ?? [] {
      guard let eventId = item["id"] as? String, let event = store.event(withIdentifier: eventId),
        event.calendar.calendarIdentifier == id
      else {
        failed += 1
        continue
      }
      if fill(event, item), (try? store.save(event, span: .thisEvent, commit: false)) != nil {
        saved += 1
      } else {
        failed += 1
      }
    }
    for eventId in request["remove"] as? [String] ?? [] {
      // Only ever an event in the Brief calendar.
      guard let event = store.event(withIdentifier: eventId),
        event.calendar.calendarIdentifier == id,
        (try? store.remove(event, span: .thisEvent, commit: false)) != nil
      else {
        failed += 1
        continue
      }
      saved += 1
    }
    do {
      try store.commit()
    } catch {
      store.reset()
      return json(["error": error.localizedDescription])
    }
    return json(["saved": saved, "failed": failed])
  }
}

@_cdecl("brief_calendar_free")
public func briefCalendarFree(_ value: UnsafeMutablePointer<CChar>?) {
  free(value)
}
