//! `yaml._rust`: PyO3 binding over [`yaml_rust_core`].
//!
//! The binding owns every Python-visible conversion and formatting
//! decision, so the core stays Python-free:
//!
//! - `str` <-> code points, lossless through lone surrogates via a single
//!   `surrogatepass` round-trip (`ValueError` past `0x10FFFF`, mirroring
//!   `chr()`);
//! - tokens/events as plain tuples (built once per pull by the facades);
//! - errors as `_ScanFault`/`_UriFault`/`_EmitFault` payloads carrying
//!   message *templates* plus typed args; the Python facades render `%`
//!   formatting, so `%r` output is byte-identical to the original.
//!
//! The single `unsafe` site policy of earlier ports does not apply here:
//! this binding uses only safe PyO3 APIs.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use yaml_rust_core::emit::{EmitCore, EmitError, EmitOptions, EventIn};
use yaml_rust_core::parse::{Event as ParseEvent, Fault as ParseFault, ParseCore};
use yaml_rust_core::scan::{
    DirectiveValue, ScanCore, ScanFault, Token as ScanToken, UriDecodeFault,
};
use yaml_rust_core::{EngineError, ErrArg, Formatted, MarkPos, YStr};

pyo3::create_exception!(yaml_rust, ScanFaultExc, pyo3::exceptions::PyException);
pyo3::create_exception!(yaml_rust, ParseFaultExc, pyo3::exceptions::PyException);
pyo3::create_exception!(yaml_rust, UriFaultExc, pyo3::exceptions::PyException);
pyo3::create_exception!(yaml_rust, EmitFaultExc, pyo3::exceptions::PyException);

// ---------------------------------------------------------------------------
// Lossless str <-> code points.
// ---------------------------------------------------------------------------

/// Python `str` to code points. Fast path borrows valid text with no
/// Python call; lone surrogates (`to_str` fails) fall back to one
/// `surrogatepass` encode plus CESU-8 decode (pairs stay split, exactly
/// like the codec pair does).
fn str_to_u32s(s: &Bound<'_, PyString>) -> PyResult<Vec<u32>> {
    match s.to_str() {
        Ok(text) => Ok(text.chars().map(|c| c as u32).collect()),
        Err(_) => {
            let bytes: Bound<'_, PyBytes> =
                s.call_method("encode", ("utf-8", "surrogatepass"), None)?.extract()?;
            Ok(decode_cesu8(bytes.as_bytes()))
        }
    }
}

fn decode_cesu8(b: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let b0 = b[i] as u32;
        if b0 < 0x80 {
            out.push(b0);
            i += 1;
        } else if b0 < 0xC0 {
            // Unreachable on encoder output; stay total.
            out.push(0xFFFD);
            i += 1;
        } else if b0 < 0xE0 {
            if i + 1 < b.len() {
                out.push(((b0 & 0x1F) << 6) | (b[i + 1] as u32 & 0x3F));
                i += 2;
            } else {
                out.push(0xFFFD);
                i += 1;
            }
        } else if b0 < 0xF0 {
            if i + 2 < b.len() {
                out.push(
                    ((b0 & 0x0F) << 12)
                        | ((b[i + 1] as u32 & 0x3F) << 6)
                        | (b[i + 2] as u32 & 0x3F),
                );
                i += 3;
            } else {
                out.push(0xFFFD);
                i = b.len();
            }
        } else if i + 3 < b.len() {
            out.push(
                ((b0 & 0x07) << 18)
                    | ((b[i + 1] as u32 & 0x3F) << 12)
                    | ((b[i + 2] as u32 & 0x3F) << 6)
                    | (b[i + 3] as u32 & 0x3F),
            );
            i += 4;
        } else {
            out.push(0xFFFD);
            i = b.len();
        }
    }
    out
}

