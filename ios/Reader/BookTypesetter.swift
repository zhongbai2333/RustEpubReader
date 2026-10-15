import UIKit
import ImageIO
import CoreText

private final class ImageMetrics {
    let size: CGSize
    init(_ size: CGSize) { self.size = size }
}
// Attachments retain dimensions and source; decoded pixels live in a bounded cache.
final class NativeIllustration {
    let encoded: String
    let size: CGSize
    fileprivate var failed = false // Accessed under BookTypesetter.pendingLock.
    init(encoded: String, size: CGSize) { self.encoded = encoded; self.size = size }
}
enum BookTypesetter {
    private static let illustrationCache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 64 * 1024 * 1024
        cache.countLimit = 16
        return cache
    }()
    private static let failedIllustrations: NSCache<NSString, NSNumber> = {
        let cache = NSCache<NSString, NSNumber>(); cache.totalCostLimit = 2 * 1024 * 1024; cache.countLimit = 16; return cache
    }()
    static let illustrationReady = Notification.Name("ReaderIllustrationReady")
    private static let decodeQueue = DispatchQueue(label: "reader.illustrations", qos: .userInitiated)
    private static let pendingLock = NSLock()
    private static var pendingIllustrations = Set<String>()
    private static let illustrationMetadata: NSCache<NSString, NativeIllustration> = {
        let cache = NSCache<NSString, NativeIllustration>(); cache.totalCostLimit = 8 * 1024 * 1024; return cache
    }()
    static func isIllustration(_ value: Any?) -> Bool { value is UIImage || value is NativeIllustration }
    static func prepareImages(_ chapters: [BookChapter]) {
        for chapter in chapters { for block in chapter.blocks {
            if let encoded = block.data { _ = cachedIllustration(encoded) }
        } }
    }
    static let listMarkerKey=NSAttributedString.Key("ReaderListMarker")
    static let noteKey = NSAttributedString.Key("ReaderNote")
    static let actionKey = NSAttributedString.Key("ReaderAction")
    static let imageDescriptionKey=NSAttributedString.Key("ReaderImageDescription")
    static let imageKey = NSAttributedString.Key("ReaderImage")
    static func chapters(_ chapters: [BookChapter], options: ReaderOptions, rect: CGRect) -> [LaidOutChapter] {
        let name = options.fontName.isEmpty ? (options.cjkFontName ?? "") : options.fontName
        let font = UIFont(name: name, size: options.fontSize) ?? .systemFont(ofSize: options.fontSize)
        return chapters.map { layoutChapter($0, font: font, options: options, rect: rect) }
    }
    private static func layoutChapter(_ chapter: BookChapter, font: UIFont, options: ReaderOptions, rect: CGRect) -> LaidOutChapter {
        let text = NSMutableAttributedString(string: "")
        var coordinates: [BlockCoordinate] = []
        for (index, block) in chapter.blocks.enumerated() {
            let native=block.native_style
            if native?.hidden == true {continue}
            let source = (block.spans ?? []).map(\.text).joined()
            coordinates.append(.init(block: block.core_block ?? index, start: text.length, source: source, base: block.core_char_offset ?? 0))
            let paragraph = NSMutableParagraphStyle()
            paragraph.paragraphSpacing = font.pointSize * (native?.after ?? options.paragraphSpacing)
            paragraph.paragraphSpacingBefore = font.pointSize * (native?.before ?? 0)
            paragraph.alignment = options.justified ? .justified : .left
            let nativeFont = min(96,max(6,native?.size_px ?? font.pointSize * (native?.size ?? 1)))
            let left = min(rect.width*0.4,(native?.inset_left?.resolve(nativeFont,rect.width) ?? 0)+(native?.padding_left?.resolve(nativeFont,rect.width) ?? 0))
            let right = min(rect.width*0.4,(native?.inset_right?.resolve(nativeFont,rect.width) ?? 0)+(native?.padding_right?.resolve(nativeFont,rect.width) ?? 0))
            paragraph.headIndent = left; paragraph.tailIndent = -right
            paragraph.paragraphSpacingBefore += (native?.margin_top?.resolve(nativeFont,rect.width) ?? 0)+(native?.padding_top?.resolve(nativeFont,rect.width) ?? 0)
            paragraph.paragraphSpacing += (native?.margin_bottom?.resolve(nativeFont,rect.width) ?? 0)+(native?.padding_bottom?.resolve(nativeFont,rect.width) ?? 0)
            let blockStart=text.length
            if let items=native?.list,!items.isEmpty {
                coordinates.removeLast()
                for item in items {
                    let lineParagraph=paragraph.mutableCopy() as! NSMutableParagraphStyle
                    lineParagraph.headIndent=max(nativeFont*1.2,left)+nativeFont*1.2*Double(max(0,item.depth-(native?.list_depth ?? 1)))
                    lineParagraph.firstLineHeadIndent=lineParagraph.headIndent;lineParagraph.minimumLineHeight=nativeFont*(native?.line ?? options.lineHeight);lineParagraph.maximumLineHeight=lineParagraph.minimumLineHeight;lineParagraph.paragraphSpacing=nativeFont*0.3
                    coordinates.append(.init(block:block.core_block ?? index,start:text.length,source:item.text,base:(block.core_char_offset ?? 0)+item.start))
                    text.append(NSAttributedString(string:item.text,attributes:[.font:font.withSize(nativeFont),.foregroundColor:options.ink,.paragraphStyle:lineParagraph,listMarkerKey:item.marker]))
                    // Keep the marker on the first glyph only, so wrapping does not repeat it.
                    if item.text.utf16.count>1 {text.removeAttribute(listMarkerKey,range:NSRange(location:text.length-item.text.utf16.count+1,length:item.text.utf16.count-1))}
                    text.append(NSAttributedString(string:"\n",attributes:[.font:font,.paragraphStyle:lineParagraph]))
                }
                continue
            }
            switch block.type {
            case "Paragraph", "Heading":
                let heading = block.type == "Heading"
                let size: CGFloat
                if let absolute=native?.size_px {size=min(96,max(6,absolute))}
                else if let scale = native?.size { size = min(96, max(6, font.pointSize * CGFloat(scale))) }
                else { size = heading ? font.pointSize * CGFloat(max(1.05, 1.5 - Double(block.level ?? 1) * 0.08)) : font.pointSize }
                paragraph.minimumLineHeight = size * max((native?.line ?? options.lineHeight) * (native?.inline ?? []).filter {!$0.hidden}.compactMap(\.size).reduce(1,max), (native?.ruby ?? []).isEmpty ? 1 : 1.7)
                paragraph.maximumLineHeight = paragraph.minimumLineHeight
                paragraph.firstLineHeadIndent = left + (heading ? 0 : (native?.indent_length?.resolve(size,rect.width) ?? size * (native?.indent ?? options.firstLineIndent)))
                if heading { paragraph.alignment = .center; if native?.margin_bottom == nil { paragraph.paragraphSpacing = font.pointSize * (native?.after ?? (24/font.pointSize)) } }
                if let align=native?.align {paragraph.alignment=align=="center" ? .center : align=="right" ? .right : align=="justify" ? .justified : .left}
                if let table=native?.table, let image=tableImage(table,font:font.withSize(size),ink:options.ink,width:rect.width-left-right,maxHeight:rect.height-32) {
                    paragraph.alignment = .left; paragraph.firstLineHeadIndent=left; paragraph.paragraphSpacing=0;paragraph.paragraphSpacingBefore=0
                    coordinates[coordinates.count-1].renderLength=1
                    appendImage(image,size:image.size,paragraph:paragraph,to:text,description:source)
                    break
                }
                let spans=native?.text.map {[BookSpan(text:$0,style:"Normal",link_url:nil)]} ?? block.spans ?? []
                for span in spans {
                    var traits: UIFontDescriptor.SymbolicTraits = []
                    if native?.bold == true || heading || span.style == "Bold" || span.style == "BoldItalic" { traits.insert(.traitBold) }
                    if native?.italic == true || span.style == "Italic" || span.style == "BoldItalic" { traits.insert(.traitItalic) }
                    let family=native?.family
                        let name=family=="serif" ? "TimesNewRomanPSMT" : family=="sans-serif" ? "Helvetica" : family=="monospace" ? "Menlo-Regular" : family
                        let base=name.flatMap {UIFont(name:$0,size:size)} ?? font.withSize(size)
                    let styled = base.fontDescriptor.withSymbolicTraits(traits).map { UIFont(descriptor: $0, size: size) } ?? base
                    var attrs: [NSAttributedString.Key: Any] = [.font: styled, .foregroundColor: (!options.darkAppearance ? native?.color.map {UIColor(hex:$0.replacingOccurrences(of:"#",with:""))} : nil) ?? options.ink, .paragraphStyle: paragraph]
                    if let action=native?.action {attrs[actionKey]=action;attrs[.underlineStyle]=NSUnderlineStyle.single.rawValue;attrs[.foregroundColor]=UIColor.systemBlue}
                    if span.link_url != nil { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
                    text.append(NSAttributedString(string: span.text, attributes: attrs))
                }
                applyInline(native,source:source,start:blockStart,size:size,ink:options.ink,dark:options.darkAppearance,to:text)
            case "Image":
                paragraph.alignment = .center
                if let encoded = block.data, let image = illustrationAsset(encoded), image.size.width > 0, image.size.height > 0 {
                    let boundHeight=max(40,rect.height-font.pointSize*options.lineHeight*1.5-64)
                    let size=imageSize(image.size,native:native,font:nativeFont,width:rect.width-left-right,height:boundHeight)
                    appendImage(image,size:size,paragraph:paragraph,to:text,description:block.alt)
                } else {
                    text.append(NSAttributedString(string: block.alt ?? "[Image]", attributes: [.font: font, .foregroundColor: options.ink, .paragraphStyle: paragraph]))
                }
            case "Separator":
                paragraph.alignment = .center
                text.append(NSAttributedString(string: "—", attributes: [.font: font, .foregroundColor: options.ink, .paragraphStyle: paragraph]))
            default: break
            }
            text.append(NSAttributedString(string: "\n", attributes: [.font: font, .foregroundColor: options.ink, .paragraphStyle: paragraph]))
        }
        if text.length == 0 {
            coordinates.append(.init(block: 0, start: 0, source: ""))
            text.append(NSAttributedString(string: chapter.title + "\n", attributes: [.font: font, .foregroundColor: options.ink]))
        }
        applyChineseFont(to: text, name: options.cjkFontName ?? options.fontName)
        return LaidOutChapter(text: text, coordinates: coordinates)
    }
    private static func applyInline(_ style:NativeStyle?,source:String,start:Int,size:CGFloat,ink:UIColor,dark:Bool,to text:NSMutableAttributedString) {
        var offsets=[0];for scalar in source.unicodeScalars {offsets.append(offsets.last!+String(scalar).utf16.count)}
        func range(_ begin:Int,_ end:Int)->NSRange? {guard begin>=0,end>begin,end<offsets.count else{return nil};let r=NSRange(location:start+offsets[begin],length:offsets[end]-offsets[begin]);return NSMaxRange(r)<=text.length ? r:nil}
        for item in style?.inline ?? [] {
            guard let r=range(item.start,item.end) else {continue}
            var updates:[(NSRange,UIFont)]=[]
            text.enumerateAttribute(.font,in:r) {value,run,_ in
                let old=(value as? UIFont) ?? .systemFont(ofSize:size)
                var traits=old.fontDescriptor.symbolicTraits
                if let bold=item.bold {if bold{traits.insert(.traitBold)}else{traits.remove(.traitBold)}}
                if let italic=item.italic {if italic{traits.insert(.traitItalic)}else{traits.remove(.traitItalic)}}
                let pointSize=item.hidden ? 0.01 : size*(item.size ?? 1)
                let family=item.family
                let name=family=="serif" ? "TimesNewRomanPSMT" : family=="sans-serif" ? "Helvetica" : family=="monospace" ? "Menlo-Regular" : family
                let base=name.flatMap {UIFont(name:$0,size:pointSize)} ?? old.withSize(pointSize)
                var descriptor=base.fontDescriptor.withSymbolicTraits(traits) ?? base.fontDescriptor
                descriptor=descriptor.addingAttributes([.featureSettings:[[UIFontDescriptor.FeatureKey.type:kLowerCaseType,UIFontDescriptor.FeatureKey.selector:item.small_caps ? kLowerCaseSmallCapsSelector : kDefaultLowerCaseSelector]]])
                let styled=UIFont(descriptor:descriptor,size:pointSize)
                updates.append((run,styled))
            }
            for(run,font) in updates {text.addAttribute(.font,value:font,range:run)}
            if item.hidden {text.addAttribute(.foregroundColor,value:UIColor.clear,range:r);text.removeAttribute(.underlineStyle,range:r)}
            else if !dark,let color=item.color {text.addAttribute(.foregroundColor,value:UIColor(hex:color.replacingOccurrences(of:"#",with:"")),range:r)}
            if item.underline {text.addAttribute(.underlineStyle,value:NSUnderlineStyle.single.rawValue,range:r)}
            if item.strike {text.addAttribute(.strikethroughStyle,value:NSUnderlineStyle.single.rawValue,range:r)}
            if item.baseline != 0 {text.addAttribute(.baselineOffset,value:item.baseline*size,range:r)}
            if let href=item.note {text.addAttribute(noteKey,value:href,range:r);text.addAttribute(.foregroundColor,value:UIColor.systemBlue,range:r);text.addAttribute(.underlineStyle,value:NSUnderlineStyle.single.rawValue,range:r)}
        }
        for ruby in style?.ruby ?? [] {guard let r=range(ruby.start,ruby.end) else{continue};let retained=Unmanaged.passRetained(ruby.text as CFString);var values:[Unmanaged<CFString>?]=[retained,nil,nil,nil];let annotation=CTRubyAnnotationCreate(.center,.auto,0.5,&values);retained.release();text.addAttribute(NSAttributedString.Key(kCTRubyAnnotationAttributeName as String),value:annotation,range:r)}
    }
    private static func imageSize(_ natural:CGSize,native:NativeStyle?,font:Double,width:Double,height:Double)->CGSize {
        let desiredWidth=native?.image_width?.resolve(font,width) ?? natural.width
        let desiredHeight=native?.image_height?.resolve(font,height) ?? natural.height
        let request:Double
        if native?.image_width != nil && native?.image_height != nil {request=min(desiredWidth/natural.width,desiredHeight/natural.height)}
        else if native?.image_width != nil {request=desiredWidth/natural.width}
        else if native?.image_height != nil {request=desiredHeight/natural.height}
        else {request=1}
        let scale=max(0,min(request,min(min(width,native?.image_max_width?.resolve(font,width) ?? width)/natural.width,min(height,native?.image_max_height?.resolve(font,height) ?? height)/natural.height)))
        return CGSize(width:natural.width*scale,height:natural.height*scale)
    }
    private static func appendImage(_ image:Any,size:CGSize,paragraph:NSParagraphStyle,to text:NSMutableAttributedString,description:String?=nil) {
        let attachmentParagraph=paragraph.mutableCopy() as! NSMutableParagraphStyle
        attachmentParagraph.minimumLineHeight=0;attachmentParagraph.maximumLineHeight=0
        var callbacks=CTRunDelegateCallbacks(version:kCTRunDelegateVersion1,dealloc:{Unmanaged<ImageMetrics>.fromOpaque($0).release()},getAscent:{Unmanaged<ImageMetrics>.fromOpaque($0).takeUnretainedValue().size.height},getDescent:{_ in 0},getWidth:{Unmanaged<ImageMetrics>.fromOpaque($0).takeUnretainedValue().size.width})
        let pointer=Unmanaged.passRetained(ImageMetrics(size)).toOpaque()
        if let delegate=CTRunDelegateCreate(&callbacks,pointer){text.append(NSAttributedString(string:"\u{fffc}",attributes:[NSAttributedString.Key(kCTRunDelegateAttributeName as String):delegate,imageKey:image,imageDescriptionKey:description ?? "",.paragraphStyle:attachmentParagraph,.foregroundColor:UIColor.clear]))}
        else {Unmanaged<ImageMetrics>.fromOpaque(pointer).release()}
    }
    // CoreText draws cells into one native row attachment; oversized rows retain
    // their complete original text so pagination can split without losing content.
    private static func tableImage(_ table:NativeTable,font:UIFont,ink:UIColor,width:CGFloat,maxHeight:CGFloat)->UIImage? {
        guard width>80,table.columns>0,!table.cells.isEmpty else{return nil}
        let column=width/CGFloat(table.columns)
        let cells=table.cells.map {cell -> (NativeCell,NSAttributedString,CGFloat) in
            let paragraph=NSMutableParagraphStyle();paragraph.alignment=cell.align=="right" ? .right : cell.align=="center" ? .center : .left;paragraph.minimumLineHeight=font.pointSize*1.5
            let styled=cell.header ? font.fontDescriptor.withSymbolicTraits(.traitBold).map {UIFont(descriptor:$0,size:font.pointSize)} ?? font : font
            let text=NSAttributedString(string:cell.text,attributes:[.font:styled,.foregroundColor:ink,.paragraphStyle:paragraph])
            let bounds=text.boundingRect(with:CGSize(width:max(16,column*CGFloat(cell.span)-12),height:100000),options:[.usesLineFragmentOrigin,.usesFontLeading],context:nil)
            return(cell,text,ceil(bounds.height)+12)
        }
        let height=cells.map {$0.2}.max() ?? font.pointSize*1.5+12
        guard height<=maxHeight else{return nil}
        let format=UIGraphicsImageRendererFormat();format.opaque=false;format.scale=UIScreen.main.scale
        return UIGraphicsImageRenderer(size:CGSize(width:width,height:height),format:format).image {renderer in
            var x:CGFloat=0
            for(cell,text,_) in cells {let w=column*CGFloat(cell.span);let rect=CGRect(x:x,y:0,width:w,height:height)
                if cell.header {ink.withAlphaComponent(0.06).setFill();renderer.cgContext.fill(rect)}
                ink.withAlphaComponent(0.25).setStroke();renderer.cgContext.setLineWidth(0.5);renderer.cgContext.stroke(rect.insetBy(dx:0.25,dy:0.25));text.draw(with:rect.insetBy(dx:6,dy:6),options:[.usesLineFragmentOrigin,.usesFontLeading],context:nil);x+=w
            }
        }
    }
    // Change attributes only: UTF-16 ranges and Rust scalar anchors stay intact.
    private static func applyChineseFont(to text: NSMutableAttributedString, name: String) {
        var offset = 0
        var ranges: [NSRange] = []
        for character in text.string {
            let length = String(character).utf16.count
            let chinese = character.unicodeScalars.contains { scalar in
                switch scalar.value {
                case 0x2e80...0x9fff, 0xf900...0xfaff, 0x2018...0x201f,
                     0xfe10...0xfe1f, 0xfe30...0xfe4f, 0xff01...0xff65,
                     0x20000...0x323af: return true
                default: return false
                }
            }
            if chinese {
                if let last = ranges.last, NSMaxRange(last) == offset {
                    ranges[ranges.count - 1].length += length
                } else { ranges.append(NSRange(location: offset, length: length)) }
            }
            offset += length
        }
        var styledFonts: [String: UIFont] = [:]
        for range in ranges {
            var updates: [(NSRange, UIFont)] = []
            text.enumerateAttribute(.font, in: range) { value, run, _ in
                guard let old = value as? UIFont else { return }
                let traits = old.fontDescriptor.symbolicTraits.intersection([.traitBold, .traitItalic])
                let key = "\(old.pointSize)/\(traits.rawValue)"
                let styled: UIFont
                if let cached = styledFonts[key] { styled = cached }
                else {
                    let base = UIFont(name: name, size: old.pointSize)
                        ?? UIFont(name: "PingFangSC-Regular", size: old.pointSize)
                        ?? .systemFont(ofSize: old.pointSize)
                    styled = base.fontDescriptor.withSymbolicTraits(traits)
                        .map { UIFont(descriptor: $0, size: old.pointSize) } ?? base
                    styledFonts[key] = styled
                }
                updates.append((run, styled))
            }
            for (run, font) in updates { text.addAttribute(.font, value: font, range: run) }
        }
    }
    private static func illustrationAsset(_ encoded: String) -> NativeIllustration? {
        let key = encoded as NSString
        if let asset = illustrationMetadata.object(forKey: key) { return asset }
        guard let bytes = Data(base64Encoded: encoded), let source = CGImageSourceCreateWithData(bytes as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let height = properties[kCGImagePropertyPixelHeight] as? NSNumber,
              width.doubleValue > 0, height.doubleValue > 0 else { return nil }
        let orientation = (properties[kCGImagePropertyOrientation] as? NSNumber)?.intValue ?? 1
        let size = (5...8).contains(orientation) ? CGSize(width: height.doubleValue, height: width.doubleValue) : CGSize(width: width.doubleValue, height: height.doubleValue)
        let asset = NativeIllustration(encoded: encoded, size: size)
        illustrationMetadata.setObject(asset, forKey: key, cost: encoded.utf8.count)
        return asset
    }
    private static func readyIllustration(_ asset: NativeIllustration) -> UIImage? {
        if let image = illustrationCache.object(forKey: asset.encoded as NSString) { return image }
        pendingLock.lock()
        if asset.failed { pendingLock.unlock(); return nil }
        if failedIllustrations.object(forKey: asset.encoded as NSString) != nil { asset.failed=true; pendingLock.unlock(); return nil }
        let inserted: Bool
        if pendingIllustrations.count < 8 { inserted = pendingIllustrations.insert(asset.encoded).inserted }
        else { inserted = false }
        pendingLock.unlock()
        if inserted {
            decodeQueue.async {
                let failed=cachedIllustration(asset.encoded) == nil
                if failed {
                    failedIllustrations.setObject(true, forKey: asset.encoded as NSString, cost: asset.encoded.utf8.count)
                }
                pendingLock.lock(); asset.failed=failed; pendingIllustrations.remove(asset.encoded); pendingLock.unlock()
                DispatchQueue.main.async { NotificationCenter.default.post(name: illustrationReady, object: nil) }
            }
        }
        return nil
    }

    private static func cachedIllustration(_ encoded: String) -> UIImage? {
        let key = encoded as NSString
        if let image = illustrationCache.object(forKey: key) { return image }
        guard let bytes = Data(base64Encoded: encoded), let image = decodedIllustration(bytes, maxSize: 2048) else { return nil }
        let pixelBytes = image.cgImage.map { $0.bytesPerRow * $0.height } ?? 0
        illustrationCache.setObject(image, forKey: key, cost: pixelBytes + encoded.utf8.count)
        return image
    }

    // Thumbnail decoding bounds memory for large scans and applies EXIF orientation.
    private static func decodedIllustration(_ data: Data, maxSize: CGFloat) -> UIImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: min(4096, max(1, maxSize)),
                kCGImageSourceShouldCacheImmediately: true
              ] as CFDictionary) else { return nil }
        return UIImage(cgImage: image)
    }

    static func accessibilityText(_ text:NSAttributedString)->String {
        var output=""
        text.enumerateAttributes(in:NSRange(location:0,length:text.length)) {attrs,range,_ in
            if let description=attrs[imageDescriptionKey] as? String {output+=description}
            else {output+=text.attributedSubstring(from:range).string}
        }
        return output
    }
    @discardableResult static func prefetch(_ page: ReadingPage) -> Bool {
        var ready=true
        let frame = CTFramesetterCreateFrame(page.framesetter ?? CTFramesetterCreateWithAttributedString(page.text), page.range,
            CGPath(rect: page.textRect, transform: nil), nil)
        for line in CTFrameGetLines(frame) as! [CTLine] {
            for run in CTLineGetGlyphRuns(line) as! [CTRun] {
                let attrs = CTRunGetAttributes(run) as NSDictionary
                if let asset = attrs[imageKey] as? NativeIllustration {
                    let image=readyIllustration(asset)
                    pendingLock.lock();let failed=asset.failed;pendingLock.unlock()
                    if image==nil && !failed {ready=false}
                }
            }
        }
        return ready
    }
    static func drawImages(frame: CTFrame, context: CGContext) {
        let lines = CTFrameGetLines(frame) as! [CTLine]
        var origins = Array(repeating: CGPoint.zero, count: lines.count)
        CTFrameGetLineOrigins(frame, CFRange(location: 0, length: 0), &origins)
        let origin = CTFrameGetPath(frame).boundingBox.origin
        for (index, line) in lines.enumerated() {
            for run in CTLineGetGlyphRuns(line) as! [CTRun] {
                let attrs = CTRunGetAttributes(run) as NSDictionary
                if let marker=attrs[listMarkerKey] as? String,let font=attrs[NSAttributedString.Key.font] as? UIFont {
                    let label=CTLineCreateWithAttributedString(NSAttributedString(string:marker,attributes:[.font:font,.foregroundColor:attrs[NSAttributedString.Key.foregroundColor] ?? UIColor.label]))
                    let x=CTLineGetOffsetForStringIndex(line,CTRunGetStringRange(run).location,nil)
                    context.textPosition=CGPoint(x:origin.x+origins[index].x+x-font.pointSize*1.1,y:origin.y+origins[index].y);CTLineDraw(label,context)
                }
                let image = (attrs[imageKey] as? UIImage) ?? (attrs[imageKey] as? NativeIllustration).flatMap { readyIllustration($0) }
                guard let cgImage = image?.cgImage else { continue }
                var ascent: CGFloat = 0, descent: CGFloat = 0
                let width = CTRunGetTypographicBounds(run, CFRange(location: 0, length: 0), &ascent, &descent, nil)
                let x = CTLineGetOffsetForStringIndex(line, CTRunGetStringRange(run).location, nil)
                context.draw(cgImage, in: CGRect(x: origin.x + origins[index].x + x, y: origin.y + origins[index].y - descent, width: width, height: ascent + descent))
            }
        }
    }
}
