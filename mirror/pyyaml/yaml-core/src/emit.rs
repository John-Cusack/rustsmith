//! YAML 1.1 emitter: events to text.
//!
//! Faithful port of PyYAML 6.0.2 `lib/yaml/emitter.py`. Consumes
//! [`EventIn`] values (converted from Python events by the binding) through
//! the same queue + lookahead (`need_events`) discipline, tracks
//! line/column/whitespace in characters, and accumulates output text. All
//! `%`-style error text stays templates + args; `%02X`/`%04X`/`%08X`
//! escapes are plain ASCII formatting done here. Byte encoding (`encoding`)
//! happens facade-side, exactly like the original's per-write `.encode`.

use super::{ErrArg, Formatted, YStr};

// ---------------------------------------------------------------------------
// Input events (from Python) and options.
// ---------------------------------------------------------------------------

/// Input events, converted from Python events by the binding. All human
/// text rides as code-point vectors (lossless through lone surrogates);
/// the binding builds these with a single `surrogatepass` decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventIn {
    StreamStart,
    StreamEnd,
    DocumentStart {
        explicit: bool,
        // Decimal strings (facade `str()`s Python ints): exact past i64.
        version: Option<(String, String)>,
        tags: Option<Vec<(Vec<u32>, Vec<u32>)>>,
    },
    DocumentEnd { explicit: bool },
    Alias { anchor: Option<Vec<u32>> },
    Scalar {
        anchor: Option<Vec<u32>>,
        tag: Option<Vec<u32>>,
        implicit: (bool, bool),
        value: YStr,
        // Raw event style (`None`/`''`/single/multi-char verbatim).
        style: Option<String>,
    },
    SequenceStart {
        anchor: Option<Vec<u32>>,
        tag: Option<Vec<u32>>,
        implicit: bool,
        flow: bool,
    },
    SequenceEnd,
    MappingStart {
        anchor: Option<Vec<u32>>,
        tag: Option<Vec<u32>>,
        implicit: bool,
        flow: bool,
    },
    MappingEnd,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmitOptions {
    pub canonical: bool,
    pub indent: Option<i64>,
    pub width: Option<i64>,
    pub allow_unicode: bool,
    pub line_break: Option<String>,
}

/// Emitter fault: a template plus args rendered facade-side into
/// `EmitterError`. `%s`-of-event templates take the oldest unprocessed
/// event (retained facade-side); `processed` tells the facade how many
/// retained events completed before the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitError {
    pub problem: Formatted,
    pub processed: usize,
}

impl EmitError {
    fn new(tpl: &str) -> Self {
        EmitError { problem: Formatted::new(tpl), processed: 0 }
    }
    fn arg(mut self, a: ErrArg) -> Self {
        self.problem.args.push(a);
        self
    }
}

/// ASCII alphanumerics on code points.
fn is_ascii_alphanumeric(ch: u32) -> bool {
    (b'0' as u32..=b'9' as u32).contains(&ch)
        || (b'A' as u32..=b'Z' as u32).contains(&ch)
        || (b'a' as u32..=b'z' as u32).contains(&ch)
}

/// The emitter's tag-special set: `-;/?:@&=+$,_.~*'()[]`.
fn is_tag_special(ch: u32) -> bool {
    matches!(
        ch,
        0x2D | 0x3B | 0x2F | 0x3F | 0x3A | 0x40 | 0x26 | 0x3D | 0x2B | 0x24 | 0x2C
        | 0x5F | 0x2E | 0x7E | 0x2A | 0x27 | 0x28 | 0x29 | 0x5B | 0x5D
    )
}

/// Validated-ASCII code points to text (prepared tags/anchors/directives
/// are ASCII by construction).
fn ascii_string(v: &[u32]) -> String {
    v.iter().map(|&c| char::from_u32(c).expect("prepared text is ASCII")).collect()
}

/// Percent-encode one code point as its UTF-8 bytes (`%XX` per byte).
/// Surrogates cannot encode: mirrors `UnicodeEncodeError("utf-8", ...,
/// "surrogates not allowed")` with the exact message template (the
/// original encodes per character, so position is always 0).
fn utf8_percent_encode(ch: u32) -> Result<Vec<u32>, EmitError> {
    if (0xD800..0xE000).contains(&ch) {
        return Err(EmitError::new(
            "'utf-8' codec can't encode character %r in position %d: surrogates not allowed",
        )
        .arg(ErrArg::Char(ch))
        .arg(ErrArg::Int(0)));
    }
    let c = char::from_u32(ch).expect("tag code point is a valid scalar");
    let mut buf = [0u8; 4];
    let mut out = Vec::new();
    for b in c.encode_utf8(&mut buf).bytes() {
        out.push(0x25);
        for d in format!("{b:02X}").chars() {
            out.push(d as u32);
        }
    }
    Ok(out)
}

/// Render a writer segment. Segments never hold lone surrogates: any scalar
/// containing one is forced double-quoted by analysis, and anchors/handles
/// are ASCII-validated, so `expect` only fires on a core bug (loud, never
/// silent corruption).
fn u32s_to_string(v: &[u32]) -> String {
    v.iter().map(|&c| char::from_u32(c).expect("emitter segment holds valid scalars")).collect()
}

// ---------------------------------------------------------------------------
// Scalar analysis.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ScalarAnalysis {
    scalar: Vec<u32>,
    empty: bool,
    multiline: bool,
    allow_flow_plain: bool,
    allow_block_plain: bool,
    allow_single_quoted: bool,
    // `allow_double_quoted` is always true in the original (never cleared).
    allow_block: bool,
}

// ---------------------------------------------------------------------------
// Emitter states.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    StreamStart,
    Nothing,
    FirstDocumentStart,
    DocumentStart { first: bool },
    DocumentEnd,
    DocumentRoot,
    NodeRoot,
    Node { sequence: bool, mapping: bool, simple_key: bool },
    FirstFlowSequenceItem,
    FlowSequenceItem,
    FirstFlowMappingKey,
    FlowMappingKey,
    FlowMappingSimpleValue,
    FlowMappingValue,
    FirstBlockSequenceItem,
    BlockSequenceItem { first: bool },
    FirstBlockMappingKey,
    BlockMappingKey { first: bool },
    BlockMappingSimpleValue,
    BlockMappingValue,
}

#[derive(Debug, Clone)]
pub struct EmitCore {
    output: String,
    drained: usize,
    states: Vec<State>,
    state: State,
    events: Vec<EventIn>,
    event: Option<EventIn>,
    indents: Vec<Option<i64>>,
    indent: Option<i64>,
    flow_level: i64,
    root_context: bool,
    sequence_context: bool,
    mapping_context: bool,
    simple_key_context: bool,
    line: usize,
    column: usize,
    whitespace: bool,
    indention: bool,
    open_ended: bool,
    canonical: bool,
    allow_unicode: bool,
    best_indent: i64,
    best_width: i64,
    best_line_break: String,
    tag_prefixes: Option<Vec<(Vec<u32>, Vec<u32>)>>,
    prepared_anchor: Option<Vec<u32>>,
    prepared_tag: Option<Vec<u32>>,
    analysis: Option<ScalarAnalysis>,
    style: Option<char>,
}

