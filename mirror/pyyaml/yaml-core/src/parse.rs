//! YAML 1.1 parser: tokens to events.
//!
//! Faithful port of PyYAML 6.0.2 `lib/yaml/parser.py`. The Python version
//! threads bound methods through `state`/`states`; here states are an enum
//! (the `first` parameters ride as variants). Token pulls go through the
//! shared [`ScanCore`]. Tag handles keep insertion order in a `Vec` so
//! `DocumentStartEvent.tags` round-trips deterministically.

use super::scan::{DirectiveValue, ScanCore, Token, TokenKind};
use super::{EngineError, ErrArg, Formatted, MarkPos, YStr};

// ---------------------------------------------------------------------------
// Event descriptors.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    StreamStart { encoding: Option<String>, start: MarkPos, end: MarkPos },
    StreamEnd { start: MarkPos, end: MarkPos },
    DocumentStart {
        explicit: bool,
        version: Option<(String, String)>,
        tags: Option<Vec<(String, String)>>,
        start: MarkPos,
        end: MarkPos,
    },
    DocumentEnd { explicit: bool, start: MarkPos, end: MarkPos },
    Alias { anchor: String, start: MarkPos, end: MarkPos },
    Scalar {
        anchor: Option<String>,
        tag: Option<String>,
        implicit: (bool, bool),
        value: YStr,
        style: Option<char>,
        start: MarkPos,
        end: MarkPos,
    },
    SequenceStart {
        anchor: Option<String>,
        tag: Option<String>,
        implicit: bool,
        flow: bool,
        start: MarkPos,
        end: MarkPos,
    },
    SequenceEnd { start: MarkPos, end: MarkPos },
    MappingStart {
        anchor: Option<String>,
        tag: Option<String>,
        implicit: bool,
        flow: bool,
        start: MarkPos,
        end: MarkPos,
    },
    MappingEnd { start: MarkPos, end: MarkPos },
}

pub type ParseFault = EngineError;

// ---------------------------------------------------------------------------
// Parser.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    StreamStart,
    ImplicitDocumentStart,
    DocumentStart,
    DocumentEnd,
    DocumentContent,
    BlockNode,
    BlockSequenceFirstEntry,
    BlockSequenceEntry,
    IndentlessSequenceEntry,
    BlockMappingFirstKey,
    BlockMappingKey,
    BlockMappingValue,
    FlowSequenceFirstEntry,
    FlowSequenceEntry { first: bool },
    FlowSeqMapKey,
    FlowSeqMapValue,
    FlowSeqMapEnd,
    FlowMappingFirstKey,
    FlowMappingKey { first: bool },
    FlowMappingValue,
    FlowMappingEmptyValue,
}

#[derive(Debug, Clone)]
pub struct ParseCore {
    current: Option<Event>,
    yaml_version: Option<(String, String)>,
    tag_handles: Vec<(String, String)>,
    states: Vec<State>,
    marks: Vec<MarkPos>,
    state: Option<State>,
}

/// Directive processing outcome: optional YAML version plus the
/// tag-handle table (insertion-ordered) when directives were present.
type DirectiveResult = (Option<(String, String)>, Option<Vec<(String, String)>>);

/// Faults from event pulls: scan faults (token supply) or parse errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Scan(super::scan::ScanFault),
    Parse(EngineError),
}

impl From<EngineError> for Fault {
    fn from(e: EngineError) -> Self {
        Fault::Parse(e)
    }
}

impl From<super::scan::ScanFault> for Fault {
    fn from(e: super::scan::ScanFault) -> Self {
        Fault::Scan(e)
    }
}

fn token_id(tok: &Token) -> &'static str {
    match tok {
        Token::StreamStart { .. } => "<stream start>",
        Token::StreamEnd { .. } => "<stream end>",
        Token::Directive { .. } => "<directive>",
        Token::DocumentStart { .. } => "<document start>",
        Token::DocumentEnd { .. } => "<document end>",
        Token::BlockSequenceStart { .. } => "<block sequence start>",
        Token::BlockMappingStart { .. } => "<block mapping start>",
        Token::BlockEnd { .. } => "<block end>",
        Token::FlowSequenceStart { .. } => "[",
        Token::FlowMappingStart { .. } => "{",
        Token::FlowSequenceEnd { .. } => "]",
        Token::FlowMappingEnd { .. } => "}",
        Token::BlockEntry { .. } => "-",
        Token::FlowEntry { .. } => ",",
        Token::Key { .. } => "?",
        Token::Value { .. } => ":",
        Token::Alias { .. } => "<alias>",
        Token::Anchor { .. } => "<anchor>",
        Token::Tag { .. } => "<tag>",
        Token::Scalar { .. } => "<scalar>",
    }
}

