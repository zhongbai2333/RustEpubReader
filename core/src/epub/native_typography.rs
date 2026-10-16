//! Presentation-only metadata for native text, images and semantic structures.
use super::native::{CssPolicy, NativeStyle};
use super::native_css::{Computed, Dimension, Rule};
use scraper::ElementRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct InlinePresentation {
    pub start: usize,
    pub end: usize,
    pub size: Option<f32>,
    pub color: Option<String>,
    pub family: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: bool,
    pub strike: bool,
    pub hidden: bool,
    pub baseline: f32,
    pub small_caps: bool,
    pub note: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ListLine {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub depth: usize,
    pub marker: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Ruby {
    pub start: usize,
    pub end: usize,
    pub text: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TableCell {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub header: bool,
    pub span: usize,
    pub align: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub columns: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NativeNote {
    pub text: String,
    pub href: String,
}

fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
#[cfg(test)]
pub fn matches_compact_source(source: &str, tag: &str, text: &str) -> bool {
    if source == text {
        return true;
    }
    match tag {
        "li" => source.ends_with(&text),
        "tr" => source.replace('|', "") == text,
        _ => source.strip_prefix('│') == Some(text),
    }
}
struct SourceMap {
    positions: Vec<usize>,
    offset: usize,
}
impl SourceMap {
    fn new(source: &str, text: &str) -> Option<Self> {
        let chars: Vec<char> = source.chars().collect();
        let compact_source: String = chars.iter().filter(|c| !c.is_whitespace()).collect();
        let dom = compact(text);
        let at = compact_source.find(&dom)?;
        let offset = compact_source[..at].chars().count();
        let positions = chars
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(i, _)| i)
            .collect();
        Some(Self { positions, offset })
    }
    fn range(&self, start: usize, end: usize) -> Option<(usize, usize)> {
        if start >= end {
            return None;
        }
        Some((
            *self.positions.get(self.offset + start)?,
            self.positions.get(self.offset + end - 1)? + 1,
        ))
    }
}
fn count(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}
fn node_ranges(elem: ElementRef<'_>) -> HashMap<ego_tree::NodeId, (usize, usize)> {
    fn visit(
        node: ego_tree::NodeRef<'_, scraper::Node>,
        cursor: &mut usize,
        map: &mut HashMap<ego_tree::NodeId, (usize, usize)>,
    ) {
        let start = *cursor;
        if let scraper::Node::Text(text) = node.value() {
            *cursor += count(text);
        } else {
            for child in node.children() {
                visit(child, cursor, map)
            }
        }
        map.insert(node.id(), (start, *cursor));
    }
    let mut map = HashMap::new();
    visit(*elem, &mut 0, &mut map);
    map
}
pub fn enrich(
    style: &mut NativeStyle,
    elem: ElementRef<'_>,
    source: &str,
    rules: &[Rule],
    policy: CssPolicy,
) {
    let mut ancestors: Vec<_> = elem.ancestors().filter_map(ElementRef::wrap).collect();
    ancestors.reverse();
    let mut computed = Computed::default();
    for parent in &ancestors {
        computed = computed.child(*parent, rules);
    }
    computed = computed.child(elem, rules);
    let list_depth = elem
        .ancestors()
        .filter_map(ElementRef::wrap)
        .filter(|e| matches!(e.value().name(), "ul" | "ol"))
        .count();
    style.list_depth = list_depth.min(8) as u8;
    style.quote = elem.value().name() == "blockquote"
        || elem
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|e| e.value().name() == "blockquote");
    style.pre = elem.value().name() == "pre"
        || elem
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|e| e.value().name() == "pre")
        || computed
            .props
            .get("white-space")
            .is_some_and(|v| matches!(v.as_str(), "pre" | "pre-wrap" | "break-spaces"));
    if list_depth > 0 {
        style.indent = Some(0.);
        style.inset_left = Some(Dimension {
            em: list_depth.min(8) as f32 * 1.2,
            ..Default::default()
        });
    }
    if style.pre {
        style.family = Some("monospace".into());
        style.indent = Some(0.);
    }
    if style.quote {
        style.indent = Some(0.);
        style.inset_left = Some(Dimension {
            em: 1.,
            ..Default::default()
        });
        style.inset_right = Some(Dimension {
            em: 1.,
            ..Default::default()
        });
    }
    if policy != CssPolicy::None {
        if let Some(v) = computed.dimension("margin-left") {
            style.inset_left = Some(v);
        }
        if let Some(v) = computed.dimension("margin-right") {
            style.inset_right = Some(v);
        }
        style.indent_length = computed.dimension("text-indent");
        style.padding_left = computed.dimension("padding-left");
        style.padding_right = computed.dimension("padding-right");
        style.padding_top = computed.dimension("padding-top");
        style.padding_bottom = computed.dimension("padding-bottom");
        if policy == CssPolicy::Publisher {
            style.size = None;
            style.size_px = computed.authored_font.then_some(computed.font);
            style.line = computed.line.map(|n| n.clamp(1., 4.));
            style.margin_top = computed.dimension("margin-top");
            style.margin_bottom = computed.dimension("margin-bottom");
            style.indent_length = computed.dimension("text-indent");
            if style.margin_top.is_some() {
                style.before = Some(0.);
            }
            if style.margin_bottom.is_some() {
                style.after = Some(0.);
            }
        }
    }
    if elem.value().name() == "img"
        || elem.value().name() == "svg"
        || elem.value().name() == "image"
    {
        if policy != CssPolicy::None {
            image(style, &computed, elem);
        }
        return;
    }
    if policy != CssPolicy::None {
        style.align = computed
            .props
            .get("text-align")
            .and_then(|v| match v.as_str() {
                "left" | "right" | "center" | "justify" => Some(v.clone()),
                "start" => Some("left".into()),
                "end" => Some("right".into()),
                _ => None,
            })
            .or(style.align.clone());
        if policy == CssPolicy::Publisher {
            style.family = computed.props.get("font-family").map(|v| family(v));
            style.color = computed
                .props
                .get("color")
                .and_then(|v| super::native::color(v));
        }
        if !source.is_empty() {
            style.inline.push(InlinePresentation {
                start: 0,
                end: source.chars().count(),
                bold: computed
                    .props
                    .get("font-weight")
                    .map(|v| v == "bold" || v.parse::<u16>().is_ok_and(|n| n >= 600)),
                italic: computed
                    .props
                    .get("font-style")
                    .map(|v| v == "italic" || v == "oblique"),
                small_caps: small_caps(&computed),
                underline: decoration(&computed).contains("underline"),
                strike: decoration(&computed).contains("line-through"),
                ..Default::default()
            });
        }
    }
    // A supplemental illustration keeps its canonical paragraph's coordinate.
    if let Some(img) = elem
        .select(&scraper::Selector::parse("img,svg").unwrap())
        .next()
    {
        if policy != CssPolicy::None {
            let image_style = computed.child(img, rules);
            image(style, &image_style, img);
        }
    }
    if elem.value().name() == "tr" {
        table(
            style,
            elem,
            source,
            if policy == CssPolicy::None {
                &[]
            } else {
                rules
            },
        );
        return;
    }
    let text = elem.text().collect::<String>();
    let Some(mapping) = SourceMap::new(source, &text) else {
        return;
    };
    let ranges = node_ranges(elem);
    if elem.value().name() == "li"
        && elem
            .select(&scraper::Selector::parse("li").unwrap())
            .next()
            .is_some()
    {
        let chars: Vec<_> = source.chars().collect();
        for item in
            std::iter::once(elem).chain(elem.select(&scraper::Selector::parse("li").unwrap()))
        {
            let mut begin = None;
            let mut end = None;
            for child in item.children().filter(|c| {
                !ElementRef::wrap(*c).is_some_and(|e| matches!(e.value().name(), "ul" | "ol"))
            }) {
                if let Some(&(start, stop)) = ranges.get(&child.id()) {
                    if stop > start {
                        begin.get_or_insert(start);
                        end = Some(stop);
                    }
                }
            }
            if let (Some(begin), Some(end)) = (begin, end) {
                if let Some((start, end)) = mapping.range(begin, end) {
                    let depth = item
                        .ancestors()
                        .filter_map(ElementRef::wrap)
                        .filter(|e| matches!(e.value().name(), "ul" | "ol"))
                        .count();
                    let parent = item.parent().and_then(ElementRef::wrap);
                    let marker = if parent.is_some_and(|e| e.value().name() == "ol") {
                        let first = parent
                            .and_then(|e| e.value().attr("start"))
                            .and_then(|s| s.parse::<i32>().ok())
                            .unwrap_or(1);
                        let index = item
                            .prev_siblings()
                            .filter_map(ElementRef::wrap)
                            .filter(|e| e.value().name() == "li")
                            .count();
                        format!("{}.", first + index as i32)
                    } else {
                        "•".into()
                    };
                    style.list.push(ListLine {
                        start,
                        end,
                        text: chars[start..end].iter().collect(),
                        depth: depth.min(8),
                        marker,
                    });
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn visit(
        elem: ElementRef<'_>,
        parent: &Computed,
        root: &Computed,
        source: &SourceMap,
        ranges: &HashMap<ego_tree::NodeId, (usize, usize)>,
        rules: &[Rule],
        policy: CssPolicy,
        output: &mut Vec<InlinePresentation>,
    ) {
        let computed = parent.child(elem, rules);
        if let Some(&(start, end)) = ranges.get(&elem.id()) {
            if let Some((start, end)) = source.range(start, end) {
                let tag = elem.value().name();
                let mut item = InlinePresentation {
                    start,
                    end,
                    ..Default::default()
                };
                item.hidden = matches!(tag, "rt" | "rp") || computed.hidden;
                if tag == "small" {
                    item.size = Some(0.75);
                }
                item.underline = matches!(tag, "u" | "ins");
                item.strike = matches!(tag, "s" | "del" | "strike");
                if tag == "sup" {
                    item.size = Some(0.75);
                    item.baseline = 0.4;
                }
                if tag == "sub" {
                    item.size = Some(0.75);
                    item.baseline = -0.2;
                }
                if tag == "code" || tag == "kbd" || tag == "samp" {
                    item.family = Some("monospace".into());
                }
                item.note = elem
                    .value()
                    .attr("href")
                    .filter(|_| is_note_ref(elem))
                    .map(str::to_owned);
                if policy != CssPolicy::None {
                    item.size = Some((computed.font / root.font).clamp(0.4, 4.));
                    item.bold = computed
                        .props
                        .get("font-weight")
                        .map(|v| v == "bold" || v.parse::<u16>().is_ok_and(|v| v >= 600));
                    item.italic = computed
                        .props
                        .get("font-style")
                        .map(|v| v == "italic" || v == "oblique");
                    item.small_caps = small_caps(&computed);
                    if policy == CssPolicy::Publisher {
                        item.color = computed
                            .props
                            .get("color")
                            .and_then(|v| super::native::color(v));
                        item.family = computed.props.get("font-family").map(|v| family(v));
                    }
                    item.underline |= decoration(&computed).contains("underline");
                    item.strike |= decoration(&computed).contains("line-through");
                    if let Some(v) = computed.props.get("vertical-align") {
                        if v == "super" {
                            item.baseline = 0.4;
                            item.size = Some(item.size.unwrap_or(1.) * 0.75);
                        }
                        if v == "sub" {
                            item.baseline = -0.2;
                            item.size = Some(item.size.unwrap_or(1.) * 0.75);
                        }
                    }
                }
                if tag == "sup" || tag == "sub" {
                    item.size = Some(item.size.unwrap_or(1.).min(0.75));
                }
                output.push(item);
            }
        }
        for child in elem.children().filter_map(ElementRef::wrap) {
            visit(
                child, &computed, root, source, ranges, rules, policy, output,
            );
        }
    }
    if is_note_ref(elem) {
        if let Some((start, end)) = mapping.range(0, count(&text)) {
            style.inline.push(InlinePresentation {
                start,
                end,
                note: elem.value().attr("href").map(str::to_owned),
                ..Default::default()
            });
        }
    }
    for child in elem.children().filter_map(ElementRef::wrap) {
        visit(
            child,
            &computed,
            &computed,
            &mapping,
            &ranges,
            rules,
            policy,
            &mut style.inline,
        );
    }
    for ruby in std::iter::once(elem)
        .chain(elem.descendants().filter_map(ElementRef::wrap))
        .filter(|e| e.value().name() == "ruby")
    {
        let mut base_start = None;
        let mut base_end = None;
        for child in ruby.children() {
            match ElementRef::wrap(child).map(|e| e.value().name()) {
                Some("rp") => {}
                Some("rt") => {
                    if let (Some(start), Some(end)) = (base_start, base_end) {
                        if let Some((start, end)) = mapping.range(start, end) {
                            style.ruby.push(Ruby {
                                start,
                                end,
                                text: ElementRef::wrap(child).unwrap().text().collect(),
                            });
                        }
                    }
                    base_start = None;
                    base_end = None;
                }
                _ => {
                    if let Some(&(start, end)) = ranges.get(&child.id()) {
                        if end > start {
                            base_start.get_or_insert(start);
                            base_end = Some(end);
                        }
                    }
                }
            }
        }
    }
}
fn decoration(computed: &Computed) -> &str {
    computed
        .props
        .get("text-decoration-line")
        .or_else(|| computed.props.get("text-decoration"))
        .map_or("", String::as_str)
}
fn small_caps(computed: &Computed) -> bool {
    computed
        .props
        .get("font-variant-caps")
        .or_else(|| computed.props.get("font-variant"))
        .is_some_and(|v| v.contains("small-caps"))
}
fn family(value: &str) -> String {
    let names: Vec<_> = value
        .split(',')
        .map(|n| n.trim().trim_matches(['\'', '"']))
        .collect();
    names
        .iter()
        .find(|n| matches!(**n, "serif" | "sans-serif" | "monospace" | "system-ui"))
        .copied()
        .unwrap_or_else(|| names.first().copied().unwrap_or("serif"))
        .replace("system-ui", "sans-serif")
}
fn is_note_ref(elem: ElementRef<'_>) -> bool {
    elem.value()
        .attr("epub:type")
        .is_some_and(|v| v.split_whitespace().any(|v| v == "noteref"))
        || elem.value().attr("role") == Some("doc-noteref")
        || elem.value().attr("class").is_some_and(|v| {
            v.split_whitespace()
                .any(|v| matches!(v, "noteref" | "footnote-ref" | "fnanchor"))
        })
}
fn image(style: &mut NativeStyle, computed: &Computed, elem: ElementRef<'_>) {
    let dimension = |key: &str| {
        computed.dimension(key).or_else(|| {
            elem.value()
                .attr(key)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|n| n.is_finite() && *n > 0.)
                .map(|px| Dimension {
                    px,
                    ..Default::default()
                })
        })
    };
    style.image_width = dimension("width");
    style.image_height = dimension("height");
    style.image_max_width = computed.dimension("max-width");
    style.image_max_height = computed.dimension("max-height");
}
fn table(style: &mut NativeStyle, elem: ElementRef<'_>, source: &str, rules: &[Rule]) {
    let chars: Vec<char> = source.chars().collect();
    let mut search = 0;
    let mut cells = Vec::new();
    for cell in elem
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|e| matches!(e.value().name(), "td" | "th"))
    {
        let text = cell.text().collect::<String>();
        let target = compact(&text);
        if target.is_empty() {
            cells.push(TableCell {
                start: search,
                end: search,
                text: String::new(),
                header: cell.value().name() == "th",
                span: cell
                    .value()
                    .attr("colspan")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1)
                    .clamp(1, 12),
                align: None,
            });
            continue;
        }
        let remaining: String = chars[search..].iter().collect();
        let Some(mapping) = SourceMap::new(&remaining, &text) else {
            continue;
        };
        let Some((start, end)) = mapping.range(0, count(&text)) else {
            continue;
        };
        let computed = Computed::default().child(cell, rules);
        let span = cell
            .value()
            .attr("colspan")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 12);
        cells.push(TableCell {
            start: search + start,
            end: search + end,
            text: chars[search + start..search + end].iter().collect(),
            header: cell.value().name() == "th",
            span,
            align: computed.props.get("text-align").cloned(),
        });
        search += end;
    }
    if !cells.is_empty() {
        let columns = elem
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|e| e.value().name() == "table")
            .map(|table| {
                table
                    .select(&scraper::Selector::parse("tr").unwrap())
                    .map(|row| {
                        row.children()
                            .filter_map(ElementRef::wrap)
                            .filter(|e| matches!(e.value().name(), "td" | "th"))
                            .map(|cell| {
                                cell.value()
                                    .attr("colspan")
                                    .and_then(|s| s.parse::<usize>().ok())
                                    .unwrap_or(1)
                                    .clamp(1, 12)
                            })
                            .sum::<usize>()
                    })
                    .max()
                    .unwrap_or(1)
            })
            .unwrap_or_else(|| cells.iter().map(|c| c.span).sum());
        style.table = Some(TableRow { cells, columns });
        style.indent = Some(0.);
    }
}