/// Default tag prefixes as code points: `!` -> `!`, `tag:yaml.org,2002:` -> `!!`.
fn default_tag_prefixes() -> Vec<(Vec<u32>, Vec<u32>)> {
    vec![
        (vec![0x21], vec![0x21]),
        (
            "tag:yaml.org,2002:".chars().map(|c| c as u32).collect(),
            vec![0x21, 0x21],
        ),
    ]
}

impl EmitCore {
    pub fn new(options: EmitOptions) -> Self {
        let best_indent =
            if let Some(i) = options.indent { if i > 1 && i < 10 { i } else { 2 } } else { 2 };
        let best_width = if let Some(w) = options.width {
            if w > best_indent * 2 { w } else { 80 }
        } else {
            80
        };
        let best_line_break = match options.line_break.as_deref() {
            Some("\r") | Some("\n") | Some("\r\n") => {
                options.line_break.clone().unwrap()
            }
            _ => "\n".to_string(),
        };
        EmitCore {
            canonical: options.canonical,
            allow_unicode: options.allow_unicode,
            output: String::new(),
            drained: 0,
            states: Vec::new(),
            state: State::StreamStart,
            events: Vec::new(),
            event: None,
            indents: Vec::new(),
            indent: None,
            flow_level: 0,
            root_context: false,
            sequence_context: false,
            mapping_context: false,
            simple_key_context: false,
            line: 0,
            column: 0,
            whitespace: true,
            indention: true,
            open_ended: false,
            best_indent,
            best_width,
            best_line_break,
            tag_prefixes: None,
            prepared_anchor: None,
            prepared_tag: None,
            analysis: None,
            style: None,
        }
    }

    pub fn dispose(&mut self) {
        self.states.clear();
        self.state = State::Nothing;
    }

    /// Queue one event, processing while ready. Returns the count of newly
    /// processed events (facade drops that many retained Python events).
    pub fn emit(&mut self, event: EventIn) -> Result<usize, EmitError> {
        self.events.push(event);
        let mut processed = 0;
        while !self.need_more_events() {
            let ev = self.events.remove(0);
            processed += 1;
            self.event = Some(ev);
            let st = self.state;
            if let Err(mut e) = self.run_state(st) {
                e.processed = processed - 1;
                self.event = None;
                return Err(e);
            }
            self.event = None;
        }
        Ok(processed)
    }

    /// Text accumulated since the last drain.
    pub fn drain(&mut self) -> String {
        let s = self.output[self.drained..].to_string();
        self.drained = self.output.len();
        s
    }

    fn need_more_events(&self) -> bool {
        if self.events.is_empty() {
            return true;
        }
        match &self.events[0] {
            EventIn::DocumentStart { .. } => self.need_events(1),
            EventIn::SequenceStart { .. } => self.need_events(2),
            EventIn::MappingStart { .. } => self.need_events(3),
            _ => false,
        }
    }