fn token_start(tok: &Token) -> MarkPos {
    match tok {
        Token::StreamStart { start, .. } => *start,
        Token::StreamEnd { start, .. } => *start,
        Token::Directive { start, .. } => *start,
        Token::DocumentStart { start, .. } => *start,
        Token::DocumentEnd { start, .. } => *start,
        Token::BlockSequenceStart { start, .. } => *start,
        Token::BlockMappingStart { start, .. } => *start,
        Token::BlockEnd { start, .. } => *start,
        Token::FlowSequenceStart { start, .. } => *start,
        Token::FlowMappingStart { start, .. } => *start,
        Token::FlowSequenceEnd { start, .. } => *start,
        Token::FlowMappingEnd { start, .. } => *start,
        Token::BlockEntry { start, .. } => *start,
        Token::FlowEntry { start, .. } => *start,
        Token::Key { start, .. } => *start,
        Token::Value { start, .. } => *start,
        Token::Alias { start, .. } => *start,
        Token::Anchor { start, .. } => *start,
        Token::Tag { start, .. } => *start,
        Token::Scalar { start, .. } => *start,
    }
}

fn token_end(tok: &Token) -> MarkPos {
    match tok {
        Token::StreamStart { end, .. } => *end,
        Token::StreamEnd { end, .. } => *end,
        Token::Directive { end, .. } => *end,
        Token::DocumentStart { end, .. } => *end,
        Token::DocumentEnd { end, .. } => *end,
        Token::BlockSequenceStart { end, .. } => *end,
        Token::BlockMappingStart { end, .. } => *end,
        Token::BlockEnd { end, .. } => *end,
        Token::FlowSequenceStart { end, .. } => *end,
        Token::FlowMappingStart { end, .. } => *end,
        Token::FlowSequenceEnd { end, .. } => *end,
        Token::FlowMappingEnd { end, .. } => *end,
        Token::BlockEntry { end, .. } => *end,
        Token::FlowEntry { end, .. } => *end,
        Token::Key { end, .. } => *end,
        Token::Value { end, .. } => *end,
        Token::Alias { end, .. } => *end,
        Token::Anchor { end, .. } => *end,
        Token::Tag { end, .. } => *end,
        Token::Scalar { end, .. } => *end,
    }
}

impl Default for ParseCore {
    fn default() -> Self {
        Self::new()
    }
}

impl ParseCore {
    pub fn new() -> Self {
        ParseCore {
            current: None,
            yaml_version: None,
            tag_handles: Vec::new(),
            states: Vec::new(),
            marks: Vec::new(),
            state: Some(State::StreamStart),
        }
    }

    pub fn dispose(&mut self) {
        self.states.clear();
        self.state = None;
    }

    fn check_token(
        &self,
        scan: &mut ScanCore,
        kinds: &[TokenKind],
    ) -> Result<bool, super::scan::ScanFault> {
        let tok = scan.peek_token()?;
        Ok(tok.is_some_and(|t| kinds.contains(&t.kind())))
    }

    fn peek_token(
        &self,
        scan: &mut ScanCore,
    ) -> Result<Token, super::scan::ScanFault> {
        scan.peek_token().map(|o| {
            o.unwrap_or(Token::StreamEnd {
                start: MarkPos { index: 0, line: 0, column: 0 },
                end: MarkPos { index: 0, line: 0, column: 0 },
            })
        })
    }

    fn get_token(
        &self,
        scan: &mut ScanCore,
    ) -> Result<Token, super::scan::ScanFault> {
        scan.next_token().map(|o| {
            o.unwrap_or(Token::StreamEnd {
                start: MarkPos { index: 0, line: 0, column: 0 },
                end: MarkPos { index: 0, line: 0, column: 0 },
            })
        })
    }

