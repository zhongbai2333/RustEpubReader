//! CSS declarations and computed units used by the native typesetters.
use scraper::{ElementRef, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
static LENGTH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"([+-]?)\s*(\d*\.?\d+)\s*(rem|em|px|pt|pc|in|cm|mm|%)?").unwrap()
});
static COMMENTS_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)/\*.*?\*/").unwrap());

/// Additive CSS length. Percentages resolve against the containing width for
/// boxes, and against the image's available extent for image sizing.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Dimension {
    pub px: f32,
    pub em: f32,
    pub percent: f32,
}
impl Dimension {
    pub fn resolve(&self, font: f32, extent: f32) -> f32 {
        (self.px
            + self.em * font
            + if extent.is_finite() {
                self.percent * extent
            } else {
                0.0
            })
        .max(0.0)
    }
    pub fn parse(value: &str, root: f32) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        if value == "auto" || value == "none" {
            return None;
        }
        let expression = value
            .strip_prefix("calc(")
            .and_then(|v| v.strip_suffix(')'))
            .unwrap_or(&value);
        let re = &*LENGTH_RE;
        let mut result = Self::default();
        let mut end = 0;
        let mut any = false;
        for cap in re.captures_iter(expression) {
            let whole = cap.get(0)?;
            if !expression[end..whole.start()].trim().is_empty() {
                return None;
            }
            let n = cap[2].parse::<f32>().ok()? * if &cap[1] == "-" { -1.0 } else { 1.0 };
            if !n.is_finite() {
                return None;
            }
            match cap.get(3).map(|v| v.as_str()) {
                Some("em") => result.em += n,
                Some("rem") => result.px += n * root,
                Some("%") => result.percent += n / 100.,
                Some("px") => result.px += n,
                Some("pt") => result.px += n * 4. / 3.,
                Some("pc") => result.px += n * 16.,
                Some("in") => result.px += n * 96.,
                Some("cm") => result.px += n * 96. / 2.54,
                Some("mm") => result.px += n * 96. / 25.4,
                None if n == 0. => {}
                _ => return None,
            }
            end = whole.end();
            any = true;
        }
        (any && expression[end..].trim().is_empty()).then_some(result)
    }
}
#[derive(Clone)]
pub struct Rule {
    selector: Selector,
    specificity: (u16, u16, u16),
    declarations: Vec<(String, String, bool)>,
}
fn split_outside(value: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quote = None;
    let mut escape = false;
    for (i, c) in value.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            continue;
        }
        if c == '(' || c == '[' {
            depth += 1;
        }
        if c == ')' || c == ']' {
            depth -= 1;
        }
        if c == delimiter && depth == 0 {
            parts.push(&value[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&value[start..]);
    parts
}
fn specificity(selector: &str) -> (u16, u16, u16) {
    // :where contributes zero; :is/:not use their most specific argument.
    let mut ids = 0u16;
    let mut classes = 0u16;
    let mut types = 0u16;
    let mut it = selector.chars().peekable();
    let mut type_start = true;
    while let Some(c) = it.next() {
        match c {
            '#' => {
                ids = ids.saturating_add(1);
                type_start = false;
            }
            '.' => {
                classes = classes.saturating_add(1);
                type_start = false;
            }
            '[' => {
                classes = classes.saturating_add(1);
                let mut quote = None;
                for c in it.by_ref() {
                    if let Some(q) = quote {
                        if c == q {
                            quote = None;
                        }
                    } else if c == '\'' || c == '"' {
                        quote = Some(c);
                    } else if c == ']' {
                        break;
                    }
                }
                type_start = false;
            }
            ':' => {
                let mut name = String::new();
                while it
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '-')
                {
                    name.push(it.next().unwrap());
                }
                if matches!(name.as_str(), "is" | "not" | "where" | "has")
                    && it.peek() == Some(&'(')
                {
                    it.next();
                    let mut args = String::new();
                    let mut depth = 1;
                    for c in it.by_ref() {
                        if c == '(' {
                            depth += 1;
                        }
                        if c == ')' {
                            depth -= 1;
                        }
                        if depth == 0 {
                            break;
                        }
                        args.push(c);
                    }
                    if name != "where" {
                        let score = split_outside(&args, ',')
                            .into_iter()
                            .map(specificity)
                            .max()
                            .unwrap_or_default();
                        ids = ids.saturating_add(score.0);
                        classes = classes.saturating_add(score.1);
                        types = types.saturating_add(score.2);
                    }
                } else {
                    classes = classes.saturating_add(1);
                }
                type_start = false;
            }
            ' ' | '\n' | '\t' | '>' | '+' | '~' | ',' => type_start = true,
            '*' => type_start = false,
            c if c.is_alphabetic() && type_start => {
                types = types.saturating_add(1);
                type_start = false;
            }
            _ => {}
        }
    }
    (ids, classes, types)
}
fn whitespace_tokens(value: &str) -> Vec<&str> {
    let mut output = Vec::new();
    let mut start = None;
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in value.char_indices() {
        if start.is_none() && !c.is_whitespace() {
            start = Some(i);
        }
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            continue;
        }
        if c == '(' {
            depth += 1;
        }
        if c == ')' {
            depth -= 1;
        }
        if c.is_whitespace() && depth == 0 {
            if let Some(from) = start.take() {
                output.push(&value[from..i]);
            }
        }
    }
    if let Some(from) = start {
        output.push(&value[from..]);
    }
    output
}
fn declarations(value: &str) -> Vec<(String, String, bool)> {
    split_outside(value, ';')
        .into_iter()
        .flat_map(|entry| {
            let Some((key, value)) = entry.split_once(':') else {
                return Vec::new();
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            let lower = value.to_ascii_lowercase();
            let important = lower.ends_with("!important");
            let value = if important {
                value[..value.len() - 10].trim()
            } else {
                value
            };
            if key == "margin" || key == "padding" {
                let parts = whitespace_tokens(value);
                if parts.is_empty() || parts.len() > 4 {
                    return Vec::new();
                }
                let values = [
                    parts[0],
                    *parts.get(1).unwrap_or(&parts[0]),
                    *parts.get(2).unwrap_or(&parts[0]),
                    *parts.get(3).unwrap_or(parts.get(1).unwrap_or(&parts[0])),
                ];
                ["top", "right", "bottom", "left"]
                    .into_iter()
                    .zip(values)
                    .map(|(side, v)| (format!("{key}-{side}"), v.into(), important))
                    .collect()
            } else {
                vec![(key, value.into(), important)]
            }
        })
        .collect()
}
/// Balanced scanning keeps quoted braces, data URLs and conditional blocks from
/// becoming accidental selectors. Print/unknown conditional layouts are omitted.
pub fn rules(css: &str) -> Vec<Rule> {
    let comments = &*COMMENTS_RE;
    let css = comments.replace_all(css, "");
    let mut rules = Vec::new();
    scan(&css, &mut rules);
    rules.truncate(4096);
    rules
}
fn scan(css: &str, rules: &mut Vec<Rule>) {
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut open = None;
    let mut depth = 0;
    for (i, c) in css.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            continue;
        }
        if c == ';' && open.is_none() {
            start = i + 1;
        }
        if c == '{' {
            if depth == 0 {
                open = Some(i);
            }
            depth += 1;
        }
        if c == '}' {
            depth -= 1;
            if depth == 0 {
                if let Some(o) = open.take() {
                    let head = css[start..o].trim();
                    let body = &css[o + 1..i];
                    if head.starts_with('@') {
                        if head == "@media screen" || head == "@media all" {
                            scan(body, rules);
                        }
                    } else {
                        for selector in split_outside(head, ',') {
                            if let Ok(parsed) = Selector::parse(selector.trim()) {
                                rules.push(Rule {
                                    selector: parsed,
                                    specificity: specificity(selector),
                                    declarations: declarations(body),
                                });
                            }
                        }
                    }
                    start = i + 1;
                }
            }
        }
    }
}
pub fn cascade(elem: ElementRef<'_>, rules: &[Rule]) -> HashMap<String, String> {
    type Rank = (bool, (u16, u16, u16), usize);
    let mut values: HashMap<String, (Rank, String)> = HashMap::new();
    for (order, rule) in rules.iter().enumerate() {
        if rule.selector.matches(&elem) {
            for (k, v, important) in &rule.declarations {
                let rank = (*important, rule.specificity, order);
                if values.get(k).is_none_or(|(old, _)| rank >= *old) {
                    values.insert(k.clone(), (rank, v.clone()));
                }
            }
        }
    }
    if let Some(style) = elem.value().attr("style") {
        for (k, v, important) in declarations(style) {
            let rank = (important, (u16::MAX, 0, 0), usize::MAX);
            if values.get(&k).is_none_or(|(old, _)| rank >= *old) {
                values.insert(k, (rank, v));
            }
        }
    }
    values.into_iter().map(|(k, (_, v))| (k, v)).collect()
}
#[derive(Clone)]
pub struct Computed {
    pub props: HashMap<String, String>,
    pub font: f32,
    pub root: f32,
    pub authored_font: bool,
    pub line: Option<f32>,
    line_pixels: Option<f32>,
    line_multiplier: Option<f32>,
    pub hidden: bool,
}
impl Default for Computed {
    fn default() -> Self {
        Self {
            props: HashMap::new(),
            font: 16.,
            root: 16.,
            authored_font: false,
            line: None,
            line_pixels: None,
            line_multiplier: None,
            hidden: false,
        }
    }
}
impl Computed {
    pub fn child(&self, elem: ElementRef<'_>, rules: &[Rule]) -> Self {
        let mut own = cascade(elem, rules);
        for key in [
            "color",
            "font-family",
            "font-weight",
            "font-style",
            "font-variant",
            "font-variant-caps",
            "text-align",
            "text-indent",
            "white-space",
            "text-decoration",
            "text-decoration-line",
        ] {
            if own.get(key).is_some_and(|v| v == "inherit" || v == "unset") {
                let value = self.props.get(key).cloned().or_else(|| match key {
                    "font-weight" | "font-style" | "font-variant" | "font-variant-caps"
                    | "white-space" => Some("normal".into()),
                    "text-align" => Some("left".into()),
                    "text-indent" => Some("0".into()),
                    "text-decoration" | "text-decoration-line" => Some("none".into()),
                    _ => None,
                });
                if let Some(value) = value {
                    own.insert(key.into(), value);
                } else {
                    own.remove(key);
                }
            }
        }
        let mut props = HashMap::new();
        for (k, v) in &self.props {
            if k.starts_with("--")
                || matches!(
                    k.as_str(),
                    "color"
                        | "font-family"
                        | "font-weight"
                        | "font-style"
                        | "font-variant"
                        | "font-variant-caps"
                        | "text-align"
                        | "text-indent"
                        | "white-space"
                        | "text-decoration"
                        | "text-decoration-line"
                        | "line-height"
                )
            {
                props.insert(k.clone(), v.clone());
            }
        }
        if matches!(
            elem.value().name(),
            "strong" | "b" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        ) && !own.contains_key("font-weight")
        {
            props.insert("font-weight".into(), "bold".into());
        }
        if matches!(elem.value().name(), "em" | "i") && !own.contains_key("font-style") {
            props.insert("font-style".into(), "italic".into());
        }
        if matches!(elem.value().name(), "pre" | "code" | "kbd" | "samp")
            && !own.contains_key("font-family")
        {
            props.insert("font-family".into(), "monospace".into());
        }
        props.extend(own.clone());
        let mut computed = Self {
            props,
            font: self.font,
            root: self.root,
            authored_font: self.authored_font,
            line: self.line,
            line_pixels: self.line_pixels,
            line_multiplier: self.line_multiplier,
            hidden: self.hidden
                || elem.value().attr("hidden").is_some()
                || own.get("display").is_some_and(|v| v == "none")
                || own.get("visibility").is_some_and(|v| v == "hidden"),
        };
        for key in [
            "color",
            "font-family",
            "font-weight",
            "font-style",
            "font-variant",
            "font-variant-caps",
            "text-align",
            "text-indent",
            "white-space",
            "text-decoration",
            "text-decoration-line",
        ] {
            if let Some(value) = computed.props.get(key).cloned() {
                let value = computed.value(&value);
                computed.props.insert(key.into(), value);
            }
        }
        if let Some(weight) = computed.props.get("font-weight").cloned() {
            let parent = self
                .props
                .get("font-weight")
                .map(|v| {
                    if v == "bold" {
                        700
                    } else {
                        v.parse::<u16>().unwrap_or(400)
                    }
                })
                .unwrap_or(400);
            let value = match weight.as_str() {
                "bolder" => Some(if parent < 350 {
                    400
                } else if parent < 550 {
                    700
                } else {
                    900
                }),
                "lighter" => Some(if parent < 550 {
                    100
                } else if parent < 750 {
                    400
                } else {
                    700
                }),
                _ => None,
            };
            if let Some(value) = value {
                computed
                    .props
                    .insert("font-weight".into(), value.to_string());
            }
        }
        if !own.contains_key("font-size") {
            computed.font *= match elem.value().name() {
                "h1" => 2.,
                "h2" => 1.5,
                "h3" => 1.17,
                "h4" => 1.,
                "h5" => 0.83,
                "h6" => 0.67,
                "small" | "sup" | "sub" => 0.75,
                _ => 1.,
            };
        }
        if let Some(value) = own.get("font-size") {
            let value = computed.value(value);
            if let Some(size) = font_size(&value, self.font, self.root) {
                computed.font = size.clamp(6., 96.);
                computed.authored_font = true;
            }
        }
        if elem.value().name() == "html" {
            computed.root = computed.font;
        }
        if let Some(value) = own.get("line-height") {
            let value = computed.value(value);
            if value == "normal" || value == "initial" {
                computed.line_pixels = None;
                computed.line_multiplier = None;
            } else if value != "inherit" && value != "unset" {
                if let Some(multiplier) = value
                    .parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite() && *n > 0.)
                {
                    computed.line_multiplier = Some(multiplier);
                    computed.line_pixels = None;
                } else if let Some(length) = Dimension::parse(&value, computed.root) {
                    computed.line_pixels = Some(length.resolve(computed.font, computed.font));
                    computed.line_multiplier = None;
                }
            }
        }
        computed.line = computed
            .line_multiplier
            .or_else(|| computed.line_pixels.map(|n| n / computed.font));
        computed
    }
    pub fn value(&self, value: &str) -> String {
        let mut value = value.to_owned();
        for _ in 0..8 {
            let Some(start) = value.find("var(") else {
                break;
            };
            let Some(end) = value[start..].find(')').map(|n| n + start) else {
                break;
            };
            let expression = &value[start + 4..end];
            let (name, fallback) = expression.split_once(',').unwrap_or((expression, ""));
            let replacement = self
                .props
                .get(name.trim())
                .map(|s| s.as_str())
                .unwrap_or(fallback.trim())
                .to_owned();
            value.replace_range(start..=end, &replacement);
        }
        value
    }
    pub fn dimension(&self, key: &str) -> Option<Dimension> {
        Dimension::parse(&self.value(self.props.get(key)?), self.root)
    }
}
fn font_size(value: &str, parent: f32, root: f32) -> Option<f32> {
    match value.trim() {
        "inherit" | "unset" => Some(parent),
        "initial" | "medium" => Some(16.),
        "smaller" => Some(parent * 0.8),
        "larger" => Some(parent * 1.2),
        "small" => Some(13.),
        "large" => Some(18.),
        _ => Dimension::parse(value, root).map(|v| v.resolve(parent, parent)),
    }
}
