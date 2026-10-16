#if DEBUG
import Foundation
import UIKit
import CoreText

enum NativePresentationRuntimeCheck {
    @MainActor static func run() {
        Task { @MainActor in
            let files=FileManager.default
            let documents=files.urls(for:.documentDirectory,in:.userDomainMask)[0]
            let caches=files.urls(for:.cachesDirectory,in:.userDomainMask)[0]
            var result:[String:Any]=[:]
            do {
                let root=files.urls(for:.applicationSupportDirectory,in:.userDomainMask)[0].appendingPathComponent("ReaderLibrary")
                let client=CoreClient(root:root.path,device:"native-layout-check")
                let entry:LibraryEntry=try client.call("import",["path":documents.appendingPathComponent("original-layout.epub").path])
                let book:PreviewBook=try client.call("open",["id":entry.id])
                let original=book.chapters[0]
                let styled=try requireIndex(original.blocks.firstIndex {($0.spans ?? []).map(\.text).joined().contains("这是出版社指定")})
                let revealed=try requireIndex(original.blocks.firstIndex {($0.spans ?? []).map(\.text).joined()=="脚本控制的展开内容"})
                func present(_ mode:String,_ enabled:Bool,_ events:[[String:Int]]=[]) throws -> NativePresentation {
                    try client.call("native_presentation",["id":entry.id,"chapter":0,"policy":mode,"scripts":enabled,"events":events])
                }
                for mode in ["none","mixed","publisher"] {
                    let initial=try present(mode,true)
                    try require(initial.errors.isEmpty,"Script startup failed: \(initial.errors)")
                    try require(initial.styles[revealed].hidden,"Hidden paragraph is visible")
                    try require(initial.styles[styled].align==(mode=="none" ? nil:"right"),"Style priority failed")
                    let action=try requireIndex(initial.styles.compactMap(\.action).first)
                    let opened=try present(mode,true,[["node":action]])
                    try require(opened.errors.isEmpty && !opened.styles[revealed].hidden,"Click failed")
                    let stopped=try present(mode,false,[["node":action]])
                    try require(stopped.styles[revealed].hidden && stopped.styles.allSatisfy {$0.action==nil},"Disabled scripts ran")
                    let chapters=book.chapters.enumerated().map { index,chapter in
                        BookChapter(title:chapter.title,blocks:chapter.blocks.map {block in
                            var copy=block
                            if index==0,let source=block.core_block,opened.styles.indices.contains(source){copy.native_style=opened.styles[source]}
                            return copy
                        })
                    }
                    for flow in ReadingMode.allCases {
                        let options=ReaderOptions(fontSize:18,lineHeight:1.5,fontName:"Helvetica",theme:"light",sliding:flow == .slide,cjkFontName:"PingFangSC-Regular",readingMode:flow)
                        let content=BookTypesetter.chapters(chapters,options:options,rect:CGRect(x:24,y:80,width:354,height:694))
                        let pages=ReadingLayout.paginate(content,in:CGRect(x:24,y:80,width:354,height:694))
                        try require(!pages.isEmpty && content[0].text.string.contains("脚本控制的展开内容"),"Native layout missing script result")
                        let source=content[0].coordinates.first {$0.block==original.blocks[revealed].core_block}
                        try require(source?.source=="脚本控制的展开内容","Source coordinates changed")
                        if let range=content[0].text.string.range(of:"这是出版社指定"),mode != "none" {
                            let at=NSRange(range,in:content[0].text.string).location
                            let style=content[0].text.attribute(.paragraphStyle,at:at,effectiveRange:nil) as? NSParagraphStyle
                            try require(style?.alignment == .right,"CoreText ignored publication alignment")
                            try require(abs((style?.minimumLineHeight ?? 0)-(opened.styles[styled].size_px ?? 18*(opened.styles[styled].size ?? 1))*(mode=="publisher" ? 1.9:1.5))<0.1,"CoreText ignored line policy")
                        }
                    }
                }
                let typography:LibraryEntry=try client.call("import",["path":documents.appendingPathComponent("native-typography.epub").path])
                let typographyBook:PreviewBook=try client.call("open",["id":typography.id])
                var typographyChecks=0
                for mode in ["none","mixed","publisher"] {
                    let presentation:NativePresentation=try client.call("native_presentation",["id":typography.id,"chapter":0,"policy":mode,"scripts":false,"events":[]])
                    let ch=BookChapter(title:typographyBook.chapters[0].title,blocks:typographyBook.chapters[0].blocks.map {block in var value=block;value.native_style=presentation.styles[block.core_block ?? 0];return value})
                    for width:CGFloat in [320,640] {
                        let rect=CGRect(x:0,y:0,width:width,height:650)
                        let options=ReaderOptions(fontSize:18,lineHeight:1.5,fontName:"Helvetica",theme:"light",sliding:false,cjkFontName:"PingFangSC-Regular")
                        BookTypesetter.prepareImages([ch])
                        let content=BookTypesetter.chapters([ch],options:options,rect:rect)[0]
                        let pages=ReadingLayout.paginate([content],in:rect)
                        try require(!pages.isEmpty,"Typography pages missing")
                        var rubyCount=0,noteCount=0,imageCount=0,baselineCount=0
                        content.text.enumerateAttributes(in:NSRange(location:0,length:content.text.length)) {attrs,_,_ in
                            if attrs[NSAttributedString.Key(kCTRubyAnnotationAttributeName as String)] != nil {rubyCount+=1}
                            if attrs[BookTypesetter.noteKey] != nil {noteCount+=1}
                            if BookTypesetter.isIllustration(attrs[BookTypesetter.imageKey]) {imageCount+=1}
                            if attrs[.baselineOffset] != nil {baselineCount+=1}
                        }
                        try require(rubyCount>=3&&noteCount>=1&&imageCount>=5&&baselineCount>=2,"Missing native typography attributes: ruby=\(rubyCount), note=\(noteCount), image=\(imageCount), baseline=\(baselineCount)")
                        let scalarSource=content.coordinates.first {$0.source.contains("文字 😀 中文")}
                        try require(scalarSource != nil,"Canonical inline source changed")
                        var consumed=0
                        for page in pages {try require(page.range.location==consumed&&page.range.length>0,"Pagination lost typography text");consumed+=page.range.length
                            guard let context=CGContext(data:nil,width:Int(width),height:650,bitsPerComponent:8,bytesPerRow:Int(width)*4,space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue)else{throw CoreFailure(message:"No drawing context")}
                            let frame=CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(content.text),page.range,CGPath(rect:rect,transform:nil),nil)
                            try checkLines(frame);CTFrameDraw(frame,context);BookTypesetter.drawImages(frame:frame,context:context)
                        }
                        try require(consumed==content.text.length,"Unpaginated typography tail")
                        typographyChecks+=1
                    }
                }
                let note:NativeNote=try client.call("native_note",["id":typography.id,"chapter":0,"href":"notes.xhtml#n%E4%B8%80"])
                try require(note.text.contains("这是跨章节的注释原文"),"Note resolution failed")
                let linkChecks=try checkBodyLinks(client:client,book:typographyBook)
                var corpusChecks:[[String:Any]]=[]
                for (name,chapterIndex) in [("pride-and-prejudice",1),("alice-standard",2),("accessible-epub-3",18),("hefty-water",0),("kusamakura",2),("hongloumeng",1),("aq",1),("zhaohuaxishi",1),("tiangongkaiwu",1),("pagination-stress",0)] {
                    let path=documents.appendingPathComponent(name+".epub")
                    if !files.fileExists(atPath:path.path){continue}
                    let entry:LibraryEntry=try client.call("import",["path":path.path])
                    let realBook:PreviewBook=try client.call("open",["id":entry.id])
                    let realStyle:NativePresentation=try client.call("native_presentation",["id":entry.id,"chapter":chapterIndex,"policy":"mixed","scripts":false,"events":[]])
                    try require(realStyle.errors.isEmpty,"Real book presentation failed: \(name)")
                    let source=realBook.chapters[chapterIndex]
                    let ch=BookChapter(title:source.title,blocks:source.blocks.map {block in var value=block;let at=block.core_block ?? 0;if realStyle.styles.indices.contains(at){value.native_style=realStyle.styles[at]};return value})
                    for width:CGFloat in [320,640] {
                        let rect=CGRect(x:0,y:0,width:width,height:650)
                        let options=ReaderOptions(fontSize:18,lineHeight:1.5,fontName:"Helvetica",theme:"light",sliding:false,cjkFontName:"PingFangSC-Regular")
                        BookTypesetter.prepareImages([ch])
                        let content=BookTypesetter.chapters([ch],options:options,rect:rect)[0]
                        let pages=ReadingLayout.paginate([content],in:rect)
                        try require(!pages.isEmpty,"No real book pages: \(name)")
                        var consumed=0
                        for page in pages {try require(page.range.location==consumed&&page.range.length>0,"Real book lost content: \(name)");consumed+=page.range.length}
                        try require(consumed==content.text.length,"Real book tail missing: \(name)")
                        // Draw actual CoreText and native attachments on representative pages.
                        for page in pages.prefix(3) {
                            guard let context=CGContext(data:nil,width:Int(width),height:650,bitsPerComponent:8,bytesPerRow:Int(width)*4,space:CGColorSpaceCreateDeviceRGB(),bitmapInfo:CGImageAlphaInfo.premultipliedLast.rawValue)else{throw CoreFailure(message:"No corpus context")}
                            let frame=CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(content.text),page.range,CGPath(rect:rect,transform:nil),nil)
                            try checkLines(frame);CTFrameDraw(frame,context);BookTypesetter.drawImages(frame:frame,context:context)
                        }
                        corpusChecks.append(["name":name,"chapter":chapterIndex,"width":width,"pages":pages.count,"drawnPages":min(3,pages.count),"contiguousCoverage":true])
                    }
                }
                result=["passed":true,"styles":3,"flows":3,"scriptSwitch":true,"stableCoordinates":true,"typographyLayouts":typographyChecks,"nativeNotes":true,"bodyLinkLayouts":linkChecks,"corpusLayouts":corpusChecks]
            }catch{result=["passed":false,"error":error.localizedDescription]}
            if let data=try? JSONSerialization.data(withJSONObject:result,options:[.prettyPrinted,.sortedKeys]) {try? data.write(to:caches.appendingPathComponent("native-verification.json"))}
        }
    }
    @MainActor private static func checkBodyLinks(client:CoreClient,book:PreviewBook) throws -> Int {
        let href="notes.xhtml#n%E4%B8%80",source="目录😀"
        let target:ReadingAnchor?=try client.call("native_link",["id":book.id,"chapter":0,"href":href])
        try require(target != nil && target?.chapter != 0,"Body-link bridge target missing")
        let options=ReaderOptions(fontSize:18,lineHeight:1.5,fontName:"Helvetica",theme:"light",sliding:false,cjkFontName:"PingFangSC-Regular")
        let rect=CGRect(x:20,y:200,width:300,height:130)
        for list in [false,true] {
            var style=NativeStyle(align:nil,size:nil,line:nil,indent:nil,before:nil,after:nil,color:nil,family:nil,bold:false,italic:false,hidden:false,action:nil,text:nil)
            if list {style.list=[NativeList(start:0,end:source.unicodeScalars.count,text:source,depth:1,marker:"1.")]}
            let block=BookBlock(native_style:style,type:"Paragraph",core_block:0,core_char_offset:0,level:nil,spans:[BookSpan(text:source,style:"Normal",link_url:href)],data:nil,alt:nil)
            let chapter=BookChapter(title:"Contents",blocks:[block])
            let laid=BookTypesetter.chapters([chapter],options:options,rect:rect)
            guard let page=ReadingLayout.paginate(laid,in:rect).first else{throw CoreFailure(message:"Body-link page missing")}
            let at=(page.text.string as NSString).range(of:source).location
            try require(at != NSNotFound && page.text.attribute(BookTypesetter.linkKey,at:at,effectiveRange:nil) as? String == href,"Link target lost during typesetting")
            let frame=CTFramesetterCreateFrame(CTFramesetterCreateWithAttributedString(page.text),page.range,CGPath(rect:rect,transform:nil),nil)
            let lines=CTFrameGetLines(frame) as! [CTLine]
            var origins=Array(repeating:CGPoint.zero,count:lines.count);CTFrameGetLineOrigins(frame,CFRange(location:0,length:0),&origins)
            guard let row=lines.firstIndex(where:{let r=CTLineGetStringRange($0);return at>=r.location && at<r.location+r.length}) else{throw CoreFailure(message:"Link line missing")}
            let point=CGPoint(x:rect.minX+origins[row].x+CTLineGetOffsetForStringIndex(lines[row],at,nil)+3,y:400-rect.minY-origins[row].y-3)
            let paper=PaperSurface(frame:CGRect(x:0,y:0,width:340,height:400));paper.page=page
            let controller=NativeReadingController(book:book,anchor:.init(chapter:0),options:options)
            var opened:String?,noted:String?
            controller.publicationLink={_,value in opened=value};controller.publicationNote={_,value in noted=value}
            try require(controller.activatePublication(at:point,in:paper) && opened==href,"Native body-link hit testing failed")
            let annotated=NSMutableAttributedString(attributedString:page.text)
            annotated.addAttribute(BookTypesetter.noteKey,value:href,range:NSRange(location:at,length:source.utf16.count))
            paper.page=ReadingPage(chapter:page.chapter,content:LaidOutChapter(text:annotated,coordinates:page.content.coordinates),range:page.range,textRect:page.textRect)
            try require(controller.activatePublication(at:point,in:paper) && noted==href,"Note did not take precedence over navigation")
            try require(!controller.activatePublication(at:CGPoint(x:0,y:0),in:paper),"Blank margin activated a link")
            if let target {
                for flow in ReadingMode.allCases {
                    var flowOptions=options;flowOptions.readingMode=flow;flowOptions.sliding=flow == .slide
                    let reader=NativeReadingController(book:book,anchor:.init(chapter:0),options:flowOptions)
                    reader.loadViewIfNeeded();reader.view.frame=CGRect(x:0,y:0,width:340,height:600);reader.view.setNeedsLayout();reader.view.layoutIfNeeded()
                    reader.publicationLink={ [weak reader] _,_ in reader?.update(options:flowOptions,jump:target,jumpID:1,annotations:.empty,revision:0)}
                    // Restore the ordinary link after testing footnote precedence.
                    paper.page=page
                    try require(reader.activatePublication(at:point,in:paper) && reader.anchor.chapter==target.chapter,"Link did not navigate in \(flow)")
                }
            }
        }
        return 2
    }
    private static func checkLines(_ frame:CTFrame) throws {
        let lines=CTFrameGetLines(frame) as! [CTLine];var origins=Array(repeating:CGPoint.zero,count:lines.count);CTFrameGetLineOrigins(frame,CFRange(location:0,length:0),&origins)
        var previousBottom=CGFloat.greatestFiniteMagnitude
        for(index,line)in lines.enumerated(){var ascent:CGFloat=0,descent:CGFloat=0;CTLineGetTypographicBounds(line,&ascent,&descent,nil);let top=origins[index].y+ascent
            try require(top<=previousBottom+2,"Native lines overlap: \(index) \(top) > \(previousBottom)");previousBottom=origins[index].y-descent}
    }
    private static func require(_ condition:Bool,_ message:String) throws {if !condition {throw CoreFailure(message:message)}}
    private static func requireIndex(_ value:Int?) throws -> Int {guard let value else {throw CoreFailure(message:"Fixture element missing")};return value}
}
#endif
