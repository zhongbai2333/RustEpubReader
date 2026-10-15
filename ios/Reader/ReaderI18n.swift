import Foundation
import Combine

/// Catalog files are copied from core/src/i18n at build time; there is no separate iOS dictionary.
final class ReaderI18n: ObservableObject, @unchecked Sendable {
    static let shared = ReaderI18n()
    @Published private(set) var preference: String
    @Published private(set) var resolvedCode: String
    private let defaults:UserDefaults
    private let catalogs: [String: [String:String]]
    private let lock=NSLock()
    private var selected: [String:String]
    private var messageCache=[String:String]()
    private var observers=[NSObjectProtocol]()
    var choices:[(code:String,title:String)] {[("auto",text("language.auto")),("zh_cn","中文"),("en","English")]}
    var choiceLabel:String {choices.first(where: {$0.code==preference})?.title ?? text("language.auto")}
    var locale: Locale {Locale(identifier: resolvedCode=="zh_cn" ? "zh-Hans" : "en")}
    init(defaults:UserDefaults = .standard, bundle:Bundle = .main) {
        self.defaults=defaults
        var loaded=[String:[String:String]]()
        for code in ["en","zh_cn"] {
            if let path=bundle.url(forResource:code,withExtension:"json",subdirectory:"i18n"),
               let data=try? Data(contentsOf:path),let values=try? JSONDecoder().decode([String:String].self,from:data),!values.isEmpty {loaded[code]=values}
        }
        catalogs=loaded
        let initialPreference=Self.normalize(defaults.string(forKey:"reader.language") ?? "auto")
        let initialCode=Self.resolve(initialPreference,system:Locale.preferredLanguages.first ?? "en")
        preference=initialPreference;resolvedCode=initialCode
        selected=(loaded["en"] ?? [:]).merging(loaded[initialCode] ?? [:],uniquingKeysWith: {_,new in new})
        observers.append(NotificationCenter.default.addObserver(forName:NSLocale.currentLocaleDidChangeNotification,object:nil,queue:.main) { [weak self] _ in self?.refresh() })
    }
    deinit {observers.forEach {NotificationCenter.default.removeObserver($0)}}
    static func normalize(_ code:String)->String {
        switch code.trimmingCharacters(in:.whitespacesAndNewlines).lowercased().replacingOccurrences(of:"_",with:"-").split(separator:"-").first {
        case "zh":return "zh_cn";case "en":return "en";default:return "auto"
        }
    }
    static func resolve(_ choice:String,system:String)->String {
        let choice=normalize(choice)
        return choice=="auto" ? (normalize(system)=="zh_cn" ? "zh_cn" : "en") : choice
    }
    func setLanguage(_ choice:String) {
        preference=Self.normalize(choice);defaults.set(preference,forKey:"reader.language");refresh()
    }
    func refresh(system:String=Locale.preferredLanguages.first ?? "en") {
        let resolved=Self.resolve(preference,system:system)
        guard resolved != resolvedCode else {return}
        let values=(catalogs["en"] ?? [:]).merging(catalogs[resolved] ?? [:],uniquingKeysWith: {_,new in new})
        guard !values.isEmpty else {return}
        lock.lock();selected=values;lock.unlock();resolvedCode=resolved
    }
    func text(_ key:String)->String {
        lock.lock();defer {lock.unlock()}
        return selected[key] ?? catalogs["en"]?[key] ?? (resolvedCode=="zh_cn" ? "暂时不可用" : "Unavailable")
    }
    func count(_ key:String,_ value:Int)->String {
        let variant=key+(value==1 ? ".one" : ".other")
        lock.lock();let available=selected[variant] != nil;lock.unlock()
        return Self.format(text(available ? variant : key),args:[String(value)])
    }
    /// Only application status messages belong here, never book content or notes.
    func message(_ value:String)->String {
        let key=resolvedCode+":"+value
        lock.lock();let cached=messageCache[key];lock.unlock()
        if let cached {return cached}
        let translated=resolveMessage(value)
        lock.lock();if messageCache.count>=128 {messageCache.removeAll()};messageCache[key]=translated;lock.unlock()
        return translated
    }
    private func resolveMessage(_ value:String)->String {
        for code in ["en","zh_cn"] {
            let catalog=catalogs[code] ?? [:]
            if let pair=catalog.first(where: {$0.value==value}) {return text(pair.key)}
            for (key,template) in catalog.sorted(by: {$0.value.count > $1.value.count}) {
                guard let slots=try? NSRegularExpression(pattern:#"\{(\d*)\}"#) else {continue}
                let matches=slots.matches(in:template,range:NSRange(template.startIndex...,in:template))
                guard !matches.isEmpty else {continue}
                var pattern="^",last=template.startIndex,indices=[Int]()
                for m in matches {
                    guard let r=Range(m.range,in:template),let n=Range(m.range(at:1),in:template) else {continue}
                    pattern+=NSRegularExpression.escapedPattern(for:String(template[last..<r.lowerBound]))+"(.*?)"
                    indices.append(Int(template[n]) ?? indices.count);last=r.upperBound
                }
                pattern+=NSRegularExpression.escapedPattern(for:String(template[last...]))+"$"
                if let re=try? NSRegularExpression(pattern:pattern),let match=re.firstMatch(in:value,range:NSRange(value.startIndex...,in:value)) {
                    var args=Array(repeating:"",count:(indices.max() ?? 0)+1)
                    for (group,index) in indices.enumerated() {if let r=Range(match.range(at:group+1),in:value) {args[index]=String(value[r])}}
                    return Self.format(text(key),args:args)
                }
            }
        }
        return value
    }
    static func format(_ template:String,args:[String],named:[String:String]=[:])->String {
        var out="",at=template.startIndex,next=0
        while at<template.endIndex {
            let tail=template[at...]
            if tail.hasPrefix("{{") {out.append("{");at=template.index(at,offsetBy:2);continue}
            if tail.hasPrefix("}}") {out.append("}");at=template.index(at,offsetBy:2);continue}
            if template[at]=="{",let end=tail.firstIndex(of:"}") {
                let name=String(template[template.index(after:at)..<end])
                let value:String?
                if name.isEmpty {value=args.indices.contains(next) ? args[next] : nil;next+=1}
                else if let i=Int(name) {value=args.indices.contains(i) ? args[i] : nil}
                else {value=named[name]}
                if let value {out+=value;at=template.index(after:end);continue}
            }
            out.append(template[at]);at=template.index(after:at)
        }
        return out
    }
}
func L(_ key:String,_ args:Any...)->String {
    ReaderI18n.format(ReaderI18n.shared.text(key),args:args.map {String(describing:$0)})
}

func LM(_ value:String)->String {ReaderI18n.shared.message(value)}

#if DEBUG
extension ReaderI18n {
    static func verifyBundled() {
        let suite="i18n-runtime-"+UUID().uuidString,defaults=UserDefaults(suiteName:suite)!
        defer {defaults.removePersistentDomain(forName:suite)}
        let i=ReaderI18n(defaults:defaults)
        i.setLanguage("auto");i.refresh(system:"zh-Hant-HK")
        let auto=i.preference=="auto" && i.resolvedCode=="zh_cn" && i.text("common.cancel")=="取消"
        let old=i.text("tts.playing")
        i.setLanguage("en");i.refresh(system:"zh-CN")
        let explicit=i.resolvedCode=="en" && i.preference=="en" && defaults.string(forKey:"reader.language")=="en"
        let status=i.message(old)==i.text("tts.playing")
        let plural=i.count("ui.library_count",1)=="1 book" && i.count("ui.library_count",2)=="2 books"
        let braces=format("{1}/{0}",args:["😀","{}"] )=="{}/😀"
        let fallback=resolve("auto",system:"fr-FR")=="en" && i.text("missing")=="Unavailable"
        let report=["auto":auto,"explicit":explicit,"status":status,"plural":plural,"braces":braces,"fallback":fallback]
        let cache=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0]
        if let data=try? JSONSerialization.data(withJSONObject:report,options:[.prettyPrinted,.sortedKeys]) {try? data.write(to:cache.appendingPathComponent("i18n-verification.json"))}
    }
}
#endif