/// Code points to Python `str`: fast path for valid scalars, CESU-8 +
/// `surrogatepass` decode for lone surrogates, `ValueError` past
/// `0x10FFFF` (mirrors `chr()` exactly, message verified on the host).
fn u32s_to_py<'py>(py: Python<'py>, v: &[u32]) -> PyResult<Bound<'py, PyString>> {
    if v.iter().all(|&c| char::from_u32(c).is_some()) {
        let s: String = v.iter().map(|&c| char::from_u32(c).unwrap()).collect();
        return Ok(PyString::new(py, &s));
    }
    if v.iter().any(|&c| c > 0x10FFFF) {
        return Err(PyValueError::new_err("chr() arg not in range(0x110000)"));
    }
    let mut b = Vec::with_capacity(v.len() * 3);
    for &c in v {
        if c < 0x80 {
            b.push(c as u8);
        } else if c < 0x800 {
            b.push((0xC0 | (c >> 6)) as u8);
            b.push((0x80 | (c & 0x3F)) as u8);
        } else if c < 0x10000 {
            b.push((0xE0 | (c >> 12)) as u8);
            b.push((0x80 | ((c >> 6) & 0x3F)) as u8);
            b.push((0x80 | (c & 0x3F)) as u8);
        } else {
            b.push((0xF0 | (c >> 18)) as u8);
            b.push((0x80 | ((c >> 12) & 0x3F)) as u8);
            b.push((0x80 | ((c >> 6) & 0x3F)) as u8);
            b.push((0x80 | (c & 0x3F)) as u8);
        }
    }
    let bytes = PyBytes::new(py, &b);
    bytes.call_method("decode", ("utf-8", "surrogatepass"), None)?.extract()
}

// ---------------------------------------------------------------------------
// Error payloads.
// ---------------------------------------------------------------------------

/// A mark as a plain `(index, line, column)` triple.
type MarkTriple = (usize, usize, usize);

fn mark_tuple(mark: &MarkPos) -> MarkTriple {
    (mark.index, mark.line, mark.column)
}

/// Rendered scan/parse fault: context and problem templates with args.
type FaultPayload =
    (Option<String>, Vec<PyObject>, Option<MarkTriple>, String, Vec<PyObject>, MarkTriple);

