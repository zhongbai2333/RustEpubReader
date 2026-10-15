import Foundation
import CoreText
@main struct SpeechChecks {
 static func main() async {
  let vectors = try! JSONDecoder().decode([String:String].self, from: Data(contentsOf:URL(fileURLWithPath:CommandLine.arguments[1])))
  for (timestamp, expected) in vectors { precondition(EdgeSpeechToken.generate(at: Date(timeIntervalSince1970:Double(timestamp)!)) == expected, "Checksum mismatch at \(timestamp)") }
  print("PASS: v2 speech protocol checksum vectors")
  let original = "甲😀e\u{301}👨‍👩‍👧‍👦乙，下一句。"
  let sources = [SpeechSourceBlock(text: original, anchor: .init(chapter: 0, block: 2)), SpeechSourceBlock(text: "后半段", anchor: .init(chapter: 0, block: 2, charOffset: original.unicodeScalars.count)), SpeechSourceBlock(text: " ", anchor: .init(chapter: 1, block: 0)), SpeechSourceBlock(text: "第二章", anchor: .init(chapter: 2, block: 1))]
  let units = SpeechText.units(sources, from: .init(chapter: 0), limit: 8)
  precondition(units.filter { $0.sourceIndex == 0 }.map(\.text).joined() == original)
  precondition(units.allSatisfy { $0.end.charOffset - $0.anchor.charOffset == $0.text.unicodeScalars.count })
  precondition(units.filter { $0.sourceIndex == 0 }.flatMap { Array($0.text) } == Array(original))
  precondition(!units.contains { $0.anchor.chapter == 1 })
  let resumed = SpeechText.units(sources, from: .init(chapter: 0, block: 2, charOffset: 2), limit: 8)
  precondition(resumed.first?.anchor.charOffset == 2)
  precondition(resumed.first?.text.first == "e\u{301}")
  precondition(resumed.last?.anchor.chapter == 2)
  precondition(SpeechText.scalarOffset(in: "甲😀乙", utf16Offset: 3) == 2)
  precondition(SpeechText.xml("<&'\"\u{1}") == "&lt;&amp;&apos;&quot;")
  let fragments = LaidOutChapter(text: NSAttributedString(string:"前\n\u{fffc}\n后😀\n"), coordinates:[.init(block:2,start:0,source:"前",base:0),.init(block:2,start:2,source:"",base:1),.init(block:2,start:4,source:"后😀",base:1)])
  precondition(fragments.utf16(for: .init(chapter:0,block:2,charOffset:2)) == 5)
  let header = Data("Path:audio\r\nContent-Type:audio/mpeg\r\n".utf8)
  var frame = Data([UInt8(header.count >> 8), UInt8(header.count & 255)]);frame.append(header);frame.append(Data([1,2,3]))
  precondition(try! EdgeSpeech.audioPayload(frame) == Data([1,2,3]))
  do { _ = try EdgeSpeech.audioPayload(Data([0, 10, 2])); fatalError("Accepted truncated audio header") } catch {}
  let markers = EdgeSpeech.markers([(0,"甲"),(0.5,"😀"),(1,"甲")],text:"甲😀甲")
  precondition(markers.map(\.offset) == [0,1,2] && markers[1].length == 1)
  print("PASS: grapheme-safe chunks, scalar offsets, inline-image fragments, cross-chapter ordering, XML escaping, framed audio and word markers")
  if CommandLine.arguments.contains("--live-edge") {
   do {
    let audio = try await EdgeSpeech.synthesize(text:"春江潮水连海平，海上明月共潮生。",voice:"zh-CN-XiaoxiaoNeural",rate:1)
    try audio.data.write(to:URL(fileURLWithPath:CommandLine.arguments.last!))
    print("LIVE_EDGE bytes=\(audio.data.count) markers=\(audio.markers.count)")
   } catch { let e=error as NSError;print("LIVE_EDGE_FAILED domain=\(e.domain) code=\(e.code)");exit(1) }
  }
 }
}
