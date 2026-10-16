//! Native publication presentation. Canonical blocks are never renumbered.
pub use super::native_css::Dimension;
use super::native_css::{cascade, rules};
pub use super::native_typography::{InlinePresentation, ListLine, NativeNote, Ruby, TableRow};
use super::ContentBlock;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CssPolicy {
    None,
    #[default]
    Mixed,
    Publisher,
}
impl CssPolicy {
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Mixed => "mixed",
            Self::Publisher => "publisher",
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct NativeStyle {
    pub size_px: Option<f32>,
    pub inline: Vec<InlinePresentation>,
    pub ruby: Vec<Ruby>,
    pub table: Option<TableRow>,
    pub list: Vec<ListLine>,
    pub list_depth: u8,
    pub quote: bool,
    pub pre: bool,
    pub inset_left: Option<Dimension>,
    pub inset_right: Option<Dimension>,
    pub padding_left: Option<Dimension>,
    pub padding_right: Option<Dimension>,
    pub padding_top: Option<Dimension>,
    pub padding_bottom: Option<Dimension>,
    pub margin_top: Option<Dimension>,
    pub margin_bottom: Option<Dimension>,
    pub indent_length: Option<Dimension>,
    pub image_width: Option<Dimension>,
    pub image_height: Option<Dimension>,
    pub image_max_width: Option<Dimension>,
    pub image_max_height: Option<Dimension>,
    pub align: Option<String>,
    pub size: Option<f32>,
    pub line: Option<f32>,
    pub indent: Option<f32>,
    pub before: Option<f32>,
    pub after: Option<f32>,
    pub color: Option<String>,
    pub family: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub hidden: bool,
    pub action: Option<usize>,
    pub text: Option<String>,
}
impl NativeStyle {
    pub fn size_at(&self, base: f32) -> f32 {
        self.size_px
            .unwrap_or(base * self.size.unwrap_or(1.0))
            .clamp(6.0, 96.0)
    }
    pub fn image_size(
        &self,
        native_width: f32,
        native_height: f32,
        font: f32,
        width: f32,
        height: f32,
    ) -> (f32, f32) {
        let desired_width = self
            .image_width
            .as_ref()
            .map(|v| v.resolve(font, width))
            .unwrap_or(native_width);
        let image_height = self
            .image_height
            .as_ref()
            .filter(|v| height.is_finite() || v.px != 0. || v.em != 0.);
        let desired_height = image_height
            .map(|v| v.resolve(font, height))
            .unwrap_or(native_height);
        let max_width = self
            .image_max_width
            .as_ref()
            .map(|v| v.resolve(font, width))
            .unwrap_or(width)
            .min(width);
        let max_height = self
            .image_max_height
            .as_ref()
            .filter(|v| height.is_finite() || v.px != 0. || v.em != 0.)
            .map(|v| v.resolve(font, height))
            .unwrap_or(height)
            .min(height);
        let requested = if self.image_width.is_some() && image_height.is_some() {
            (desired_width / native_width).min(desired_height / native_height)
        } else if self.image_width.is_some() {
            desired_width / native_width
        } else if image_height.is_some() {
            desired_height / native_height
        } else {
            1.0
        };
        let factor = requested
            .min(max_width / native_width)
            .min(max_height / native_height)
            .max(0.0);
        (native_width * factor, native_height * factor)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct NativeNode {
    tag: String,
    attrs: HashMap<String, String>,
    parent: Option<usize>,
    text: String,
    #[serde(default)]
    changed: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NativeDocument {
    html: String,
    nodes: Vec<NativeNode>,
    bindings: Vec<Option<usize>>,
    css: String,
    scripts: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeEvent {
    pub node: usize,
    #[serde(default = "click")]
    pub kind: String,
}
fn click() -> String {
    "click".into()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NativePresentation {
    pub styles: Vec<NativeStyle>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeLinkTarget {
    pub chapter: usize,
    pub block: usize,
    pub char_offset: usize,
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Only relative, book-local resource names are accepted. No filesystem/network APIs.
pub(super) fn resolve(base: &str, href: &str) -> Option<String> {
    if href.contains(':') || href.starts_with('/') || href.starts_with('\\') {
        return None;
    }
    let href = href.split(['#', '?']).next()?;
    let mut parts: Vec<_> = base
        .rsplit_once('/')
        .map_or("", |x| x.0)
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    for part in href.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(format!(
        "{}{}",
        if base.starts_with('/') { "/" } else { "" },
        parts.join("/")
    ))
}
fn stylesheet(
    path: &str,
    source: &str,
    read: &mut impl FnMut(&str) -> Option<String>,
    depth: usize,
) -> String {
    if depth > 4 || source.len() > 512 * 1024 {
        return String::new();
    }
    let imports =
        regex::Regex::new(r#"@import\s+(?:url\(\s*)?["']([^"']+)["']\s*\)?\s*;"#).unwrap();
    let mut output = String::new();
    for caps in imports.captures_iter(source) {
        if let Some(path) = resolve(path, &caps[1]) {
            if let Some(s) = read(&path) {
                output.push_str(&stylesheet(&path, &s, read, depth + 1));
            }
        }
        if output.len() > 1024 * 1024 {
            break;
        }
    }
    output.push_str(&imports.replace_all(source, ""));
    output
}
impl NativeDocument {
    /// Resolve IDs on paragraphs, containers and inline/empty anchors without
    /// introducing render-only block numbers or UTF-16 offsets.
    pub fn anchor(&self, id: &str) -> Option<(usize, usize)> {
        let dom = Html::parse_document(&self.html);
        let elements: Vec<_> = dom
            .select(&Selector::parse("*").ok()?)
            .take(16_384)
            .collect();
        let target = elements.iter().find(|e| {
            e.value().attr("id") == Some(id)
                || (e.value().name() == "a" && e.value().attr("name") == Some(id))
        })?;
        for (block, binding) in self.bindings.iter().enumerate() {
            let Some(element) = binding.and_then(|i| elements.get(i)) else {
                continue;
            };
            if element.id() == target.id() || element.ancestors().any(|p| p.id() == target.id()) {
                return Some((block, 0));
            }
            if target.ancestors().any(|p| p.id() == element.id()) {
                let prefix: String = element
                    .descendants()
                    .take_while(|n| n.id() != target.id())
                    .filter(|n| {
                        !n.ancestors()
                            .filter_map(ElementRef::wrap)
                            .any(|e| matches!(e.value().name(), "rt" | "rp" | "script" | "style"))
                    })
                    .filter_map(|n| match n.value() {
                        scraper::Node::Text(t) => Some(t.text.as_ref()),
                        _ => None,
                    })
                    .collect();
                let source = self.sources.get(block)?;
                let prefix = compact(&prefix);
                let offset = if compact(source).starts_with(&prefix) && !prefix.is_empty() {
                    source
                        .chars()
                        .enumerate()
                        .filter(|(_, c)| !c.is_whitespace())
                        .nth(prefix.chars().count())
                        .map_or(source.chars().count(), |(i, _)| i)
                } else {
                    0
                };
                return Some((block, offset));
            }
        }
        // Legacy books also put empty named anchors between block elements.
        if target.text().all(|text| text.trim().is_empty()) {
            let index = elements.iter().position(|e| e.id() == target.id())?;
            if let Some((block, _)) = self
                .bindings
                .iter()
                .enumerate()
                .find(|(_, binding)| binding.is_some_and(|i| i > index))
            {
                return Some((block, 0));
            }
            if let Some((block, _)) = self
                .bindings
                .iter()
                .enumerate()
                .rev()
                .find(|(_, binding)| binding.is_some_and(|i| i < index))
            {
                return Some((block, self.sources.get(block)?.chars().count()));
            }
        }
        None
    }

    pub fn parse(
        html: &str,
        path: &str,
        blocks: &[ContentBlock],
        mut read: impl FnMut(&str) -> Option<String>,
    ) -> Self {
        let document = Html::parse_document(html);
        let selector = Selector::parse("*").unwrap();
        let mut nodes = Vec::new();
        let mut ids = HashMap::new();
        let mut css = String::new();
        let mut scripts = Vec::new();
        let mut script_bytes = 0;
        for elem in document.select(&selector).take(16_384) {
            let parent = elem.ancestors().find_map(|a| ids.get(&a.id()).copied());
            ids.insert(elem.id(), nodes.len());
            nodes.push(NativeNode {
                tag: elem.value().name().into(),
                attrs: elem
                    .value()
                    .attrs()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
                parent,
                text: elem.text().collect(),
                changed: None,
            });
            match elem.value().name() {
                "style" => css.push_str(&stylesheet(
                    path,
                    &elem.text().collect::<String>(),
                    &mut read,
                    0,
                )),
                "link"
                    if elem.value().attr("rel").is_some_and(|r| {
                        r.split_whitespace()
                            .any(|r| r.eq_ignore_ascii_case("stylesheet"))
                    }) =>
                {
                    if let Some(p) = elem.value().attr("href").and_then(|h| resolve(path, h)) {
                        if let Some(s) = read(&p) {
                            css.push_str(&stylesheet(&p, &s, &mut read, 0));
                        }
                    }
                }
                "script" if !matches!(elem.value().attr("type"), Some("module")) => {
                    let script = if let Some(src) = elem.value().attr("src") {
                        resolve(path, src).and_then(|p| read(&p))
                    } else {
                        Some(elem.text().collect())
                    };
                    if let Some(s) = script {
                        script_bytes += s.len();
                        if script_bytes <= 512 * 1024 {
                            scripts.push(s);
                        }
                    }
                }
                _ => {}
            }
        }
        if css.len() > 2 * 1024 * 1024 {
            let mut limit = 2 * 1024 * 1024;
            while !css.is_char_boundary(limit) {
                limit -= 1;
            }
            css.truncate(limit);
        }
        let compact_texts: Vec<_> = nodes.iter().map(|node| compact(&node.text)).collect();
        let mut used = std::collections::HashSet::new();
        let bindings = blocks
            .iter()
            .map(|b| {
                let (anchor, text) = match b {
                    ContentBlock::Paragraph {
                        anchor_id, spans, ..
                    }
                    | ContentBlock::Heading {
                        anchor_id, spans, ..
                    } => (
                        anchor_id.as_deref(),
                        compact(&spans.iter().map(|s| s.text.as_str()).collect::<String>()),
                    ),
                    ContentBlock::Image { .. } => {
                        let index = nodes
                            .iter()
                            .enumerate()
                            .find(|(i, n)| {
                                !used.contains(i)
                                    && matches!(n.tag.as_str(), "img" | "svg" | "image")
                            })
                            .map(|(i, _)| i);
                        if let Some(i) = index {
                            used.insert(i);
                            for (child, n) in nodes.iter().enumerate().skip(i + 1) {
                                let mut parent = n.parent;
                                while let Some(p) = parent {
                                    if p == i {
                                        if matches!(n.tag.as_str(), "img" | "svg" | "image") {
                                            used.insert(child);
                                        }
                                        break;
                                    }
                                    parent = nodes[p].parent;
                                }
                            }
                        }
                        return index;
                    }
                    _ => return None,
                };
                let index = nodes
                    .iter()
                    .enumerate()
                    .find(|(i, n)| {
                        !used.contains(i)
                            && (anchor.is_some_and(|a| n.attrs.get("id").is_some_and(|id| id == a))
                                || (!text.is_empty()
                                    && !matches!(
                                        n.tag.as_str(),
                                        "html"
                                            | "head"
                                            | "body"
                                            | "div"
                                            | "section"
                                            | "article"
                                            | "script"
                                            | "style"
                                            | "table"
                                            | "title"
                                            | "meta"
                                            | "link"
                                    )
                                    && super::native_typography::matches_compact_source(
                                        &text,
                                        &n.tag,
                                        &compact_texts[*i],
                                    )
                                    && !matches!(
                                        n.tag.as_str(),
                                        "html"
                                            | "head"
                                            | "body"
                                            | "div"
                                            | "section"
                                            | "article"
                                            | "script"
                                            | "style"
                                            | "table"
                                    )))
                    })
                    .map(|(i, _)| i);
                if let Some(i) = index {
                    used.insert(i);
                    // Supplemental media is attached to this text block; do not bind
                    // that same image again when locating later canonical images.
                    for (child, n) in nodes.iter().enumerate().skip(i + 1) {
                        let mut parent = n.parent;
                        while let Some(p) = parent {
                            if p == i {
                                if matches!(n.tag.as_str(), "img" | "svg" | "image") {
                                    used.insert(child);
                                }
                                break;
                            }
                            parent = nodes[p].parent;
                        }
                    }
                }
                index
            })
            .collect();
        Self {
            html: html.into(),
            nodes,
            bindings,
            css,
            scripts,
            sources: blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Paragraph { spans, .. } | ContentBlock::Heading { spans, .. } => {
                        spans.iter().map(|s| s.text.as_str()).collect()
                    }
                    _ => String::new(),
                })
                .collect(),
        }
    }
    pub fn present(
        &self,
        policy: CssPolicy,
        scripts: bool,
        events: &[NativeEvent],
    ) -> NativePresentation {
        let mut errors = Vec::new();
        let nodes = if scripts
            && (!self.scripts.is_empty()
                || self.nodes.iter().any(|n| n.attrs.contains_key("onclick")))
        {
            match self.execute(events) {
                Ok(nodes) => nodes,
                Err(e) => {
                    errors.push(e);
                    self.nodes.clone()
                }
            }
        } else {
            self.nodes.clone()
        };
        let dom = Html::parse_document(&self.updated_html(&nodes));
        let all = Selector::parse("[data-reader-node]").unwrap();
        let elements: HashMap<usize, ElementRef<'_>> = dom
            .select(&all)
            .filter_map(|e| Some((e.value().attr("data-reader-node")?.parse().ok()?, e)))
            .collect();
        let rules = rules(&self.css);
        let styles = self
            .bindings
            .iter()
            .enumerate()
            .map(|(block, binding)| {
                let Some(i) = *binding else {
                    return NativeStyle::default();
                };
                let Some(elem) = elements.get(&i) else {
                    return NativeStyle::default();
                };
                let mut inherited = HashMap::new();
                let mut hidden = false;
                let mut ancestors: Vec<_> = std::iter::once(**elem)
                    .chain(elem.ancestors())
                    .filter_map(ElementRef::wrap)
                    .collect();
                ancestors.reverse();
                for ancestor in ancestors {
                    let declarations = cascade(ancestor, &rules);
                    hidden |= ancestor.value().attr("hidden").is_some()
                        || declarations.get("display").is_some_and(|d| d == "none")
                        || declarations
                            .get("visibility")
                            .is_some_and(|d| d == "hidden");
                    for key in [
                        "text-align",
                        "font-size",
                        "line-height",
                        "color",
                        "font-family",
                        "font-weight",
                        "font-style",
                        "text-indent",
                    ] {
                        if let Some(value) = declarations.get(key) {
                            inherited.insert(key.into(), value.clone());
                        }
                    }
                }
                inherited.extend(cascade(*elem, &rules));
                let mut style = NativeStyle::default();
                if policy != CssPolicy::None {
                    style.align = inherited
                        .get("text-align")
                        .filter(|v| matches!(v.as_str(), "left" | "center" | "right" | "justify"))
                        .cloned();
                    style.indent = inherited.get("text-indent").and_then(|v| length(v, 16.0));
                    style.bold = inherited
                        .get("font-weight")
                        .is_some_and(|v| v == "bold" || v.parse::<u16>().is_ok_and(|n| n >= 600));
                    style.italic = inherited
                        .get("font-style")
                        .is_some_and(|v| v == "italic" || v == "oblique");
                    if policy == CssPolicy::Publisher {
                        style.size = inherited
                            .get("font-size")
                            .and_then(|v| length(v, 16.0))
                            .map(|v| v.clamp(0.5, 4.0));
                        style.line = inherited
                            .get("line-height")
                            .and_then(|v| v.parse::<f32>().ok().or_else(|| length(v, 16.0)))
                            .filter(|v| v.is_finite())
                            .map(|v| v.clamp(1.0, 3.0));
                        style.before = inherited
                            .get("margin-top")
                            .and_then(|v| length(v, 16.0))
                            .map(|v| v.max(0.0));
                        style.after = inherited
                            .get("margin-bottom")
                            .and_then(|v| length(v, 16.0))
                            .map(|v| v.max(0.0));
                        style.family = inherited.get("font-family").map(|v| {
                            v.split(',')
                                .next()
                                .unwrap_or(v)
                                .trim_matches(['\'', '"'])
                                .trim()
                                .into()
                        });
                        style.color = inherited.get("color").and_then(|v| color(v));
                    }
                }
                // Visibility is semantic for hidden and interactive nodes even without CSS.
                style.hidden = hidden;
                if scripts && errors.is_empty() {
                    style.action = elem
                        .ancestors()
                        .chain(elem.descendants())
                        .filter_map(ElementRef::wrap)
                        .find_map(|e| {
                            let idx: usize = e.value().attr("data-reader-node")?.parse().ok()?;
                            let n = nodes.get(idx)?;
                            (n.tag == "button"
                                || n.attrs.contains_key("onclick")
                                || n.attrs
                                    .get("data-native-action")
                                    .is_some_and(|v| v == "true"))
                            .then_some(idx)
                        });
                }
                if scripts {
                    let current = elem.text().collect::<String>();
                    if current != self.nodes[i].text {
                        style.text = Some(current);
                    }
                }
                super::native_typography::enrich(
                    &mut style,
                    *elem,
                    self.sources.get(block).map(String::as_str).unwrap_or(""),
                    &rules,
                    policy,
                );
                style
            })
            .collect();
        NativePresentation { styles, errors }
    }
    fn execute(&self, events: &[NativeEvent]) -> Result<Vec<NativeNode>, String> {
        use rquickjs::{Context, Runtime};
        if events.len() > 128 {
            return Err("Publication interaction limit reached".into());
        }
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_memory_limit(16 * 1024 * 1024);
        runtime.set_max_stack_size(256 * 1024);
        let until = Instant::now() + Duration::from_millis(100);
        runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= until)));
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        context.with(|ctx| {
            ctx.globals().set("__nativeInput", serde_json::to_string(&self.nodes).unwrap()).map_err(|e|e.to_string())?;
            ctx.eval::<(),_>(include_str!("native.js")).map_err(|e|format!("Native document: {e}"))?;
            for source in &self.scripts { ctx.eval::<(),_>(source.as_bytes()).map_err(|e|format!("Publication script: {e}"))?; }
            ctx.eval::<(),_>("document.readyState='complete'; document.dispatchEvent({type:'DOMContentLoaded'}); window.dispatchEvent({type:'load'});").map_err(|e|e.to_string())?;
            for event in events { if event.node>=self.nodes.len() || !matches!(event.kind.as_str(),"click"|"change") { return Err("Invalid publication event".into()); }
                ctx.eval::<(),_>(format!("__nativeNodes[{}].dispatchEvent({{type:{}}});",event.node,serde_json::to_string(&event.kind).unwrap())).map_err(|e|format!("Publication event: {e}"))?;
            }
            let snapshot: String = ctx.eval("JSON.stringify(__nativeNodes.map(n=>n.snapshot()))").map_err(|e|e.to_string())?;
            if snapshot.len()>4*1024*1024 { return Err("Publication state exceeds limit".into()); }
            serde_json::from_str(&snapshot).map_err(|e|e.to_string())
        })
    }
    fn updated_html(&self, nodes: &[NativeNode]) -> String {
        // Preserve actual markup/text ordering; change only element attributes.
        let dom = Html::parse_document(&self.html);
        let mut html = String::new();
        let mut index = 0;
        fn write(
            node: ego_tree::NodeRef<'_, scraper::Node>,
            nodes: &[NativeNode],
            index: &mut usize,
            output: &mut String,
        ) {
            if let Some(e) = ElementRef::wrap(node) {
                let i = *index;
                *index += 1;
                output.push('<');
                output.push_str(e.value().name());
                let attrs = nodes.get(i).map(|n| &n.attrs);
                if let Some(attrs) = attrs {
                    for (key, value) in attrs {
                        if key.starts_with("on") || key == "data-reader-node" {
                            continue;
                        }
                        output.push(' ');
                        output.push_str(key);
                        output.push_str("=\"");
                        output.push_str(&escape(value));
                        output.push('"');
                    }
                }
                output.push_str(&format!(" data-reader-node=\"{i}\">"));
                if let Some(text) = nodes.get(i).and_then(|n| n.changed.as_ref()) {
                    output.push_str(&escape(text));
                    // Consume source descendants to keep all stable node indices.
                    *index += node
                        .descendants()
                        .skip(1)
                        .filter(|n| ElementRef::wrap(*n).is_some())
                        .count();
                } else {
                    for child in node.children() {
                        write(child, nodes, index, output);
                    }
                }
                output.push_str("</");
                output.push_str(e.value().name());
                output.push('>');
            } else if let scraper::Node::Text(t) = node.value() {
                output.push_str(&escape(t));
            } else {
                for child in node.children() {
                    write(child, nodes, index, output);
                }
            }
        }
        write(dom.tree.root(), nodes, &mut index, &mut html);
        html
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}
fn length(s: &str, base: f32) -> Option<f32> {
    let s = s.trim();
    let value = if s == "0" {
        Some(0.0)
    } else if let Some(s) = s.strip_suffix("rem").or_else(|| s.strip_suffix("em")) {
        s.trim().parse().ok()
    } else if let Some(s) = s.strip_suffix('%') {
        s.trim().parse::<f32>().ok().map(|n| n / 100.0)
    } else if let Some(s) = s.strip_suffix("px").or_else(|| s.strip_suffix("pt")) {
        s.trim().parse::<f32>().ok().map(|n| n / base)
    } else {
        None
    };
    value.filter(|n| n.is_finite()).map(|n| n.clamp(-4.0, 12.0))
}
pub(super) fn color(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(value);
        }
        if hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(format!(
                "#{}",
                hex.chars().flat_map(|c| [c, c]).collect::<String>()
            ));
        }
    }
    if let Some(inner) = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let rgb: Option<Vec<u8>> = inner
            .split(',')
            .map(|n| n.trim().parse::<u8>().ok())
            .collect();
        if let Some(rgb) = rgb.filter(|v| v.len() == 3) {
            return Some(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]));
        }
    }
    match value.as_str() {
        "black" => Some("#000000".into()),
        "white" => Some("#ffffff".into()),
        "red" => Some("#ff0000".into()),
        "blue" => Some("#0000ff".into()),
        "green" => Some("#008000".into()),
        "gray" | "grey" => Some("#808080".into()),
        _ => None,
    }
}

impl NativeDocument {
    pub fn note(&self, id: &str) -> Option<String> {
        let dom = Html::parse_document(&self.html);
        dom.select(&Selector::parse("[id]").ok()?)
            .find(|e| e.value().attr("id") == Some(id))
            .map(|e| {
                e.text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty() && s.len() <= 32768)
    }
}
