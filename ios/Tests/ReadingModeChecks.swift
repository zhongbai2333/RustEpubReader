import Foundation
@main struct Checks {
 static func main() {
  let firstSuite="first-reader-"+UUID().uuidString
  let first=UserDefaults(suiteName:firstSuite)!
  ReadingIntroduction.prepare(first); ReadingMode.migrate(first)
  precondition(first.string(forKey:"preview.readingStyle")=="paging")
  precondition(first.string(forKey:"preview.pageAnimation")=="curl")
  precondition(!first.bool(forKey:ReadingIntroduction.storageKey))
  ReadingIntroduction.prepare(first)
  precondition(!first.bool(forKey:ReadingIntroduction.storageKey))
  first.set(true,forKey:ReadingIntroduction.storageKey);ReadingIntroduction.prepare(first)
  precondition(first.bool(forKey:ReadingIntroduction.storageKey))
  first.removePersistentDomain(forName:firstSuite)
  let oldSuite="old-reader-"+UUID().uuidString
  let old=UserDefaults(suiteName:oldSuite)!
  old.set("scroll",forKey:"preview.readingMode")
  ReadingIntroduction.prepare(old);ReadingMode.migrate(old)
  precondition(old.bool(forKey:ReadingIntroduction.storageKey))
  precondition(old.string(forKey:"preview.readingStyle")=="scroll")
  old.removePersistentDomain(forName:oldSuite)

  for native in [false,true] { for slide in [false,true] {
   let suite = "reading-modes-" + UUID().uuidString
   let defaults = UserDefaults(suiteName:suite)!
   defaults.set(native,forKey:"preview.nativeReader"); defaults.set(slide,forKey:"preview.sliding")
   ReadingMode.migrate(defaults)
   precondition(defaults.string(forKey:"preview.readingMode") == (native ? (slide ? "slide":"curl") : "scroll"))
   defaults.set("slide",forKey:"preview.readingMode"); ReadingMode.migrate(defaults)
   precondition(defaults.string(forKey:"preview.readingMode")=="slide")
   defaults.removePersistentDomain(forName:suite)
  }}
  for previous in ["scroll", "curl", "slide"] {
   let suite="reading-mode-split-" + UUID().uuidString
   let defaults=UserDefaults(suiteName:suite)!
   defaults.set(previous,forKey:"preview.readingMode"); ReadingMode.migrate(defaults)
   precondition(defaults.string(forKey:"preview.readingStyle") == (previous == "scroll" ? "scroll" : "paging"))
   precondition(defaults.string(forKey:"preview.pageAnimation") == (previous == "scroll" ? "curl" : previous))
   defaults.set("scroll",forKey:"preview.readingStyle"); defaults.set("none",forKey:"preview.pageAnimation"); ReadingMode.migrate(defaults)
   precondition(defaults.string(forKey:"preview.pageAnimation") == "none")
   precondition(ReadingMode.resolve(style:"scroll",animation:"cover") == .scroll)
   precondition(ReadingMode.resolve(style:"paging",animation:"none") == .none)
   precondition(ReadingMode.resolve(style:"paging",animation:"cover") == .cover)
   defaults.removePersistentDomain(forName:suite)
  }
  precondition(ReadingSpread.columns(width:874,height:402,margin:24,fontSize:18)==2)
  precondition(ReadingSpread.columns(width:402,height:874,margin:24,fontSize:18)==1)
  precondition(ReadingSpread.columns(width:640,height:360,margin:24,fontSize:28)==1)
  precondition(ReadingSpread.start(5,columns:2)==4)
  print("PASS: preference migration, choice persistence, phone landscape spread, portrait and large-font fallback")
 }
}
