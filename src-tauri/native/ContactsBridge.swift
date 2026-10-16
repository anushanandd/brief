import Contacts
import Foundation

// Read-only access to Mac Contacts for People: names to pick from, and photos. Brief never
// writes to Contacts and never reads phone numbers, emails, or addresses.

public typealias BriefContactsCallback = @convention(c) (
  UnsafeMutableRawPointer?, Int32, UnsafePointer<CChar>?
) -> Void

private let maxResults = 25
private let maxPhotos = 100

private func owned(_ text: String) -> UnsafeMutablePointer<CChar>? {
  strdup(text)
}

private func json(_ value: Any) -> UnsafeMutablePointer<CChar>? {
  guard let data = try? JSONSerialization.data(withJSONObject: value),
    let text = String(data: data, encoding: .utf8)
  else { return nil }
  return owned(text)
}

/// 0 not asked, 1 restricted, 2 denied, 3 allowed, 4 allowed for some contacts.
@_cdecl("brief_contacts_status")
public func briefContactsStatus() -> Int32 {
  switch CNContactStore.authorizationStatus(for: .contacts) {
  case .notDetermined: return 0
  case .restricted: return 1
  case .denied: return 2
  case .authorized: return 3
  @unknown default:
    // A newer partial grant (`.limited`) still reads the contacts it covers.
    return 4
  }
}

@_cdecl("brief_contacts_request")
public func briefContactsRequest(
  _ completion: UnsafeMutableRawPointer?, _ callback: @escaping BriefContactsCallback
) {
  // The pointer is Rust's and only handed back; carry it as an integer across threads.
  let context = Int(bitPattern: completion)
  CNContactStore().requestAccess(for: .contacts) { granted, error in
    let pointer = UnsafeMutableRawPointer(bitPattern: context)
    if let error, !granted {
      error.localizedDescription.withCString { callback(pointer, 0, $0) }
    } else {
      callback(pointer, granted ? 1 : 0, nil)
    }
  }
}

/// Contacts whose name matches `query`, as a JSON array of `{identifier, name}`, or null when
/// Contacts can't be read.
@_cdecl("brief_contacts_search")
public func briefContactsSearch(_ query: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  guard let query else { return nil }
  let text = String(cString: query).trimmingCharacters(in: .whitespacesAndNewlines)
  if text.isEmpty { return json([]) }
  let keys: [CNKeyDescriptor] = [
    CNContactFormatter.descriptorForRequiredKeys(for: .fullName),
    CNContactOrganizationNameKey as CNKeyDescriptor,
    CNContactImageDataAvailableKey as CNKeyDescriptor,
  ]
  do {
    let contacts = try CNContactStore().unifiedContacts(
      matching: CNContact.predicateForContacts(matchingName: text), keysToFetch: keys)
    let found = contacts.compactMap { contact -> [String: Any]? in
      let name =
        CNContactFormatter.string(from: contact, style: .fullName) ?? contact.organizationName
      guard !name.isEmpty else { return nil }
      return [
        "identifier": contact.identifier, "name": name, "hasPhoto": contact.imageDataAvailable,
      ]
    }
    return json(Array(found.prefix(maxResults)))
  } catch {
    return nil
  }
}

/// Thumbnails for the given identifiers (a JSON array), as a JSON object of base64 PNG or JPEG
/// data by identifier. A contact without a photo, or no longer present, is left out.
@_cdecl("brief_contacts_photos")
public func briefContactsPhotos(_ identifiers: UnsafePointer<CChar>?) -> UnsafeMutablePointer<
  CChar
>? {
  guard let identifiers,
    let data = String(cString: identifiers).data(using: .utf8),
    let ids = try? JSONSerialization.jsonObject(with: data) as? [String]
  else { return nil }
  if ids.isEmpty { return json([String: String]()) }
  do {
    let contacts = try CNContactStore().unifiedContacts(
      matching: CNContact.predicateForContacts(withIdentifiers: Array(ids.prefix(maxPhotos))),
      keysToFetch: [CNContactThumbnailImageDataKey as CNKeyDescriptor])
    var photos = [String: String]()
    for contact in contacts {
      if let thumbnail = contact.thumbnailImageData {
        photos[contact.identifier] = thumbnail.base64EncodedString()
      }
    }
    return json(photos)
  } catch {
    return nil
  }
}

@_cdecl("brief_contacts_free")
public func briefContactsFree(_ value: UnsafeMutablePointer<CChar>?) {
  free(value)
}
