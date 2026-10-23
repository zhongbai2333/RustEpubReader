import SwiftUI

#if DEBUG
/// Disposable local fixture; no user Documents or library are read or replaced.
struct TxtImportUITestHost: View {
    @StateObject private var library: PreviewLibrary
    @State private var source: URL?
    @State private var presented = false
    init() {
        let root = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0].appendingPathComponent("txt-ui-" + UUID().uuidString)
        _library = StateObject(wrappedValue: PreviewLibrary(rootOverride: root))
    }
    var body: some View {
        VStack {
            Text("TXT verification")
            Text("\(library.books.count)").accessibilityIdentifier("txt-imported-count")
            Button("Open TXT") { presented = true }.accessibilityIdentifier("txt-open")
        }
        .sheet(isPresented: $presented) { if let source { TxtImportView(source: source, library: library) } }
        .task {
            ReaderI18n.shared.setLanguage(CommandLine.arguments.contains("--txt-chinese") ? "zh_cn" : "en")
            let url = URL(fileURLWithPath: library.client.root).appendingPathComponent("mixed.txt")
            let body = String(repeating: "这是保留完整的测试正文，不应被误认为标题。", count: 12)
            let text = "第一章 初到\n\n\(body)\n\n第2章 相逢\n\n\(body)\n\n3. 远行\n\n\(body)\n\n4- 回家\n\n\(body)\n\n§ 未知格式：风从海上来 §\n\n\(body)"
            do {
                try text.write(to: url, atomically: true, encoding: .utf8)
                source = url; presented = true
            } catch { library.fail(error, title: L("library.error_import_failed")) }
        }
    }
}
struct NativeParityUITestHost:View {
    @StateObject private var library:PreviewLibrary
    @State private var annotations:ReadingAnnotations?
    init(){let root=FileManager.default.urls(for:.cachesDirectory,in:.userDomainMask)[0].appendingPathComponent("parity-ui-"+UUID().uuidString);_library=StateObject(wrappedValue:PreviewLibrary(rootOverride:root))}
    var body:some View {
        NavigationStack{List{
            NavigationLink("Services"){NativeServiceSettings()}.accessibilityIdentifier("parity-services")
            NavigationLink("Translate"){NativeServiceResult(client:library.client,text:"Synthetic selected text 中文",kind:"translate")}.accessibilityIdentifier("parity-translate")
            NavigationLink("Invalid service"){NativeServiceResult(client:library.client,text:"Synthetic selected text",kind:"translate",endpointOverride:"file:///test-only")}.accessibilityIdentifier("parity-invalid-service")
            if let annotations {NavigationLink("Samples"){NativeContributionView(annotations:annotations)}.accessibilityIdentifier("parity-samples")}
        }}.task{
            ReaderI18n.shared.setLanguage("en")
            UserDefaults.standard.removeObject(forKey:"reader.service.translate.url")
            UserDefaults.standard.removeObject(forKey:"reader.service.dictionary.url")
            let url=URL(fileURLWithPath:library.client.root).appendingPathComponent("contribution.txt")
            do{
                try "第一章 测试\n原文用于合成样例，不会提交。".write(to:url,atomically:true,encoding:.utf8)
                let client=library.client
                let _:TxtImportResult=try await Task.detached{try client.call("txt_convert",["path":url.path])}.value
                await library.reload()
                guard let entry=library.books.first,let book=await library.open(entry),let block=try book.chapter(at:0).blocks.firstIndex(where:{$0.type=="Paragraph" && ($0.spans?.map(\.text).joined() ?? "").hasPrefix("原文")}) else{return}
                let value=ReadingAnnotations(library:library,book:book)
                _=await value.perform(["action":"correction","chapter":0,"block":block,"char_offset":0,"original":"原","corrected":"新"])
                annotations=value
            }catch{library.fail(error,title:L("common.error_operation_failed"))}
        }
    }
}
#endif