    fn need_events(&self, count: usize) -> bool {
        let mut level: i64 = 0;
        for event in self.events.iter().skip(1) {
            match event {
                EventIn::DocumentStart { .. }
                | EventIn::SequenceStart { .. }
                | EventIn::MappingStart { .. } => level += 1,
                EventIn::DocumentEnd { .. }
                | EventIn::SequenceEnd
                | EventIn::MappingEnd => level -= 1,
                EventIn::StreamEnd => level = -1,
                _ => {}
            }
            if level < 0 {
                return false;
            }
        }
        self.events.len() < count + 1
    }

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.indents.push(self.indent);
        if self.indent.is_none() {
            self.indent = Some(if flow { self.best_indent } else { 0 });
        } else if !indentless {
            self.indent = Some(self.indent.unwrap() + self.best_indent);
        }
    }

    fn current(&self) -> EventIn {
        self.event.clone().expect("emitter state without event")
    }

    fn run_state(&mut self, st: State) -> Result<(), EmitError> {
        match st {
            State::StreamStart => self.expect_stream_start(),
            State::Nothing => self.expect_nothing(),
            State::FirstDocumentStart => self.expect_document_start(true),
            State::DocumentStart { first } => self.expect_document_start(first),
            State::DocumentEnd => self.expect_document_end(),
            State::DocumentRoot => self.expect_document_root(),
            State::NodeRoot => self.expect_node(true, false, false, false),
            State::Node { sequence, mapping, simple_key } => {
                self.expect_node(false, sequence, mapping, simple_key)
            }
            State::FirstFlowSequenceItem => self.expect_first_flow_sequence_item(),
            State::FlowSequenceItem => self.expect_flow_sequence_item(),
            State::FirstFlowMappingKey => self.expect_first_flow_mapping_key(),
            State::FlowMappingKey => self.expect_flow_mapping_key(),
            State::FlowMappingSimpleValue => self.expect_flow_mapping_simple_value(),
            State::FlowMappingValue => self.expect_flow_mapping_value(),
            State::FirstBlockSequenceItem => self.expect_block_sequence_item(true),
            State::BlockSequenceItem { first } => self.expect_block_sequence_item(first),
            State::FirstBlockMappingKey => self.expect_block_mapping_key(true),
            State::BlockMappingKey { first } => self.expect_block_mapping_key(first),
            State::BlockMappingSimpleValue => self.expect_block_mapping_simple_value(),
            State::BlockMappingValue => self.expect_block_mapping_value(),
        }
    }


    fn expect_stream_start(&mut self) -> Result<(), EmitError> {
        match self.current() {
            EventIn::StreamStart => {
                // BOM/byte encoding is facade-side; the core only orders text.
                self.state = State::FirstDocumentStart;
                Ok(())
            }
            _ => Err(EmitError::new("expected StreamStartEvent, but got %s")),
        }
    }

    fn expect_nothing(&mut self) -> Result<(), EmitError> {
        Err(EmitError::new("expected nothing, but got %s"))
    }

    fn expect_document_start(&mut self, first: bool) -> Result<(), EmitError> {
        match self.current() {
            EventIn::DocumentStart { explicit, version, tags } => {
                if (version.is_some() || tags.is_some()) && self.open_ended {
                    self.write_indicator("...", true, false, false);
                    self.write_indent();
                }
                if let Some((major, minor)) = &version {
                    let text = self.prepare_version(major, minor)?;
                    self.write_version_directive(&ascii_string(&text));
                }
                let mut prefixes: Vec<(Vec<u32>, Vec<u32>)> = default_tag_prefixes();
                if let Some(t) = &tags {
                    let mut handles: Vec<&Vec<u32>> = t.iter().map(|(h, _)| h).collect();
                    handles.sort();
                    for handle in handles {
                        let prefix =
                            t.iter().find(|(h, _)| h == handle).map(|(_, p)| p.clone()).unwrap();
                        prefixes.push((prefix.clone(), (*handle).clone()));
                        let handle_text = self.prepare_tag_handle(handle)?;
                        let prefix_text = Self::prepare_tag_prefix(&prefix)?;
                        self.write_tag_directive(&ascii_string(&handle_text), &ascii_string(&prefix_text));
                    }
                }
                self.tag_prefixes = Some(prefixes);
                let implicit = first
                    && !explicit
                    && !self.canonical
                    && version.is_none()
                    && tags.is_none()
                    && !self.check_empty_document();
                if !implicit {
                    self.write_indent();
                    self.write_indicator("---", true, false, false);
                    if self.canonical {
                        self.write_indent();
                    }
                }
                self.state = State::DocumentRoot;
                Ok(())
            }
            EventIn::StreamEnd => {
                if self.open_ended {
                    self.write_indicator("...", true, false, false);
                    self.write_indent();
                }
                // Flush is facade-side (stream ownership lives in Python).
                self.state = State::Nothing;
                Ok(())
            }
            _ => Err(EmitError::new("expected DocumentStartEvent, but got %s")),
        }
    }

    fn expect_document_end(&mut self) -> Result<(), EmitError> {
        match self.current() {
            EventIn::DocumentEnd { explicit } => {
                self.write_indent();
                if explicit {
                    self.write_indicator("...", true, false, false);
                    self.write_indent();
                }
                self.state = State::DocumentStart { first: false };
                Ok(())
            }
            _ => Err(EmitError::new("expected DocumentEndEvent, but got %s")),
        }
    }

    fn expect_document_root(&mut self) -> Result<(), EmitError> {
        self.states.push(State::DocumentEnd);
        self.state = State::NodeRoot;
        self.expect_node(true, false, false, false)
    }

    fn expect_node(
        &mut self,
        root: bool,
        sequence: bool,
        mapping: bool,
        simple_key: bool,
    ) -> Result<(), EmitError> {
        self.root_context = root;
        self.sequence_context = sequence;
        self.mapping_context = mapping;
        self.simple_key_context = simple_key;
        match self.current() {
            EventIn::Alias { .. } => self.expect_alias(),
            EventIn::Scalar { .. }
            | EventIn::SequenceStart { .. }
            | EventIn::MappingStart { .. } => {
                self.process_anchor('&')?;
                self.process_tag()?;
                match self.current() {
                    EventIn::Scalar { .. } => self.expect_scalar(),
                    EventIn::SequenceStart { flow, .. } => {
                        if self.flow_level > 0 || self.canonical || flow || self.check_empty_sequence()
                        {
                            self.expect_flow_sequence()?;
                        } else {
                            self.expect_block_sequence()?;
                        }
                        Ok(())
                    }
                    EventIn::MappingStart { flow, .. } => {
                        if self.flow_level > 0 || self.canonical || flow || self.check_empty_mapping()
                        {
                            self.expect_flow_mapping()?;
                        } else {
                            self.expect_block_mapping()?;
                        }
                        Ok(())
                    }
                    _ => unreachable!(),
                }
            }
            _ => Err(EmitError::new("expected NodeEvent, but got %s")),
        }
    }

    fn expect_alias(&mut self) -> Result<(), EmitError> {
        match self.current() {
            EventIn::Alias { anchor: None } => {
                Err(EmitError::new("anchor is not specified for alias"))
            }
            EventIn::Alias { anchor: Some(anchor) } => {
                self.write_anchor('*', &anchor)?;
                self.state = self.states.pop().unwrap_or(State::Nothing);
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    fn expect_scalar(&mut self) -> Result<(), EmitError> {
        self.increase_indent(true, false);
        self.process_scalar()?;
        self.indent = self.indents.pop().unwrap_or(None);
        self.state = self.states.pop().unwrap_or(State::Nothing);
        Ok(())
    }

    fn expect_flow_sequence(&mut self) -> Result<(), EmitError> {
        self.write_indicator("[", true, true, false);
        self.flow_level += 1;
        self.increase_indent(true, false);
        self.state = State::FirstFlowSequenceItem;
        Ok(())
    }

    fn expect_first_flow_sequence_item(&mut self) -> Result<(), EmitError> {
        if matches!(self.current(), EventIn::SequenceEnd) {
            self.indent = self.indents.pop().unwrap_or(None);
            self.flow_level -= 1;
            self.write_indicator("]", false, false, false);
            self.state = self.states.pop().unwrap_or(State::Nothing);
            Ok(())
        } else {
            if self.canonical || self.column as i64 > self.best_width {
                self.write_indent();
            }
            self.states.push(State::FlowSequenceItem);
            self.state = State::Node { sequence: true, mapping: false, simple_key: false };
            self.expect_node(false, true, false, false)
        }
    }

    fn expect_flow_sequence_item(&mut self) -> Result<(), EmitError> {
        if matches!(self.current(), EventIn::SequenceEnd) {
            self.indent = self.indents.pop().unwrap_or(None);
            self.flow_level -= 1;
            if self.canonical {
                self.write_indicator(",", false, false, false);
                self.write_indent();
            }
            self.write_indicator("]", false, false, false);
            self.state = self.states.pop().unwrap_or(State::Nothing);
            Ok(())
        } else {
            self.write_indicator(",", false, false, false);
            if self.canonical || self.column as i64 > self.best_width {
                self.write_indent();
            }
            self.states.push(State::FlowSequenceItem);
            self.state = State::Node { sequence: true, mapping: false, simple_key: false };
            self.expect_node(false, true, false, false)
        }
    }

    fn expect_flow_mapping(&mut self) -> Result<(), EmitError> {
        self.write_indicator("{", true, true, false);
        self.flow_level += 1;
        self.increase_indent(true, false);
        self.state = State::FirstFlowMappingKey;
        Ok(())
    }

    fn expect_first_flow_mapping_key(&mut self) -> Result<(), EmitError> {
        if matches!(self.current(), EventIn::MappingEnd) {
            self.indent = self.indents.pop().unwrap_or(None);
            self.flow_level -= 1;
            self.write_indicator("}", false, false, false);
            self.state = self.states.pop().unwrap_or(State::Nothing);
            Ok(())
        } else {
            if self.canonical || self.column as i64 > self.best_width {
                self.write_indent();
            }
            if !self.canonical && self.check_simple_key()? {
                self.states.push(State::FlowMappingSimpleValue);
                self.state = State::Node { sequence: false, mapping: true, simple_key: true };
                self.expect_node(false, false, true, true)
            } else {
                self.write_indicator("?", true, false, false);
                self.states.push(State::FlowMappingValue);
                self.state = State::Node { sequence: false, mapping: true, simple_key: false };
                self.expect_node(false, false, true, false)
            }
        }
    }

    fn expect_flow_mapping_key(&mut self) -> Result<(), EmitError> {
        if matches!(self.current(), EventIn::MappingEnd) {
            self.indent = self.indents.pop().unwrap_or(None);
            self.flow_level -= 1;
            if self.canonical {
                self.write_indicator(",", false, false, false);
                self.write_indent();
            }
            self.write_indicator("}", false, false, false);
            self.state = self.states.pop().unwrap_or(State::Nothing);
            Ok(())
        } else {
            self.write_indicator(",", false, false, false);
            if self.canonical || self.column as i64 > self.best_width {
                self.write_indent();
            }
            if !self.canonical && self.check_simple_key()? {
                self.states.push(State::FlowMappingSimpleValue);
                self.state = State::Node { sequence: false, mapping: true, simple_key: true };
                self.expect_node(false, false, true, true)
            } else {
                self.write_indicator("?", true, false, false);
                self.states.push(State::FlowMappingValue);
                self.state = State::Node { sequence: false, mapping: true, simple_key: false };
                self.expect_node(false, false, true, false)
            }
        }
    }

    fn expect_flow_mapping_simple_value(&mut self) -> Result<(), EmitError> {
        self.write_indicator(":", false, false, false);
        self.states.push(State::FlowMappingKey);
        self.state = State::Node { sequence: false, mapping: true, simple_key: false };
        self.expect_node(false, false, true, false)
    }

    fn expect_flow_mapping_value(&mut self) -> Result<(), EmitError> {
        if self.canonical || self.column as i64 > self.best_width {
            self.write_indent();
        }
        self.write_indicator(":", true, false, false);
        self.states.push(State::FlowMappingKey);
        self.state = State::Node { sequence: false, mapping: true, simple_key: false };
        self.expect_node(false, false, true, false)
    }

    fn expect_block_sequence(&mut self) -> Result<(), EmitError> {
        let indentless = self.mapping_context && !self.indention;
        self.increase_indent(false, indentless);
        self.state = State::FirstBlockSequenceItem;
        Ok(())
    }

    fn expect_block_sequence_item(&mut self, first: bool) -> Result<(), EmitError> {
        if !first && matches!(self.current(), EventIn::SequenceEnd) {
            self.indent = self.indents.pop().unwrap_or(None);
            self.state = self.states.pop().unwrap_or(State::Nothing);
            Ok(())
        } else {
            self.write_indent();
            self.write_indicator("-", true, false, true);
            self.states.push(State::BlockSequenceItem { first: false });
            self.state = State::Node { sequence: true, mapping: false, simple_key: false };
            self.expect_node(false, true, false, false)
        }
    }

    fn expect_block_mapping(&mut self) -> Result<(), EmitError> {
        self.increase_indent(false, false);
        self.state = State::FirstBlockMappingKey;
        Ok(())
    }

    fn expect_block_mapping_key(&mut self, first: bool) -> Result<(), EmitError> {
        if !first && matches!(self.current(), EventIn::MappingEnd) {
            self.indent = self.indents.pop().unwrap_or(None);
            self.state = self.states.pop().unwrap_or(State::Nothing);
            Ok(())
        } else {
            self.write_indent();
            if self.check_simple_key()? {
                self.states.push(State::BlockMappingSimpleValue);
                self.state = State::Node { sequence: false, mapping: true, simple_key: true };
                self.expect_node(false, false, true, true)
            } else {
                self.write_indicator("?", true, false, true);
                self.states.push(State::BlockMappingValue);
                self.state = State::Node { sequence: false, mapping: true, simple_key: false };
                self.expect_node(false, false, true, false)
            }
        }
    }

    fn expect_block_mapping_simple_value(&mut self) -> Result<(), EmitError> {
        self.write_indicator(":", false, false, false);
        self.states.push(State::BlockMappingKey { first: false });
        self.state = State::Node { sequence: false, mapping: true, simple_key: false };
        self.expect_node(false, false, true, false)
    }

    fn expect_block_mapping_value(&mut self) -> Result<(), EmitError> {
        self.write_indent();
        self.write_indicator(":", true, false, true);
        self.states.push(State::BlockMappingKey { first: false });
        self.state = State::Node { sequence: false, mapping: true, simple_key: false };
        self.expect_node(false, false, true, false)
    }

    // -- checkers -------------------------------------------------------------

    fn check_empty_sequence(&self) -> bool {
        matches!(self.event, Some(EventIn::SequenceStart { .. }))
            && matches!(self.events.first(), Some(EventIn::SequenceEnd))
    }

    fn check_empty_mapping(&self) -> bool {
        matches!(self.event, Some(EventIn::MappingStart { .. }))
            && matches!(self.events.first(), Some(EventIn::MappingEnd))
    }

    fn check_empty_document(&self) -> bool {
        if !matches!(self.event, Some(EventIn::DocumentStart { .. })) || self.events.is_empty() {
            return false;
        }
        match self.events.first() {
            // `implicit` is a non-empty tuple on every scalar, hence always
            // truthy in the original: only anchor/tag/value decide.
            Some(EventIn::Scalar { anchor: None, tag: None, value, .. }) => {
                value.is_empty()
            }
            _ => false,
        }
    }

    fn check_simple_key(&mut self) -> Result<bool, EmitError> {
        let mut length: usize = 0;
        if let Some(ev) = self.event.clone() {
            if let EventIn::Alias { anchor: Some(anchor) } = &ev {
                if self.prepared_anchor.is_none() {
                    // Mirrors the original: preparation validates and raises.
                    self.prepared_anchor = Some(Self::prepare_anchor(anchor)?);
                }
                length += self.prepared_anchor.as_ref().map(|s| s.len()).unwrap_or(0);
            }
            let tag = match &ev {
                EventIn::Scalar { tag: Some(t), .. } => Some(t),
                EventIn::SequenceStart { tag: Some(t), .. } => Some(t),
                EventIn::MappingStart { tag: Some(t), .. } => Some(t),
                _ => None,
            };
            if let Some(t) = tag {
                if self.prepared_tag.is_none() {
                    self.prepared_tag = Some(self.prepare_tag(t)?);
                }
                length += self.prepared_tag.as_ref().map(|s| s.len()).unwrap_or(0);
            }
            if let EventIn::Scalar { value, .. } = &ev {
                if self.analysis.is_none() {
                    self.analysis = Some(self.analyze_scalar(&value.0));
                }
                length += self.analysis.as_ref().map(|a| a.scalar.len()).unwrap_or(0);
            }
        }
        Ok(length < 128
            && match &self.event {
                Some(EventIn::Alias { .. }) => true,
                Some(EventIn::Scalar { .. }) => {
                    let a = self.analysis.as_ref().unwrap();
                    !a.empty && !a.multiline
                }
                _ => self.check_empty_sequence() || self.check_empty_mapping(),
            })
    }

    // -- anchor/tag/scalar processors ------------------------------------------

    fn process_anchor(&mut self, indicator: char) -> Result<(), EmitError> {
        let anchor: Option<Vec<u32>> = match self.current() {
            EventIn::Alias { anchor } => anchor,
            EventIn::Scalar { anchor, .. } => anchor,
            EventIn::SequenceStart { anchor, .. } => anchor,
            EventIn::MappingStart { anchor, .. } => anchor,
            _ => None,
        };
        match anchor {
            None => {
                self.prepared_anchor = None;
                Ok(())
            }
            Some(a) => self.write_anchor(indicator, &a),
        }
    }

    fn write_anchor(&mut self, indicator: char, anchor: &[u32]) -> Result<(), EmitError> {
        if self.prepared_anchor.is_none() {
            self.prepared_anchor = Some(Self::prepare_anchor(anchor)?);
        }
        let text = self.prepared_anchor.clone().unwrap_or_default();
        if !text.is_empty() {
            let mut s = String::new();
            s.push(indicator);
            s.push_str(&ascii_string(&text));
            self.write_indicator(&s, true, false, false);
        }
        self.prepared_anchor = None;
        Ok(())
    }

    fn process_tag(&mut self) -> Result<(), EmitError> {
        let (is_scalar, tag) = match self.current() {
            EventIn::Scalar { tag, .. } => (true, tag),
            EventIn::SequenceStart { tag, .. } => (false, tag),
            EventIn::MappingStart { tag, .. } => (false, tag),
            _ => (false, None),
        };
        if is_scalar {
            if self.style.is_none() {
                self.style = Some(self.choose_scalar_style()?);
            }
            let style = self.style.unwrap();
            let implicit = match self.current() {
                EventIn::Scalar { implicit, .. } => implicit,
                _ => (false, false),
            };
            if (!self.canonical || tag.is_none())
                && ((style == '\0' && implicit.0) || (style != '\0' && implicit.1))
            {
                self.prepared_tag = None;
                return Ok(());
            }
            let mut tagv = tag;
            if implicit.0 && tagv.is_none() {
                tagv = Some(vec![0x21]);
                self.prepared_tag = None;
            }
            if tagv.is_none() {
                return Err(EmitError::new("tag is not specified"));
            }
            if self.prepared_tag.is_none() {
                let t = tagv.take().unwrap();
                self.prepared_tag = Some(self.prepare_tag(&t)?);
            }
        } else {
            let implicit = match self.current() {
                EventIn::SequenceStart { implicit, .. } => implicit,
                EventIn::MappingStart { implicit, .. } => implicit,
                _ => false,
            };
            if (!self.canonical || tag.is_none()) && implicit {
                self.prepared_tag = None;
                return Ok(());
            }
            let mut tagv = tag;
            if tagv.is_none() {
                return Err(EmitError::new("tag is not specified"));
            }
            if self.prepared_tag.is_none() {
                let t = tagv.take().unwrap();
                self.prepared_tag = Some(self.prepare_tag(&t)?);
            }
        }
        let text = self.prepared_tag.clone().unwrap_or_default();
        if !text.is_empty() {
            self.write_indicator(&ascii_string(&text), true, false, false);
        }
        self.prepared_tag = None;
        Ok(())
    }

    fn choose_scalar_style(&mut self) -> Result<char, EmitError> {
        let (value, estyle, implicit) = match self.current() {
            EventIn::Scalar { value, style, implicit, .. } => (value, style, implicit),
            _ => unreachable!(),
        };
        if self.analysis.is_none() {
            self.analysis = Some(self.analyze_scalar(&value.0));
        }
        let a = self.analysis.clone().unwrap();
        // `estyle` compares as a whole string in the original (`''` is
        // falsy-plain; multi-char styles match nothing and fall to '"').
        let es = estyle.as_deref();
        if es == Some("\"") || self.canonical {
            return Ok('"');
        }
        let plain_event = es.is_none() || es == Some("");
        if plain_event
            && implicit.0
            && !(self.simple_key_context && (a.empty || a.multiline))
            && ((self.flow_level > 0 && a.allow_flow_plain)
                || (self.flow_level == 0 && a.allow_block_plain))
        {
            return Ok('\0');
        }
        if (es == Some("|") || es == Some(">"))
            && self.flow_level == 0
            && !self.simple_key_context
            && a.allow_block
        {
            return Ok(es.unwrap().chars().next().unwrap());
        }
        if (plain_event || es == Some("'"))
            && a.allow_single_quoted
            && !(self.simple_key_context && a.multiline)
        {
            return Ok('\'');
        }
        Ok('"')
    }

    fn process_scalar(&mut self) -> Result<(), EmitError> {
        let value = match self.current() {
            EventIn::Scalar { value, .. } => value,
            _ => unreachable!(),
        };
        if self.analysis.is_none() {
            self.analysis = Some(self.analyze_scalar(&value.0));
        }
        if self.style.is_none() {
            self.style = Some(self.choose_scalar_style()?);
        }
        let split = !self.simple_key_context;
        let a = self.analysis.clone().unwrap();
        match self.style.unwrap() {
            '"' => self.write_double_quoted(&a.scalar, split),
            '\'' => self.write_single_quoted(&a.scalar, split),
            '>' => self.write_folded(&a.scalar),
            '|' => self.write_literal(&a.scalar),
            _ => self.write_plain(&a.scalar, split),
        }
        self.analysis = None;
        self.style = None;
        Ok(())
    }

    // -- analyzers ---------------------------------------------------------------

    fn prepare_version(&self, major: &str, minor: &str) -> Result<Vec<u32>, EmitError> {
        if major != "1" {
            return Err(EmitError::new("unsupported YAML version: %d.%d")
                .arg(ErrArg::Str(major.to_string()))
                .arg(ErrArg::Str(minor.to_string())));
        }
        Ok(format!("{major}.{minor}").chars().map(|c| c as u32).collect())
    }

    fn prepare_tag_handle(&self, handle: &[u32]) -> Result<Vec<u32>, EmitError> {
        if handle.is_empty() {
            return Err(EmitError::new("tag handle must not be empty"));
        }
        if handle[0] != 0x21 || handle[handle.len() - 1] != 0x21 {
            return Err(EmitError::new("tag handle must start and end with '!': %r")
                .arg(ErrArg::Codes(handle.to_vec())));
        }
        // Python slices clamp (`'!'[1:-1]` is empty); guard the len-1 case.
        for ch in handle.get(1..handle.len().saturating_sub(1)).unwrap_or(&[]) {
            if !is_ascii_alphanumeric(*ch) && *ch != 0x2D && *ch != 0x5F {
                return Err(EmitError::new("invalid character %r in the tag handle: %r")
                    .arg(ErrArg::Char(*ch))
                    .arg(ErrArg::Codes(handle.to_vec())));
            }
        }
        Ok(handle.to_vec())
    }

    fn prepare_tag_prefix(prefix: &[u32]) -> Result<Vec<u32>, EmitError> {
        if prefix.is_empty() {
            return Err(EmitError::new("tag prefix must not be empty"));
        }
        let mut chunks: Vec<u32> = Vec::new();
        let mut start = 0;
        let mut end = if prefix[0] == 0x21 { 1 } else { 0 };
        while end < prefix.len() {
            let ch = prefix[end];
            if is_ascii_alphanumeric(ch) || is_tag_special(ch) {
                end += 1;
            } else {
                chunks.extend_from_slice(&prefix[start..end]);
                start = end + 1;
                end = start;
                chunks.extend_from_slice(&utf8_percent_encode(ch)?);
            }
        }
        chunks.extend_from_slice(&prefix[start..end]);
        Ok(chunks)
    }

    fn prepare_tag(&self, tag: &[u32]) -> Result<Vec<u32>, EmitError> {
        if tag.is_empty() {
            return Err(EmitError::new("tag must not be empty"));
        }
        if tag == [0x21].as_slice() {
            return Ok(vec![0x21]);
        }
        let prefixes = self.tag_prefixes.clone().unwrap_or_default();
        let mut keys: Vec<&Vec<u32>> = prefixes.iter().map(|(p, _)| p).collect();
        keys.sort();
        let mut handle: Option<Vec<u32>> = None;
        let mut suffix = tag.to_vec();
        for prefix in keys {
            if tag.starts_with(prefix) && (prefix.as_slice() == [0x21].as_slice() || prefix.len() < tag.len()) {
                handle = prefixes.iter().find(|(p, _)| p == prefix).map(|(_, h)| h.clone());
                suffix = tag[prefix.len()..].to_vec();
            }
        }
        let mut chunks: Vec<u32> = Vec::new();
        let mut start = 0;
        let mut end = 0;
        while end < suffix.len() {
            let ch = suffix[end];
            if is_ascii_alphanumeric(ch)
                || is_tag_special(ch)
            || (ch == 0x21 && handle.as_deref() != Some([0x21].as_slice()))
            {
                end += 1;
            } else {
                chunks.extend_from_slice(&suffix[start..end]);
                start = end + 1;
                end = start;
                chunks.extend_from_slice(&utf8_percent_encode(ch)?);
            }
        }
        chunks.extend_from_slice(&suffix[start..end]);
        if let Some(h) = handle {
            let mut out = h;
            out.extend_from_slice(&chunks);
            Ok(out)
        } else {
            let mut out = vec![0x21, 0x3C];
            out.extend_from_slice(&chunks);
            out.push(0x3E);
            Ok(out)
        }
    }

    fn prepare_anchor(anchor: &[u32]) -> Result<Vec<u32>, EmitError> {
        if anchor.is_empty() {
            return Err(EmitError::new("anchor must not be empty"));
        }
        for ch in anchor {
            if !is_ascii_alphanumeric(*ch) && *ch != 0x2D && *ch != 0x5F {
                return Err(EmitError::new("invalid character %r in the anchor: %r")
                    .arg(ErrArg::Char(*ch))
                    .arg(ErrArg::Codes(anchor.to_vec())));
            }
        }
        Ok(anchor.to_vec())
    }

    fn analyze_scalar(&self, scalar: &[u32]) -> ScalarAnalysis {
        if scalar.is_empty() {
            return ScalarAnalysis {
                scalar: Vec::new(),
                empty: true,
                multiline: false,
                allow_flow_plain: false,
                allow_block_plain: true,
                allow_single_quoted: true,
                allow_block: false,
            };
        }
        let mut block_indicators = false;
        let mut flow_indicators = false;
        let mut line_breaks = false;
        let mut special_characters = false;
        let mut leading_space = false;
        let mut leading_break = false;
        let mut trailing_space = false;
        let mut trailing_break = false;
        let mut break_space = false;
        let mut space_break = false;
        if scalar.len() >= 3 && scalar[0] == 0x2D && scalar[1] == 0x2D && scalar[2] == 0x2D {
            block_indicators = true;
            flow_indicators = true;
        }
        if scalar.len() >= 3 && scalar[0] == 0x2E && scalar[1] == 0x2E && scalar[2] == 0x2E {
            block_indicators = true;
            flow_indicators = true;
        }
        let mut preceded_by_whitespace = true;
        let mut followed_by_whitespace = scalar.len() == 1
            || matches!(scalar.get(1), Some(0) | Some(0x20) | Some(0x09) | Some(0x0D) | Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029));
        let mut previous_space = false;
        let mut previous_break = false;
        let mut index = 0;
        while index < scalar.len() {
            let ch = scalar[index];
            if index == 0 {
                if matches!(
                    ch,
                    0x23 | 0x2C | 0x5B | 0x5D | 0x7B | 0x7D | 0x26 | 0x2A | 0x21 | 0x7C
                    | 0x3E | 0x27 | 0x22 | 0x25 | 0x40 | 0x60
                ) {
                    flow_indicators = true;
                    block_indicators = true;
                }
                if ch == 0x3F || ch == 0x3A {
                    flow_indicators = true;
                    if followed_by_whitespace {
                        block_indicators = true;
                    }
                }
                if ch == 0x2D && followed_by_whitespace {
                    flow_indicators = true;
                    block_indicators = true;
                }
            } else {
                if matches!(ch, 0x2C | 0x3F | 0x5B | 0x5D | 0x7B | 0x7D) {
                    flow_indicators = true;
                }
                if ch == 0x3A {
                    flow_indicators = true;
                    if followed_by_whitespace {
                        block_indicators = true;
                    }
                }
                if ch == 0x23 && preceded_by_whitespace {
                    flow_indicators = true;
                    block_indicators = true;
                }
            }
            if matches!(ch, 0x0A | 0x85 | 0x2028 | 0x2029) {
                line_breaks = true;
            }
            if !(ch == 0x0A || (0x20..=0x7E).contains(&ch)) {
                if (ch == 0x85 || (0xA0..=0xD7FF).contains(&ch) || (0xE000..=0xFFFD).contains(&ch)
                    || (0x10000..0x10FFFF).contains(&ch))
                    && ch != 0xFEFF
                {
                    if !self.allow_unicode {
                        special_characters = true;
                    }
                } else {
                    special_characters = true;
                }
            }
            if ch == 0x20 {
                if index == 0 {
                    leading_space = true;
                }
                if index == scalar.len() - 1 {
                    trailing_space = true;
                }
                if previous_break {
                    break_space = true;
                }
                previous_space = true;
                previous_break = false;
            } else if matches!(ch, 0x0A | 0x85 | 0x2028 | 0x2029) {
                if index == 0 {
                    leading_break = true;
                }
                if index == scalar.len() - 1 {
                    trailing_break = true;
                }
                if previous_space {
                    space_break = true;
                }
                previous_space = false;
                previous_break = true;
            } else {
                previous_space = false;
                previous_break = false;
            }
            index += 1;
            preceded_by_whitespace =
                matches!(ch, 0 | 0x20 | 0x09 | 0x0D | 0x0A | 0x85 | 0x2028 | 0x2029);
            followed_by_whitespace = index + 1 >= scalar.len()
                || matches!(
                    scalar.get(index + 1),
                    Some(0) | Some(0x20) | Some(0x09) | Some(0x0D) | Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)
                );
        }
        let mut allow_flow_plain = true;
        let mut allow_block_plain = true;
        let mut allow_single_quoted = true;
        let mut allow_block = true;
        if leading_space || leading_break || trailing_space || trailing_break {
            allow_flow_plain = false;
            allow_block_plain = false;
        }
        if trailing_space {
            allow_block = false;
        }
        if break_space {
            allow_flow_plain = false;
            allow_block_plain = false;
            allow_single_quoted = false;
        }
        if space_break || special_characters {
            allow_flow_plain = false;
            allow_block_plain = false;
            allow_single_quoted = false;
            allow_block = false;
        }
        if line_breaks {
            allow_flow_plain = false;
            allow_block_plain = false;
        }
        if flow_indicators {
            allow_flow_plain = false;
        }
        if block_indicators {
            allow_block_plain = false;
        }
        ScalarAnalysis {
            scalar: scalar.to_vec(),
            empty: false,
            multiline: line_breaks,
            allow_flow_plain,
            allow_block_plain,
            allow_single_quoted,
            allow_block,
        }
    }

    // -- writers -----------------------------------------------------------------


    fn write_indicator(&mut self, indicator: &str, need_whitespace: bool, whitespace: bool, indention: bool) {
        let data = if self.whitespace || !need_whitespace {
            indicator.to_string()
        } else {
            format!(" {indicator}")
        };
        self.whitespace = whitespace;
        self.indention = self.indention && indention;
        self.column += data.chars().count();
        self.open_ended = false;
        self.output.push_str(&data);
    }

    fn write_indent(&mut self) {
        let indent = self.indent.unwrap_or(0);
        if !self.indention || self.column as i64 > indent || (self.column as i64 == indent && !self.whitespace)
        {
            self.write_line_break(None);
        }
        if (self.column as i64) < indent {
            self.whitespace = true;
            for _ in self.column as i64..indent {
                self.output.push(' ');
            }
            self.column = indent as usize;
        }
    }

    fn write_line_break(&mut self, data: Option<&str>) {
        let br = data.map(|s| s.to_string()).unwrap_or_else(|| self.best_line_break.clone());
        self.whitespace = true;
        self.indention = true;
        self.line += 1;
        self.column = 0;
        self.output.push_str(&br);
    }

    fn write_version_directive(&mut self, version_text: &str) {
        self.output.push_str(&format!("%YAML {version_text}"));
        let br = self.best_line_break.clone();
        self.write_line_break(Some(&br));
    }

    fn write_tag_directive(&mut self, handle_text: &str, prefix_text: &str) {
        self.output.push_str(&format!("%TAG {handle_text} {prefix_text}"));
        let br = self.best_line_break.clone();
        self.write_line_break(Some(&br));
    }

    fn write_single_quoted(&mut self, text: &[u32], split: bool) {
        self.write_indicator("'", true, false, false);
        let mut spaces = false;
        let mut breaks = false;
        let mut start = 0;
        let mut end = 0;
        while end <= text.len() {
            let ch = if end < text.len() { Some(text[end]) } else { None };
            if spaces {
                if ch.is_none() || ch != Some(0x20) {
                    if start + 1 == end
                        && self.column as i64 > self.best_width
                        && split
                        && start != 0
                        && end != text.len()
                    {
                        self.write_indent();
                    } else {
                        let data: String = u32s_to_string(&text[start..end]);
                        self.column += data.chars().count();
                        self.output.push_str(&data);
                    }
                    start = end;
                }
            } else if breaks {
                if ch.is_none() || !matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                    if text[start] == 0x0A {
                        self.write_line_break(None);
                    }
                    for br in &text[start..end] {
                        if *br == 0x0A {
                            self.write_line_break(None);
                        } else {
                            let s: String =
                                char::from_u32(*br).map(|c| c.to_string()).unwrap_or_default();
                            self.write_line_break(Some(&s));
                        }
                    }
                    self.write_indent();
                    start = end;
                }
            } else if (ch.is_none() || ch == Some(0x20) || matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) || ch == Some(0x27))
                && start < end
            {
                let data: String = u32s_to_string(&text[start..end]);
                self.column += data.chars().count();
                self.output.push_str(&data);
                start = end;
            }
            if ch == Some(0x27) {
                self.column += 2;
                self.output.push_str("''");
                start = end + 1;
            }
            if let Some(c) = ch {
                spaces = c == 0x20;
                breaks = matches!(c, 0x0A | 0x85 | 0x2028 | 0x2029);
            }
            end += 1;
        }
        self.write_indicator("'", false, false, false);
    }

    fn escape_char(ch: u32) -> Option<String> {
        const REPLACEMENTS: &[(u32, char)] = &[
            (0x00, '0'),
            (0x07, 'a'),
            (0x08, 'b'),
            (0x09, 't'),
            (0x0A, 'n'),
            (0x0B, 'v'),
            (0x0C, 'f'),
            (0x0D, 'r'),
            (0x1B, 'e'),
            (0x22, '"'),
            (0x5C, '\\'),
            (0x85, 'N'),
            (0xA0, '_'),
            (0x2028, 'L'),
            (0x2029, 'P'),
        ];
        if let Some((_, e)) = REPLACEMENTS.iter().find(|&&(c, _)| c == ch) {
            return Some(format!("\\{e}"));
        }
        if ch <= 0xFF {
            return Some(format!("\\x{ch:02X}"));
        }
        if ch <= 0xFFFF {
            return Some(format!("\\u{ch:04X}"));
        }
        Some(format!("\\U{ch:08X}"))
    }

    fn needs_escape(ch: u32, allow_unicode: bool) -> bool {
        ch == 0x22
            || ch == 0x5C
            || ch == 0x85
            || ch == 0x2028
            || ch == 0x2029
            || ch == 0xFEFF
            || !((0x20..=0x7E).contains(&ch)
                || (allow_unicode && ((0xA0..=0xD7FF).contains(&ch) || (0xE000..=0xFFFD).contains(&ch))))
    }

    fn write_double_quoted(&mut self, text: &[u32], split: bool) {
        self.write_indicator("\"", true, false, false);
        let mut start = 0;
        let mut end = 0;
        while end <= text.len() {
            let ch = if end < text.len() { Some(text[end]) } else { None };
            if ch.is_none() || Self::needs_escape(ch.unwrap(), self.allow_unicode) {
                if start < end {
                    let data: String = u32s_to_string(&text[start..end]);
                    self.column += data.chars().count();
                    self.output.push_str(&data);
                    start = end;
                }
                if let Some(c) = ch {
                    if let Some(e) = Self::escape_char(c) {
                        self.column += e.len();
                        self.output.push_str(&e);
                    }
                    start = end + 1;
                }
            }
            // `0 < end < len(text)-1`: `end + 1 < len` is exact, including
            // empty text; the width arithmetic is signed like the original.
            if end > 0
                && end + 1 < text.len()
                && (ch == Some(0x20) || start >= end)
                && self.column as i64 + end as i64 - start as i64 > self.best_width
                && split
            {
                // Python slices clamp (`text[start:end]` with `start > end`
                // is empty after an escape set `start = end + 1`).
                let mut data: String = if start <= end {
                    u32s_to_string(&text[start..end])
                } else {
                    String::new()
                };
                data.push('\\');
                if start < end {
                    start = end;
                }
                self.column += data.chars().count();
                self.output.push_str(&data);
                self.write_indent();
                self.whitespace = false;
                self.indention = false;
                if text[start] == 0x20 {
                    self.column += 1;
                    self.output.push('\\');
                }
            }
            end += 1;
        }
        self.write_indicator("\"", false, false, false);
    }

    fn determine_block_hints(&self, text: &[u32]) -> String {
        let mut hints = String::new();
        if !text.is_empty() {
            if matches!(text[0], 0x20 | 0x0A | 0x85 | 0x2028 | 0x2029) {
                hints.push_str(&self.best_indent.to_string());
            }
            if !matches!(text[text.len() - 1], 0x0A | 0x85 | 0x2028 | 0x2029) {
                hints.push('-');
            } else if text.len() == 1 || matches!(text[text.len() - 2], 0x0A | 0x85 | 0x2028 | 0x2029) {
                hints.push('+');
            }
        }
        hints
    }

    fn write_folded(&mut self, text: &[u32]) {
        let hints = self.determine_block_hints(text);
        self.write_indicator(&format!(">{hints}"), true, false, false);
        if hints.ends_with('+') {
            self.open_ended = true;
        }
        self.write_line_break(None);
        let mut leading_space = true;
        let mut spaces = false;
        let mut breaks = true;
        let mut start = 0;
        let mut end = 0;
        while end <= text.len() {
            let ch = if end < text.len() { Some(text[end]) } else { None };
            if breaks {
                if ch.is_none() || !matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                    if !leading_space && ch.is_some() && ch != Some(0x20) && text[start] == 0x0A {
                        self.write_line_break(None);
                    }
                    leading_space = ch == Some(0x20);
                    for br in &text[start..end] {
                        if *br == 0x0A {
                            self.write_line_break(None);
                        } else {
                            let s: String =
                                char::from_u32(*br).map(|c| c.to_string()).unwrap_or_default();
                            self.write_line_break(Some(&s));
                        }
                    }
                    if ch.is_some() {
                        self.write_indent();
                    }
                    start = end;
                }
            } else if spaces {
                if ch != Some(0x20) {
                    if start + 1 == end && self.column as i64 > self.best_width {
                        self.write_indent();
                    } else {
                        let data: String = u32s_to_string(&text[start..end]);
                        self.column += data.chars().count();
                        self.output.push_str(&data);
                    }
                    start = end;
                }
            } else if ch.is_none() || ch == Some(0x20) || matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                let data: String = u32s_to_string(&text[start..end]);
                self.column += data.chars().count();
                self.output.push_str(&data);
                if ch.is_none() {
                    self.write_line_break(None);
                }
                start = end;
            }
            if let Some(c) = ch {
                breaks = matches!(c, 0x0A | 0x85 | 0x2028 | 0x2029);
                spaces = c == 0x20;
            }
            end += 1;
        }
    }

    fn write_literal(&mut self, text: &[u32]) {
        let hints = self.determine_block_hints(text);
        self.write_indicator(&format!("|{hints}"), true, false, false);
        if hints.ends_with('+') {
            self.open_ended = true;
        }
        self.write_line_break(None);
        let mut breaks = true;
        let mut start = 0;
        let mut end = 0;
        while end <= text.len() {
            let ch = if end < text.len() { Some(text[end]) } else { None };
            if breaks {
                if ch.is_none() || !matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                    for br in &text[start..end] {
                        if *br == 0x0A {
                            self.write_line_break(None);
                        } else {
                            let s: String =
                                char::from_u32(*br).map(|c| c.to_string()).unwrap_or_default();
                            self.write_line_break(Some(&s));
                        }
                    }
                    if ch.is_some() {
                        self.write_indent();
                    }
                    start = end;
                }
            } else if ch.is_none() || matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                let data: String = u32s_to_string(&text[start..end]);
                self.column += data.chars().count();
                self.output.push_str(&data);
                if ch.is_none() {
                    self.write_line_break(None);
                }
                start = end;
            }
            if let Some(c) = ch {
                breaks = matches!(c, 0x0A | 0x85 | 0x2028 | 0x2029);
            }
            end += 1;
        }
    }

    fn write_plain(&mut self, text: &[u32], split: bool) {
        if self.root_context {
            self.open_ended = true;
        }
        if text.is_empty() {
            return;
        }
        if !self.whitespace {
            self.column += 1;
            self.output.push(' ');
        }
        self.whitespace = false;
        self.indention = false;
        let mut spaces = false;
        let mut breaks = false;
        let mut start = 0;
        let mut end = 0;
        while end <= text.len() {
            let ch = if end < text.len() { Some(text[end]) } else { None };
            if spaces {
                if ch != Some(0x20) {
                    if start + 1 == end && self.column as i64 > self.best_width && split {
                        self.write_indent();
                        self.whitespace = false;
                        self.indention = false;
                    } else {
                        let data: String = u32s_to_string(&text[start..end]);
                        self.column += data.chars().count();
                        self.output.push_str(&data);
                    }
                    start = end;
                }
            } else if breaks {
                if !matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                    if text[start] == 0x0A {
                        self.write_line_break(None);
                    }
                    for br in &text[start..end] {
                        if *br == 0x0A {
                            self.write_line_break(None);
                        } else {
                            let s: String =
                                char::from_u32(*br).map(|c| c.to_string()).unwrap_or_default();
                            self.write_line_break(Some(&s));
                        }
                    }
                    self.write_indent();
                    self.whitespace = false;
                    self.indention = false;
                    start = end;
                }
            } else if ch.is_none() || ch == Some(0x20) || matches!(ch, Some(0x0A) | Some(0x85) | Some(0x2028) | Some(0x2029)) {
                let data: String = u32s_to_string(&text[start..end]);
                self.column += data.chars().count();
                self.output.push_str(&data);
                start = end;
            }
            if let Some(c) = ch {
                spaces = c == 0x20;
                breaks = matches!(c, 0x0A | 0x85 | 0x2028 | 0x2029);
            }
            end += 1;
        }
    }
}