fn arg_to_py(py: Python<'_>, arg: &ErrArg) -> PyResult<PyObject> {
    match arg {
        ErrArg::Char(c) => Ok(u32s_to_py(py, &[*c])?.into_any().unbind()),
        ErrArg::Int(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        ErrArg::Str(s) => Ok(PyString::new(py, s).into_any().unbind()),
        ErrArg::Codes(v) => Ok(u32s_to_py(py, v)?.into_any().unbind()),
    }
}

fn formatted_to_py(py: Python<'_>, f: &Formatted) -> PyResult<(String, Vec<PyObject>)> {
    let mut args = Vec::with_capacity(f.args.len());
    for a in &f.args {
        args.push(arg_to_py(py, a)?);
    }
    Ok((f.tpl.clone(), args))
}

fn engine_error_to_py(py: Python<'_>, e: &EngineError) -> PyResult<FaultPayload> {
    let (ctx_tpl, ctx_args) = match &e.context {
        Some(c) => {
            let (t, a) = formatted_to_py(py, c)?;
            (Some(t), a)
        }
        None => (None, Vec::new()),
    };
    let (prob_tpl, prob_args) = formatted_to_py(py, &e.problem)?;
    Ok((
        ctx_tpl,
        ctx_args,
        e.context_mark.as_ref().map(mark_tuple),
        prob_tpl,
        prob_args,
        mark_tuple(&e.problem_mark),
    ))
}

fn scan_fault_to_py(py: Python<'_>, fault: ScanFault) -> PyResult<PyErr> {
    match fault {
        ScanFault::Engine(e) => {
            let payload = engine_error_to_py(py, &e)?;
            Ok(ScanFaultExc::new_err(payload))
        }
        ScanFault::UriDecode(UriDecodeFault { codes, name, start_mark, mark }) => {
            Ok(UriFaultExc::new_err((
                codes,
                name,
                mark_tuple(&start_mark),
                mark_tuple(&mark),
            )))
        }
    }
}

fn parse_fault_to_py(py: Python<'_>, fault: ParseFault) -> PyResult<PyErr> {
    match fault {
        ParseFault::Scan(s) => scan_fault_to_py(py, s),
        ParseFault::Parse(e) => {
            let payload = engine_error_to_py(py, &e)?;
            Ok(ParseFaultExc::new_err(payload))
        }
    }
}

// ---------------------------------------------------------------------------
// Tokens: core -> tuple.
// ---------------------------------------------------------------------------

/// Token tuple: `(tag, s1, s2, s3, b1, marks)`. Per-tag payloads mirror the
/// `yaml.tokens` constructors positionally; the facade builds the objects.
fn token_to_py(py: Python<'_>, tok: &ScanToken) -> PyResult<PyObject> {
    let empty = PyString::new(py, "");
    let (tag, s1, s2, s3, b1, start, end) = match tok {
        ScanToken::StreamStart { encoding, start, end } => (
            0,
            encoding.as_deref().map(|e| PyString::new(py, e)).unwrap_or_else(|| empty.clone()),
            empty.clone(),
            empty.clone(),
            false,
            start,
            end,
        ),
        ScanToken::StreamEnd { start, end } => {
            (1, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::Directive { name, value, start, end } => {
            let (v1, v2) = match value {
                DirectiveValue::Yaml(ma, mi) => {
                    (PyString::new(py, ma), PyString::new(py, mi))
                }
                DirectiveValue::Tag(h, p) => {
                    (PyString::new(py, h), PyString::new(py, p))
                }
                DirectiveValue::Other => (empty.clone(), empty.clone()),
            };
            (2, PyString::new(py, name), v1, v2, false, start, end)
        }
        ScanToken::DocumentStart { start, end } => {
            (3, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::DocumentEnd { start, end } => {
            (4, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::BlockSequenceStart { start, end } => {
            (5, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::BlockMappingStart { start, end } => {
            (6, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::BlockEnd { start, end } => {
            (7, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::FlowSequenceStart { start, end } => {
            (8, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::FlowMappingStart { start, end } => {
            (9, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::FlowSequenceEnd { start, end } => {
            (10, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::FlowMappingEnd { start, end } => {
            (11, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::BlockEntry { start, end } => {
            (12, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::FlowEntry { start, end } => {
            (13, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::Key { start, end } => {
            (14, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::Value { start, end } => {
            (15, empty.clone(), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::Alias { value, start, end } => {
            (16, PyString::new(py, value), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::Anchor { value, start, end } => {
            (17, PyString::new(py, value), empty.clone(), empty.clone(), false, start, end)
        }
        ScanToken::Tag { handle, suffix, start, end } => (
            18,
            handle.as_deref().map(|h| PyString::new(py, h)).unwrap_or_else(|| empty.clone()),
            u32s_to_py(py, &suffix.0)?,
            empty.clone(),
            handle.is_some(),
            start,
            end,
        ),
        ScanToken::Scalar { value, plain, style, start, end } => (
            19,
            u32s_to_py(py, &value.0)?,
            style.map(|c| PyString::new(py, &c.to_string())).unwrap_or_else(|| empty.clone()),
            empty.clone(),
            *plain,
            start,
            end,
        ),
    };
    let marks = (mark_tuple(start), mark_tuple(end));
    Ok((tag, s1, s2, s3, b1, marks).into_pyobject(py)?.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Events: core -> tuple.
// ---------------------------------------------------------------------------

/// Event tuple: `(tag, s1, s2, s3, s4, b1, b2, b3, b4, marks, aux)`.
/// Destructured parts (tag, four strings, four flags, marks, aux).
type EventParts<'a> = (
    u8,
    Bound<'a, PyString>,
    Bound<'a, PyString>,
    Bound<'a, PyString>,
    Bound<'a, PyString>,
    bool,
    bool,
    bool,
    bool,
    &'a MarkPos,
    &'a MarkPos,
    Option<PyObject>,
);

fn event_to_py(py: Python<'_>, ev: &ParseEvent) -> PyResult<PyObject> {
    let empty = PyString::new(py, "");
    let none_aux: Option<PyObject> = None;
    let (tag, s1, s2, s3, s4, b1, b2, b3, b4, start, end, aux): EventParts<'_> =
        match ev {
        ParseEvent::StreamStart { encoding, start, end } => (
            0,
            encoding.as_deref().map(|e| PyString::new(py, e)).unwrap_or_else(|| empty.clone()),
            empty.clone(), empty.clone(), empty.clone(),
            false, false, false, false, start, end, none_aux,
        ),
        ParseEvent::StreamEnd { start, end } => (
            1, empty.clone(), empty.clone(), empty.clone(), empty.clone(),
            false, false, false, false, start, end, none_aux,
        ),
        ParseEvent::DocumentStart { explicit, version, tags, start, end } => {
            let (has_version, major, minor) = match version {
                Some((ma, mi)) => (true, PyString::new(py, ma), PyString::new(py, mi)),
                None => (false, empty.clone(), empty.clone()),
            };
            let aux = match tags {
                Some(pairs) => {
                    let mut items = Vec::with_capacity(pairs.len());
                    for (h, p) in pairs {
                        items.push((PyString::new(py, h), PyString::new(py, p)));
                    }
                    Some(items.into_pyobject(py)?.into_any().unbind())
                }
                None => None,
            };
            (
                2, major, minor, empty.clone(), empty.clone(),
                *explicit, has_version, false, false, start, end, aux,
            )
        }
        ParseEvent::DocumentEnd { explicit, start, end } => (
            3, empty.clone(), empty.clone(), empty.clone(), empty.clone(),
            *explicit, false, false, false, start, end, none_aux,
        ),
        ParseEvent::Alias { anchor, start, end } => (
            4, PyString::new(py, anchor), empty.clone(), empty.clone(), empty.clone(),
            false, false, false, false, start, end, none_aux,
        ),
        ParseEvent::Scalar { anchor, tag, implicit, value, style, start, end } => (
            5,
            anchor.as_deref().map(|a| PyString::new(py, a)).unwrap_or_else(|| empty.clone()),
            tag.as_deref().map(|t| PyString::new(py, t)).unwrap_or_else(|| empty.clone()),
            u32s_to_py(py, &value.0)?,
            style.map(|c| PyString::new(py, &c.to_string())).unwrap_or_else(|| empty.clone()),
            anchor.is_some(),
            tag.is_some(),
            implicit.0,
            implicit.1,
            start,
            end,
            none_aux,
        ),
        ParseEvent::SequenceStart { anchor, tag, implicit, flow, start, end } => (
            6,
            anchor.as_deref().map(|a| PyString::new(py, a)).unwrap_or_else(|| empty.clone()),
            tag.as_deref().map(|t| PyString::new(py, t)).unwrap_or_else(|| empty.clone()),
            empty.clone(), empty.clone(),
            anchor.is_some(),
            tag.is_some(),
            *implicit,
            *flow,
            start, end, none_aux,
        ),
        ParseEvent::SequenceEnd { start, end } => (
            7, empty.clone(), empty.clone(), empty.clone(), empty.clone(),
            false, false, false, false, start, end, none_aux,
        ),
        ParseEvent::MappingStart { anchor, tag, implicit, flow, start, end } => (
            8,
            anchor.as_deref().map(|a| PyString::new(py, a)).unwrap_or_else(|| empty.clone()),
            tag.as_deref().map(|t| PyString::new(py, t)).unwrap_or_else(|| empty.clone()),
            empty.clone(), empty.clone(),
            anchor.is_some(),
            tag.is_some(),
            *implicit,
            *flow,
            start, end, none_aux,
        ),
        ParseEvent::MappingEnd { start, end } => (
            9, empty.clone(), empty.clone(), empty.clone(), empty.clone(),
            false, false, false, false, start, end, none_aux,
        ),
    };
    let marks = (mark_tuple(start), mark_tuple(end));
    Ok((tag, s1, s2, s3, s4, b1, b2, b3, b4, marks, aux).into_pyobject(py)?.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Events: tuple -> core.
// ---------------------------------------------------------------------------

/// Facade event tuple: `(tag, s1, s2, s3, s4, b1, b2, b3, b4, aux)`.
fn event_from_py(ev: &Bound<'_, PyAny>) -> PyResult<EventIn> {
    let tag: u8 = ev.get_item(0)?.extract()?;
    let s = |i: usize| -> PyResult<String> { ev.get_item(i)?.extract() };
    let b = |i: usize| -> PyResult<bool> { ev.get_item(i)?.extract() };
    let opt_u32s = |i: usize, has: bool| -> PyResult<Option<Vec<u32>>> {
        if has {
            let st: Bound<'_, PyString> = ev.get_item(i)?.extract()?;
            Ok(Some(str_to_u32s(&st)?))
        } else {
            Ok(None)
        }
    };
    match tag {
        0 => Ok(EventIn::StreamStart),
        1 => Ok(EventIn::StreamEnd),
        2 => {
            let version =
                if b(6)? { Some((s(1)?, s(2)?)) } else { None };
            let tags = if b(7)? {
                let aux: Vec<(Bound<'_, PyString>, Bound<'_, PyString>)> =
                    ev.get_item(9)?.extract()?;
                let mut pairs = Vec::with_capacity(aux.len());
                for (h, p) in aux {
                    pairs.push((str_to_u32s(&h)?, str_to_u32s(&p)?));
                }
                Some(pairs)
            } else {
                None
            };
            Ok(EventIn::DocumentStart { explicit: b(5)?, version, tags })
        }
        3 => Ok(EventIn::DocumentEnd { explicit: b(5)? }),
        4 => Ok(EventIn::Alias { anchor: opt_u32s(1, b(5)?)? }),
        5 => {
            let style: Option<String> = ev.get_item(4)?.extract()?;
            let value: Bound<'_, PyString> = ev.get_item(3)?.extract()?;
            Ok(EventIn::Scalar {
                anchor: opt_u32s(1, b(5)?)?,
                tag: opt_u32s(2, b(6)?)?,
                implicit: (b(7)?, b(8)?),
                value: YStr(str_to_u32s(&value)?),
                style,
            })
        }
        6 => Ok(EventIn::SequenceStart {
            anchor: opt_u32s(1, b(5)?)?,
            tag: opt_u32s(2, b(6)?)?,
            implicit: b(7)?,
            flow: b(8)?,
        }),
        7 => Ok(EventIn::SequenceEnd),
        8 => Ok(EventIn::MappingStart {
            anchor: opt_u32s(1, b(5)?)?,
            tag: opt_u32s(2, b(6)?)?,
            implicit: b(7)?,
            flow: b(8)?,
        }),
        9 => Ok(EventIn::MappingEnd),
        other => Err(PyValueError::new_err(format!("unknown event tag {other}"))),
    }
}

// ---------------------------------------------------------------------------
// Engine classes.
// ---------------------------------------------------------------------------

/// Shared scan/parse engine behind the `Scanner`/`Parser` facades: one
/// object per loader, exactly like the original MRO-shared state.
#[pyclass]
struct Engine {
    scan: ScanCore,
    parse: Option<ParseCore>,
}

#[pymethods]
impl Engine {
    #[new]
    #[pyo3(signature = (text, encoding=None))]
    fn new(text: Bound<'_, PyString>, encoding: Option<String>) -> PyResult<Self> {
        Ok(Engine {
            scan: ScanCore::new(str_to_u32s(&text)?, encoding),
            parse: None,
        })
    }

    fn peek_token(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match self.scan.peek_token() {
            Ok(Some(tok)) => Ok(Some(token_to_py(py, &tok)?)),
            Ok(None) => Ok(None),
            Err(f) => Err(scan_fault_to_py(py, f)?),
        }
    }

    /// Drop the head token after a successful peek (mirrors `get_token`:
    /// the pop itself pulls nothing and cannot fail).
    fn consume_token(&mut self) {
        self.scan.pop_head();
    }

    fn next_token(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match self.scan.next_token() {
            Ok(Some(tok)) => Ok(Some(token_to_py(py, &tok)?)),
            Ok(None) => Ok(None),
            Err(f) => Err(scan_fault_to_py(py, f)?),
        }
    }

    fn has_token(&mut self, py: Python<'_>) -> PyResult<bool> {
        match self.scan.has_token() {
            Ok(v) => Ok(v),
            Err(f) => Err(scan_fault_to_py(py, f)?),
        }
    }

    fn start_parse(&mut self) {
        if self.parse.is_none() {
            self.parse = Some(ParseCore::new());
        }
    }

    fn parse_active(&self) -> bool {
        self.parse.is_some()
    }

    fn peek_event(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let Some(parse) = self.parse.as_mut() else {
            return Ok(None);
        };
        match parse.next_event(&mut self.scan) {
            Ok(Some(ev)) => Ok(Some(event_to_py(py, &ev)?)),
            Ok(None) => Ok(None),
            Err(f) => Err(parse_fault_to_py(py, f)?),
        }
    }

    fn take_event(&mut self) -> PyResult<bool> {
        if let Some(parse) = self.parse.as_mut() {
            Ok(parse.take_event().is_some())
        } else {
            Ok(false)
        }
    }

    fn dispose_parse(&mut self) {
        if let Some(parse) = self.parse.as_mut() {
            parse.dispose();
        }
        self.parse = None;
    }
}

fn emit_error_to_py(py: Python<'_>, e: &EmitError) -> PyResult<PyErr> {
    let (tpl, args) = formatted_to_py(py, &e.problem)?;
    Ok(EmitFaultExc::new_err((tpl, args, e.processed)))
}

/// Event-accepting engine behind the `Emitter`/`CEmitter` facades.
#[pyclass]
struct EmitEngine {
    core: EmitCore,
}

#[pymethods]
impl EmitEngine {
    #[new]
    #[pyo3(signature = (canonical=false, indent=None, width=None, allow_unicode=false, line_break=None))]
    fn new(
        canonical: bool,
        indent: Option<i64>,
        width: Option<i64>,
        allow_unicode: bool,
        line_break: Option<String>,
    ) -> PyResult<Self> {
        Ok(EmitEngine {
            core: EmitCore::new(EmitOptions {
                canonical,
                indent,
                width,
                allow_unicode,
                line_break,
            }),
        })
    }

    /// Queue one facade event tuple; returns newly processed count plus
    /// the text accumulated since the last call (one FFI round-trip per
    /// event instead of two).
    fn emit(&mut self, py: Python<'_>, ev: Bound<'_, PyAny>) -> PyResult<(usize, String)> {
        let event = event_from_py(&ev)?;
        match self.core.emit(event) {
            Ok(n) => Ok((n, self.core.drain())),
            Err(e) => Err(emit_error_to_py(py, &e)?),
        }
    }

    fn dispose(&mut self) {
        self.core.dispose();
    }
}

#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Engine>()?;
    m.add_class::<EmitEngine>()?;
    m.add("ScanFault", m.py().get_type::<ScanFaultExc>())?;
    m.add("ParseFault", m.py().get_type::<ParseFaultExc>())?;
    m.add("UriFault", m.py().get_type::<UriFaultExc>())?;
    m.add("EmitFault", m.py().get_type::<EmitFaultExc>())?;
    Ok(())
}
