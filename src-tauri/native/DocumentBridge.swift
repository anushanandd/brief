import AppKit
import Foundation
import PDFKit
import Vision

/// Reads a document's text on this Mac, never over the network: a PDF's own text layer line by
/// line, and, when asked or when there is none, Vision's recognition of each page as an image.
/// Each line carries its box in page fractions measured from the top left, so readers can tell
/// columns apart.

private let maxPages = 24
/// Pages are recognized at about this width in pixels: enough for small print on a statement.
private let renderWidth: CGFloat = 2200

private func json(_ value: Any) -> UnsafeMutablePointer<CChar>? {
  guard let data = try? JSONSerialization.data(withJSONObject: value),
    let text = String(data: data, encoding: .utf8)
  else { return nil }
  return strdup(text)
}

private func line(_ text: String, _ box: CGRect, _ confidence: Float) -> [String: Any] {
  [
    "text": text, "x": box.minX, "y": box.minY, "w": box.width, "h": box.height,
    "confidence": confidence,
  ]
}

/// Vision's lines in `image`, boxes converted from its bottom-left origin.
private func recognize(_ image: CGImage) -> [[String: Any]] {
  let request = VNRecognizeTextRequest()
  request.recognitionLevel = .accurate
  // Figures and identifiers must come back as printed, not as dictionary words.
  request.usesLanguageCorrection = false
  guard (try? VNImageRequestHandler(cgImage: image, options: [:]).perform([request])) != nil
  else { return [] }
  return (request.results ?? []).compactMap { observation in
    guard let best = observation.topCandidates(1).first else { return nil }
    let box = observation.boundingBox
    return line(
      best.string, CGRect(x: box.minX, y: 1 - box.maxY, width: box.width, height: box.height),
      best.confidence)
  }
}

/// The text layer's lines on `page`, in reading order.
private func textLines(_ page: PDFPage) -> [[String: Any]] {
  let bounds = page.bounds(for: .mediaBox)
  guard bounds.width > 0, bounds.height > 0,
    let selection = page.selection(for: bounds)
  else { return [] }
  return selection.selectionsByLine().compactMap { selected in
    guard let text = selected.string?.trimmingCharacters(in: .whitespacesAndNewlines),
      !text.isEmpty
    else { return nil }
    let box = selected.bounds(for: page)
    return line(
      text,
      CGRect(
        x: (box.minX - bounds.minX) / bounds.width,
        y: 1 - (box.maxY - bounds.minY) / bounds.height,
        width: box.width / bounds.width, height: box.height / bounds.height), 1)
  }
}

private func render(_ page: PDFPage) -> CGImage? {
  let bounds = page.bounds(for: .mediaBox)
  guard bounds.width > 0 else { return nil }
  let scale = renderWidth / bounds.width
  let image = page.thumbnail(
    of: CGSize(width: bounds.width * scale, height: bounds.height * scale), for: .mediaBox)
  return image.cgImage(forProposedRect: nil, context: nil, hints: nil)
}

/// `{pages: [{source, lines}], encrypted}` for the file at `path`, or null when it can't be
/// read. `recognize` asks for Vision's reading of every PDF page; a page without a text layer
/// and every image are recognized regardless.
@_cdecl("brief_document_read")
public func briefDocumentRead(_ path: UnsafePointer<CChar>?, _ recognizePages: Int32)
  -> UnsafeMutablePointer<CChar>?
{
  guard let path else { return nil }
  let url = URL(fileURLWithPath: String(cString: path))
  if let document = PDFDocument(url: url) {
    if document.isLocked {
      return json(["pages": [Any](), "encrypted": true])
    }
    var pages = [[String: Any]]()
    for index in 0..<min(document.pageCount, maxPages) {
      guard let page = document.page(at: index) else { continue }
      let text = textLines(page)
      if recognizePages != 0 || text.isEmpty, let image = render(page) {
        pages.append(["source": "vision", "lines": recognize(image)])
      } else {
        pages.append(["source": "text", "lines": text])
      }
    }
    return json(["pages": pages, "encrypted": false])
  }
  guard let image = NSImage(contentsOf: url),
    let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
  else { return nil }
  return json(["pages": [["source": "vision", "lines": recognize(cgImage)]], "encrypted": false])
}

@_cdecl("brief_document_free")
public func briefDocumentFree(_ value: UnsafeMutablePointer<CChar>?) {
  free(value)
}