    /// Pull one event (or `None` when disposed/exhausted).
    pub fn next_event(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Option<Event>, Fault> {
        if self.current.is_none() {
            if let Some(st) = self.state {
                let ev = self.run_state(st, scan)?;
                self.current = Some(ev);
            }
        }
        Ok(self.current.clone())
    }

    /// Consume the pending event.
    pub fn take_event(&mut self) -> Option<Event> {
        self.current.take()
    }

    fn run_state(
        &mut self,
        st: State,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        match st {
            State::StreamStart => self.parse_stream_start(scan),
            State::ImplicitDocumentStart => self.parse_implicit_document_start(scan),
            State::DocumentStart => self.parse_document_start(scan),
            State::DocumentEnd => self.parse_document_end(scan),
            State::DocumentContent => self.parse_document_content(scan),
            State::BlockNode => self.parse_node(scan, true, false),
            State::BlockSequenceFirstEntry => self.parse_block_sequence_first_entry(scan),
            State::BlockSequenceEntry => self.parse_block_sequence_entry(scan),
            State::IndentlessSequenceEntry => self.parse_indentless_sequence_entry(scan),
            State::BlockMappingFirstKey => self.parse_block_mapping_first_key(scan),
            State::BlockMappingKey => self.parse_block_mapping_key(scan),
            State::BlockMappingValue => self.parse_block_mapping_value(scan),
            State::FlowSequenceFirstEntry => self.parse_flow_sequence_first_entry(scan),
            State::FlowSequenceEntry { first } => self.parse_flow_sequence_entry(scan, first),
            State::FlowSeqMapKey => self.parse_flow_sequence_entry_mapping_key(scan),
            State::FlowSeqMapValue => self.parse_flow_sequence_entry_mapping_value(scan),
            State::FlowSeqMapEnd => self.parse_flow_sequence_entry_mapping_end(scan),
            State::FlowMappingFirstKey => self.parse_flow_mapping_first_key(scan),
            State::FlowMappingKey { first } => self.parse_flow_mapping_key(scan, first),
            State::FlowMappingValue => self.parse_flow_mapping_value(scan),
            State::FlowMappingEmptyValue => self.parse_flow_mapping_empty_value(scan),
        }
    }

    fn parse_stream_start(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        let token = self.get_token(scan)?;
        let (start, end, encoding) = match token {
            Token::StreamStart { start, end, encoding } => (start, end, encoding),
            other => {
                return Err(Fault::Parse(EngineError::new(
                    Formatted::new("expected '<stream start>', but found %r")
                        .arg(ErrArg::Str(token_id(&other).to_string())),
                    token_start(&other),
                )));
            }
        };
        self.state = Some(State::ImplicitDocumentStart);
        Ok(Event::StreamStart { encoding, start, end })
    }

    fn parse_implicit_document_start(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        if !self.check_token(
            scan,
            &[TokenKind::Directive, TokenKind::DocumentStart, TokenKind::StreamEnd],
        )? {
            self.tag_handles = vec![
                ("!".to_string(), "!".to_string()),
                ("!!".to_string(), "tag:yaml.org,2002:".to_string()),
            ];
            let token = self.peek_token(scan)?;
            let mark = token_start(&token);
            self.states.push(State::DocumentEnd);
            self.state = Some(State::BlockNode);
            Ok(Event::DocumentStart {
                explicit: false,
                version: None,
                tags: None,
                start: mark,
                end: mark,
            })
        } else {
            self.parse_document_start(scan)
        }
    }

    fn parse_document_start(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        while self.check_token(scan, &[TokenKind::DocumentEnd])? {
            self.get_token(scan)?;
        }
        if !self.check_token(scan, &[TokenKind::StreamEnd])? {
            let token = self.peek_token(scan)?;
            let start_mark = token_start(&token);
            let (version, tags) = self.process_directives(scan)?;
            if !self.check_token(scan, &[TokenKind::DocumentStart])? {
                let tok = self.peek_token(scan)?;
                return Err(Fault::Parse(EngineError::new(
                    Formatted::new("expected '<document start>', but found %r")
                        .arg(ErrArg::Str(token_id(&tok).to_string())),
                    token_start(&tok),
                )));
            }
            let token = self.get_token(scan)?;
            let end_mark = token_end(&token);
            self.states.push(State::DocumentEnd);
            self.state = Some(State::DocumentContent);
            Ok(Event::DocumentStart {
                explicit: true,
                version,
                tags,
                start: start_mark,
                end: end_mark,
            })
        } else {
            let token = self.get_token(scan)?;
            let (start, end) = (token_start(&token), token_end(&token));
            assert!(self.states.is_empty());
            assert!(self.marks.is_empty());
            self.state = None;
            Ok(Event::StreamEnd { start, end })
        }
    }

    fn parse_document_end(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        let token = self.peek_token(scan)?;
        let start_mark = token_start(&token);
        let mut end_mark = start_mark;
        let mut explicit = false;
        if self.check_token(scan, &[TokenKind::DocumentEnd])? {
            let token = self.get_token(scan)?;
            end_mark = token_end(&token);
            explicit = true;
        }
        self.state = Some(State::DocumentStart);
        Ok(Event::DocumentEnd { explicit, start: start_mark, end: end_mark })
    }

    fn parse_document_content(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        if self.check_token(
            scan,
            &[
                TokenKind::Directive,
                TokenKind::DocumentStart,
                TokenKind::DocumentEnd,
                TokenKind::StreamEnd,
            ],
        )? {
            let token = self.peek_token(scan)?;
            let mark = token_start(&token);
            self.state = self.states.pop();
            Ok(process_empty_scalar(mark))
        } else {
            self.parse_node(scan, true, false)
        }
    }

    /// Directive processing outcome: optional YAML version plus the
    /// tag-handle table (insertion-ordered) when directives were present.
    fn process_directives(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<DirectiveResult, Fault> {
        self.yaml_version = None;
        self.tag_handles = Vec::new();
        while self.check_token(scan, &[TokenKind::Directive])? {
            let token = self.get_token(scan)?;
            let start = match &token {
                Token::Directive { start, .. } => *start,
                _ => unreachable!(),
            };
            let (dname, dvalue) = match token {
                Token::Directive { name, value, .. } => (name, value),
                _ => unreachable!(),
            };
            if dname == "YAML" {
                if self.yaml_version.is_some() {
                    return Err(Fault::Parse(EngineError::new(
                        Formatted::new("found duplicate YAML directive"),
                        start,
                    )));
                }
                let (major, minor) = match dvalue {
                    DirectiveValue::Yaml(ma, mi) => (ma, mi),
                    _ => unreachable!(),
                };
                if major != "1" {
                    return Err(Fault::Parse(EngineError::new(
                        Formatted::new(
                            "found incompatible YAML document (version 1.* is required)",
                        ),
                        start,
                    )));
                }
                self.yaml_version = Some((major, minor));
            } else if dname == "TAG" {
                let (handle, prefix) = match dvalue {
                    DirectiveValue::Tag(h, p) => (h, p),
                    _ => unreachable!(),
                };
                if self.tag_handles.iter().any(|(h, _)| h == &handle) {
                    return Err(Fault::Parse(EngineError::new(
                        Formatted::new("found duplicate tag handle %r")
                            .arg(ErrArg::Str(handle)),
                        start,
                    )));
                }
                self.tag_handles.push((handle, prefix));
            }
        }
        let value = if !self.tag_handles.is_empty() {
            (self.yaml_version.clone(), Some(self.tag_handles.clone()))
        } else {
            (self.yaml_version.clone(), None)
        };
        for (k, v) in [("!", "!"), ("!!", "tag:yaml.org,2002:")] {
            if !self.tag_handles.iter().any(|(h, _)| h == k) {
                self.tag_handles.push((k.to_string(), v.to_string()));
            }
        }
        Ok(value)
    }

    fn parse_node(
        &mut self,
        scan: &mut ScanCore,
        block: bool,
        indentless_sequence: bool,
    ) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::Alias])? {
            let token = self.get_token(scan)?;
            let (value, start, end) = match token {
                Token::Alias { value, start, end } => (value, start, end),
                _ => unreachable!(),
            };
            self.state = self.states.pop();
            return Ok(Event::Alias { anchor: value, start, end });
        }
        let mut anchor: Option<String> = None;
        let mut tag: Option<(Option<String>, String, MarkPos)> = None;
        let mut start_mark: Option<MarkPos> = None;
        let mut end_mark: Option<MarkPos> = None;
        if self.check_token(scan, &[TokenKind::Anchor])? {
            let token = self.get_token(scan)?;
            match token {
                Token::Anchor { value, start, end } => {
                    start_mark = Some(start);
                    end_mark = Some(end);
                    anchor = Some(value);
                }
                _ => unreachable!(),
            }
            if self.check_token(scan, &[TokenKind::Tag])? {
                let token = self.get_token(scan)?;
                match token {
                    Token::Tag { handle, suffix, start, end } => {
                        end_mark = Some(end);
                        tag = Some((handle, super::to_rust_string(&suffix.0), start));
                    }
                    _ => unreachable!(),
                }
            }
        } else if self.check_token(scan, &[TokenKind::Tag])? {
            let token = self.get_token(scan)?;
            match token {
                Token::Tag { handle, suffix, start, end } => {
                    start_mark = Some(start);
                    end_mark = Some(end);
                    tag = Some((handle, super::to_rust_string(&suffix.0), start));
                }
                _ => unreachable!(),
            }
            if self.check_token(scan, &[TokenKind::Anchor])? {
                let token = self.get_token(scan)?;
                match token {
                    Token::Anchor { value, end, .. } => {
                        end_mark = Some(end);
                        anchor = Some(value);
                    }
                    _ => unreachable!(),
                }
            }
        }
        let mut tag_str: Option<String> = None;
        if let Some((handle, suffix, tmark)) = tag {
            match handle {
                Some(h) => {
                    if let Some((_, prefix)) =
                        self.tag_handles.iter().find(|(hh, _)| hh == &h)
                    {
                        tag_str = Some(format!("{prefix}{suffix}"));
                    } else {
                        return Err(Fault::Parse(EngineError::new(
                            Formatted::new("found undefined tag handle %r")
                                .arg(ErrArg::Str(h)),
                            tmark,
                        )
                        .ctx(
                            Formatted::new("while parsing a node"),
                            start_mark.unwrap(),
                        )));
                    }
                }
                None => {
                    tag_str = Some(suffix);
                }
            }
        }
        if start_mark.is_none() {
            let token = self.peek_token(scan)?;
            let m = token_start(&token);
            start_mark = Some(m);
            end_mark = Some(m);
        }
        let start_mark = start_mark.unwrap();
        let mut end_mark_v = end_mark.unwrap();
        let implicit = tag_str.is_none() || tag_str.as_deref() == Some("!");
        if indentless_sequence && self.check_token(scan, &[TokenKind::BlockEntry])? {
            let token = self.peek_token(scan)?;
            end_mark_v = token_end(&token);
            let ev = Event::SequenceStart {
                anchor,
                tag: tag_str,
                implicit,
                flow: false,
                start: start_mark,
                end: end_mark_v,
            };
            self.state = Some(State::IndentlessSequenceEntry);
            return Ok(ev);
        }
        if self.check_token(scan, &[TokenKind::Scalar])? {
            let token = self.get_token(scan)?;
            let (value, plain, style, end) = match token {
                Token::Scalar { value, plain, style, end, .. } => {
                    (value, plain, style, end)
                }
                _ => unreachable!(),
            };
            end_mark_v = end;
            let implicit = if (plain && tag_str.is_none()) || tag_str.as_deref() == Some("!") {
                (true, false)
            } else if tag_str.is_none() {
                (false, true)
            } else {
                (false, false)
            };
            self.state = self.states.pop();
            return Ok(Event::Scalar {
                anchor,
                tag: tag_str,
                implicit,
                value,
                style,
                start: start_mark,
                end: end_mark_v,
            });
        }
        if self.check_token(scan, &[TokenKind::FlowSequenceStart])? {
            let token = self.peek_token(scan)?;
            end_mark_v = token_end(&token);
            self.state = Some(State::FlowSequenceFirstEntry);
            return Ok(Event::SequenceStart {
                anchor,
                tag: tag_str,
                implicit,
                flow: true,
                start: start_mark,
                end: end_mark_v,
            });
        }
        if self.check_token(scan, &[TokenKind::FlowMappingStart])? {
            let token = self.peek_token(scan)?;
            end_mark_v = token_end(&token);
            self.state = Some(State::FlowMappingFirstKey);
            return Ok(Event::MappingStart {
                anchor,
                tag: tag_str,
                implicit,
                flow: true,
                start: start_mark,
                end: end_mark_v,
            });
        }
        if block && self.check_token(scan, &[TokenKind::BlockSequenceStart])? {
            let token = self.peek_token(scan)?;
            end_mark_v = token_start(&token);
            self.state = Some(State::BlockSequenceFirstEntry);
            return Ok(Event::SequenceStart {
                anchor,
                tag: tag_str,
                implicit,
                flow: false,
                start: start_mark,
                end: end_mark_v,
            });
        }
        if block && self.check_token(scan, &[TokenKind::BlockMappingStart])? {
            let token = self.peek_token(scan)?;
            end_mark_v = token_start(&token);
            self.state = Some(State::BlockMappingFirstKey);
            return Ok(Event::MappingStart {
                anchor,
                tag: tag_str,
                implicit,
                flow: false,
                start: start_mark,
                end: end_mark_v,
            });
        }
        if anchor.is_some() || tag_str.is_some() {
            self.state = self.states.pop();
            return Ok(Event::Scalar {
                anchor,
                tag: tag_str,
                implicit: (implicit, false),
                value: YStr::new(),
                style: None,
                start: start_mark,
                end: end_mark_v,
            });
        }
        let token = self.peek_token(scan)?;
        let node = if block { "block" } else { "flow" };
        let problem_mark = token_start(&token);
        Err(Fault::Parse(EngineError::new(
            Formatted::new("expected the node content, but found %r")
                .arg(ErrArg::Str(token_id(&token).to_string())),
            problem_mark,
        )
        .ctx(
            Formatted::new(&format!("while parsing a {node} node")),
            start_mark,
        )))
    }

    fn parse_block_sequence_first_entry(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        let token = self.get_token(scan)?;
        self.marks.push(token_start(&token));
        self.parse_block_sequence_entry(scan)
    }

    fn parse_block_sequence_entry(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::BlockEntry])? {
            let token = self.get_token(scan)?;
            let end = token_end(&token);
            if !self.check_token(scan, &[TokenKind::BlockEntry, TokenKind::BlockEnd])? {
                self.states.push(State::BlockSequenceEntry);
                return self.parse_node(scan, true, false);
            }
            self.state = Some(State::BlockSequenceEntry);
            return Ok(process_empty_scalar(end));
        }
        if !self.check_token(scan, &[TokenKind::BlockEnd])? {
            let token = self.peek_token(scan)?;
            let context_mark = *self.marks.last().unwrap();
            let problem_mark = token_start(&token);
            return Err(Fault::Parse(EngineError::new(
                Formatted::new("expected <block end>, but found %r")
                    .arg(ErrArg::Str(token_id(&token).to_string())),
                problem_mark,
            )
            .ctx(
                Formatted::new("while parsing a block collection"),
                context_mark,
            )));
        }
        let token = self.get_token(scan)?;
        let (start, end) = (token_start(&token), token_end(&token));
        self.state = self.states.pop();
        self.marks.pop();
        Ok(Event::SequenceEnd { start, end })
    }

    fn parse_indentless_sequence_entry(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::BlockEntry])? {
            let token = self.get_token(scan)?;
            let end = token_end(&token);
            if !self.check_token(
                scan,
                &[
                    TokenKind::BlockEntry,
                    TokenKind::Key,
                    TokenKind::Value,
                    TokenKind::BlockEnd,
                ],
            )? {
                self.states.push(State::IndentlessSequenceEntry);
                return self.parse_node(scan, true, false);
            }
            self.state = Some(State::IndentlessSequenceEntry);
            return Ok(process_empty_scalar(end));
        }
        let token = self.peek_token(scan)?;
        let mark = token_start(&token);
        self.state = self.states.pop();
        Ok(Event::SequenceEnd { start: mark, end: mark })
    }

    fn parse_block_mapping_first_key(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        let token = self.get_token(scan)?;
        self.marks.push(token_start(&token));
        self.parse_block_mapping_key(scan)
    }

    fn parse_block_mapping_key(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::Key])? {
            let token = self.get_token(scan)?;
            let end = token_end(&token);
            if !self.check_token(
                scan,
                &[TokenKind::Key, TokenKind::Value, TokenKind::BlockEnd],
            )? {
                self.states.push(State::BlockMappingValue);
                return self.parse_node(scan, true, true);
            }
            self.state = Some(State::BlockMappingValue);
            return Ok(process_empty_scalar(end));
        }
        if !self.check_token(scan, &[TokenKind::BlockEnd])? {
            let token = self.peek_token(scan)?;
            let context_mark = *self.marks.last().unwrap();
            let problem_mark = token_start(&token);
            return Err(Fault::Parse(EngineError::new(
                Formatted::new("expected <block end>, but found %r")
                    .arg(ErrArg::Str(token_id(&token).to_string())),
                problem_mark,
            )
            .ctx(
                Formatted::new("while parsing a block mapping"),
                context_mark,
            )));
        }
        let token = self.get_token(scan)?;
        let (start, end) = (token_start(&token), token_end(&token));
        self.state = self.states.pop();
        self.marks.pop();
        Ok(Event::MappingEnd { start, end })
    }

    fn parse_block_mapping_value(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::Value])? {
            let token = self.get_token(scan)?;
            let end = token_end(&token);
            if !self.check_token(
                scan,
                &[TokenKind::Key, TokenKind::Value, TokenKind::BlockEnd],
            )? {
                self.states.push(State::BlockMappingKey);
                return self.parse_node(scan, true, true);
            }
            self.state = Some(State::BlockMappingKey);
            return Ok(process_empty_scalar(end));
        }
        self.state = Some(State::BlockMappingKey);
        let token = self.peek_token(scan)?;
        Ok(process_empty_scalar(token_start(&token)))
    }

    fn parse_flow_sequence_first_entry(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        let token = self.get_token(scan)?;
        self.marks.push(token_start(&token));
        self.parse_flow_sequence_entry(scan, true)
    }

    fn parse_flow_sequence_entry(
        &mut self,
        scan: &mut ScanCore,
        first: bool,
    ) -> Result<Event, Fault> {
        if !self.check_token(scan, &[TokenKind::FlowSequenceEnd])? {
            if !first {
                if self.check_token(scan, &[TokenKind::FlowEntry])? {
                    self.get_token(scan)?;
                } else {
                    let token = self.peek_token(scan)?;
                    let context_mark = *self.marks.last().unwrap();
                    let problem_mark = token_start(&token);
                    return Err(Fault::Parse(EngineError::new(
                        Formatted::new("expected ',' or ']', but got %r")
                            .arg(ErrArg::Str(token_id(&token).to_string())),
                        problem_mark,
                    )
                    .ctx(
                        Formatted::new("while parsing a flow sequence"),
                        context_mark,
                    )));
                }
            }
            if self.check_token(scan, &[TokenKind::Key])? {
                let token = self.peek_token(scan)?;
                let (start, end) = (token_start(&token), token_end(&token));
                self.state = Some(State::FlowSeqMapKey);
                return Ok(Event::MappingStart {
                    anchor: None,
                    tag: None,
                    implicit: true,
                    flow: true,
                    start,
                    end,
                });
            } else if !self.check_token(scan, &[TokenKind::FlowSequenceEnd])? {
                self.states.push(State::FlowSequenceEntry { first: false });
                return self.parse_node(scan, false, false);
            }
        }
        let token = self.get_token(scan)?;
        let (start, end) = (token_start(&token), token_end(&token));
        self.state = self.states.pop();
        self.marks.pop();
        Ok(Event::SequenceEnd { start, end })
    }

    fn parse_flow_sequence_entry_mapping_key(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        let token = self.get_token(scan)?;
        let end = token_end(&token);
        if !self.check_token(
            scan,
            &[TokenKind::Value, TokenKind::FlowEntry, TokenKind::FlowSequenceEnd],
        )? {
            self.states.push(State::FlowSeqMapValue);
            return self.parse_node(scan, false, false);
        }
        self.state = Some(State::FlowSeqMapValue);
        Ok(process_empty_scalar(end))
    }

    fn parse_flow_sequence_entry_mapping_value(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::Value])? {
            let token = self.get_token(scan)?;
            let end = token_end(&token);
            if !self.check_token(scan, &[TokenKind::FlowEntry, TokenKind::FlowSequenceEnd])? {
                self.states.push(State::FlowSeqMapEnd);
                return self.parse_node(scan, false, false);
            }
            self.state = Some(State::FlowSeqMapEnd);
            return Ok(process_empty_scalar(end));
        }
        self.state = Some(State::FlowSeqMapEnd);
        let token = self.peek_token(scan)?;
        Ok(process_empty_scalar(token_start(&token)))
    }

    fn parse_flow_sequence_entry_mapping_end(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        self.state = Some(State::FlowSequenceEntry { first: false });
        let token = self.peek_token(scan)?;
        let mark = token_start(&token);
        Ok(Event::MappingEnd { start: mark, end: mark })
    }

    fn parse_flow_mapping_first_key(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        let token = self.get_token(scan)?;
        self.marks.push(token_start(&token));
        self.parse_flow_mapping_key(scan, true)
    }

    fn parse_flow_mapping_key(
        &mut self,
        scan: &mut ScanCore,
        first: bool,
    ) -> Result<Event, Fault> {
        if !self.check_token(scan, &[TokenKind::FlowMappingEnd])? {
            if !first {
                if self.check_token(scan, &[TokenKind::FlowEntry])? {
                    self.get_token(scan)?;
                } else {
                    let token = self.peek_token(scan)?;
                    let context_mark = *self.marks.last().unwrap();
                    let problem_mark = token_start(&token);
                    return Err(Fault::Parse(EngineError::new(
                        Formatted::new("expected ',' or '}', but got %r")
                            .arg(ErrArg::Str(token_id(&token).to_string())),
                        problem_mark,
                    )
                    .ctx(
                        Formatted::new("while parsing a flow mapping"),
                        context_mark,
                    )));
                }
            }
            if self.check_token(scan, &[TokenKind::Key])? {
                let token = self.get_token(scan)?;
                let end = token_end(&token);
                if !self.check_token(
                    scan,
                    &[TokenKind::Value, TokenKind::FlowEntry, TokenKind::FlowMappingEnd],
                )? {
                    self.states.push(State::FlowMappingValue);
                    return self.parse_node(scan, false, false);
                }
                self.state = Some(State::FlowMappingValue);
                return Ok(process_empty_scalar(end));
            } else if !self.check_token(scan, &[TokenKind::FlowMappingEnd])? {
                self.states.push(State::FlowMappingEmptyValue);
                return self.parse_node(scan, false, false);
            }
        }
        let token = self.get_token(scan)?;
        let (start, end) = (token_start(&token), token_end(&token));
        self.state = self.states.pop();
        self.marks.pop();
        Ok(Event::MappingEnd { start, end })
    }

    fn parse_flow_mapping_value(&mut self, scan: &mut ScanCore) -> Result<Event, Fault> {
        if self.check_token(scan, &[TokenKind::Value])? {
            let token = self.get_token(scan)?;
            let end = token_end(&token);
            if !self.check_token(scan, &[TokenKind::FlowEntry, TokenKind::FlowMappingEnd])? {
                self.states.push(State::FlowMappingKey { first: false });
                return self.parse_node(scan, false, false);
            }
            self.state = Some(State::FlowMappingKey { first: false });
            return Ok(process_empty_scalar(end));
        }
        self.state = Some(State::FlowMappingKey { first: false });
        let token = self.peek_token(scan)?;
        Ok(process_empty_scalar(token_start(&token)))
    }

    fn parse_flow_mapping_empty_value(
        &mut self,
        scan: &mut ScanCore,
    ) -> Result<Event, Fault> {
        self.state = Some(State::FlowMappingKey { first: false });
        let token = self.peek_token(scan)?;
        Ok(process_empty_scalar(token_start(&token)))
    }
}

fn process_empty_scalar(mark: MarkPos) -> Event {
    Event::Scalar {
        anchor: None,
        tag: None,
        implicit: (true, false),
        value: YStr::new(),
        style: None,
        start: mark,
        end: mark,
    }
}


