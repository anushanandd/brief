import Foundation
import NaturalLanguage

// On-device sentence embeddings for search, from Apple's Natural Language framework. Texts
// come in and vectors go out; nothing is sent anywhere.

private let embedding = NLEmbedding.sentenceEmbedding(for: .english)

/// A JSON array of strings in, a JSON array of vectors out (an empty vector for a text the
/// model can't embed), or null when this Mac has no sentence model.
@_cdecl("brief_embed")
public func briefEmbed(_ texts: UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>? {
  guard let texts, let embedding,
    let data = String(cString: texts).data(using: .utf8),
    let strings = try? JSONSerialization.jsonObject(with: data) as? [String]
  else { return nil }
  let vectors = strings.map { text in
    embedding.vector(for: text).map { $0.map { Float($0) } } ?? []
  }
  guard let output = try? JSONSerialization.data(withJSONObject: vectors),
    let json = String(data: output, encoding: .utf8)
  else { return nil }
  return strdup(json)
}

@_cdecl("brief_embed_free")
public func briefEmbedFree(_ value: UnsafeMutablePointer<CChar>?) {
  free(value)
}
