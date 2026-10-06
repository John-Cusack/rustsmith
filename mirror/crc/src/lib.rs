//! Stage-1 Rust mirror of `Nicoretti/crc` (pure-Python CRC library).
//!
//! PyO3 binding over the independent `crc-rust-core` crate: same module
//! boundary (`crc._crc`), same public names, same vectors. No redesign:
//! byte-at-a-time table lookup ships (slice-by-8 is a Stage-2/M6 candidate,
//! not here). All arithmetic lives in `crc_core`; this file only translates
//! between Python objects and core values.
//!
//! License: BSD-2-Clause (preserved from the original; see NOTICE).

use pyo3::exceptions::{PyAttributeError, PyIndexError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

// Core arithmetic (single implementation shared with Rust consumers).
// `process_byte_table` is re-exported for the Stage-2 `slice8` module, which
// reaches it as `super::process_byte_table`.
pub use crc_core::{bitmask, process_byte_table, reflect_byte, Config};
use crc_core::{
    create_table, crc16_members, crc32_members, crc64_members, crc8_members, digest_of,
    format_value, render_template, reverse_of, update_bit, update_table,
};

// ---------------------------------------------------------------------------
// Python-visible classes.
// ---------------------------------------------------------------------------

/// `Configuration` (mirrors the frozen dataclass: value equality + hash,
/// `FrozenInstanceError` on attribute assignment/deletion).
#[pyclass]
#[derive(Clone)]
struct Configuration {
    cfg: Config,
}

/// `dataclasses.FrozenInstanceError` with the dataclass message, raised by
/// `Configuration.__setattr__`/`__delattr__` exactly like the original.
fn frozen_error(py: Python<'_>, action: &str, name: &str) -> PyErr {
    let msg = format!("cannot {action} field '{name}'");
    match py
        .import("dataclasses")
        .and_then(|d| d.getattr("FrozenInstanceError"))
        .and_then(|cls| cls.call1((msg.clone(),)))
    {
        Ok(inst) => PyErr::from_value(inst),
        Err(_) => PyAttributeError::new_err(msg),
    }
}

#[pymethods]
impl Configuration {
    #[new]
    #[pyo3(signature = (width, polynomial, init_value=0, final_xor_value=0, reverse_input=false, reverse_output=false))]
    fn new(
        width: u8,
        polynomial: u64,
        init_value: u64,
        final_xor_value: u64,
        reverse_input: bool,
        reverse_output: bool,
    ) -> Self {
        Self {
            cfg: Config {
                width,
                poly: polynomial,
                init: init_value,
                xorout: final_xor_value,
                refin: reverse_input,
                refout: reverse_output,
            },
        }
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .downcast::<Configuration>()
            .map(|o| o.borrow().cfg == self.cfg)
            .unwrap_or(false)
    }
    fn __ne__(&self, other: &Bound<'_, PyAny>) -> bool {
        !self.__eq__(other)
    }
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        // Same tuple the dataclass hashes, so hashes match the original.
        let t = (
            self.cfg.width,
            self.cfg.poly,
            self.cfg.init,
            self.cfg.xorout,
            self.cfg.refin,
            self.cfg.refout,
        )
            .into_pyobject(py)?;
        t.hash()
    }
    fn __setattr__(&self, py: Python<'_>, name: &str, _value: PyObject) -> PyResult<()> {
        Err(frozen_error(py, "assign to", name))
    }
    fn __delattr__(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        Err(frozen_error(py, "delete", name))
    }

    #[getter]
    fn width(&self) -> u8 {
        self.cfg.width
    }
    #[getter]
    fn polynomial(&self) -> u64 {
        self.cfg.poly
    }
    #[getter]
    fn init_value(&self) -> u64 {
        self.cfg.init
    }
    #[getter]
    fn final_xor_value(&self) -> u64 {
        self.cfg.xorout
    }
    #[getter]
    fn reverse_input(&self) -> bool {
        self.cfg.refin
    }
    #[getter]
    fn reverse_output(&self) -> bool {
        self.cfg.refout
    }

    fn __repr__(&self) -> String {
        // Dataclass `repr`: decimal ints, `True`/`False` bools.
        let b = |v: bool| if v { "True" } else { "False" };
        format!(
            "Configuration(width={}, polynomial={}, init_value={}, final_xor_value={}, reverse_input={}, reverse_output={})",
            self.cfg.width,
            self.cfg.poly,
            self.cfg.init,
            self.cfg.xorout,
            b(self.cfg.refin),
            b(self.cfg.refout)
        )
    }
}

/// Single byte with masking arithmetic (mirrors `Byte`).
#[pyclass]
#[derive(Clone)]
struct Byte {
    #[pyo3(get, set)]
    _value: u8,
}

#[pymethods]
impl Byte {
    #[new]
    #[pyo3(signature = (value=0))]
    fn new(value: i64) -> PyResult<Self> {
        Ok(Self {
            _value: (value & 0xFF) as u8,
        })
    }
    fn __add__(&self, other: &Bound<'_, PyAny>) -> PyResult<Byte> {
        let o = byte_operand(other)?;
        Ok(Byte {
            _value: self._value.wrapping_add(o),
        })
    }
    fn __radd__(&self, other: &Bound<'_, PyAny>) -> PyResult<Byte> {
        self.__add__(other)
    }
    fn __iadd__(&mut self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let o = byte_operand(other)?;
        self._value = self._value.wrapping_add(o);
        Ok(())
    }
    #[getter]
    fn value(&self) -> u8 {
        self._value
    }
    #[setter]
    fn set_value(&mut self, v: i64) {
        self._value = (v & 0xFF) as u8;
    }

    fn __len__(&self) -> usize {
        8
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        if !(0..8).contains(&index) {
            return Err(PyIndexError::new_err("byte index out of range"));
        }
        Ok((self._value >> index) & 1)
    }
    fn __iter__(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let items: Vec<u8> = (0..8).map(|i| (slf._value >> i) & 1).collect();
        let list = PyList::new(py, items)?;
        Ok(list.try_iter()?.into_pyobject(py)?.into_any().unbind())
    }
    fn __int__(&self) -> u8 {
        self._value
    }
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .downcast::<Byte>()
            .map(|b| b.borrow()._value == self._value)
            .unwrap_or(false)
    }
    fn __ne__(&self, other: &Bound<'_, PyAny>) -> bool {
        !self.__eq__(other)
    }
    fn __hash__(&self) -> isize {
        self._value as isize
    }
    #[pyo3(text_signature = "(self)")]
    fn reversed(&self) -> Byte {
        Byte {
            _value: reflect_byte(self._value),
        }
    }
}

fn byte_operand(other: &Bound<'_, PyAny>) -> PyResult<u8> {
    if let Ok(b) = other.downcast::<Byte>() {
        return Ok(b.borrow()._value);
    }
    if let Ok(n) = other.extract::<i64>() {
        return Ok((n & 0xFF) as u8);
    }
    Err(PyTypeError::new_err(format!(
        "unsupported operand: {}",
        other.get_type().qualname()?
    )))
}

/// Shared register state helpers.
fn reg_len(cfg: &Config) -> usize {
    (cfg.width / 8) as usize
}

fn reg_get(reg: u64, cfg: &Config, index: isize) -> PyResult<u8> {
    let n = reg_len(cfg) as isize;
    if !(0..n).contains(&index) {
        return Err(PyIndexError::new_err("register index out of range"));
    }
    Ok(((reg >> (index as u64 * 8)) & 0xFF) as u8)
}

/// Bit-by-bit `Register`.
#[pyclass]
struct Register {
    cfg: Config,
    reg: u64,
}

#[pymethods]
impl Register {
    #[new]
    fn new(configuration: &Bound<'_, PyAny>) -> PyResult<Self> {
        let cfg = config_of(configuration)?;
        let reg = cfg.init & bitmask(&cfg);
        Ok(Self { cfg, reg })
    }
    #[pyo3(text_signature = "(self)")]
    fn init(&mut self) {
        self.reg = self.cfg.init & bitmask(&self.cfg);
    }
    #[pyo3(text_signature = "(self, data)")]
    fn update(&mut self, data: &Bound<'_, PyAny>) -> PyResult<u64> {
        let bytes = bytes_like(data)?;
        self.reg = update_bit(self.reg, &self.cfg, &bytes);
        Ok(self.reg)
    }
    #[pyo3(text_signature = "(self)")]
    fn digest(&self) -> u64 {
        digest_of(self.reg, &self.cfg)
    }
    #[pyo3(text_signature = "(self)")]
    fn reverse(&self) -> u64 {
        reverse_of(self.reg, &self.cfg)
    }
    #[getter]
    fn register(&self) -> u64 {
        self.reg & bitmask(&self.cfg)
    }
    #[setter]
    fn set_register(&mut self, value: u64) {
        self.reg = value & bitmask(&self.cfg);
    }
    fn __len__(&self) -> usize {
        reg_len(&self.cfg)
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        reg_get(self.reg, &self.cfg, index)
    }
}

/// Table-driven `TableBasedRegister` (byte-at-a-time; mirror ships this).
#[pyclass]
struct TableBasedRegister {
    cfg: Config,
    reg: u64,
    table: [u64; 256],
}

#[pymethods]
impl TableBasedRegister {
    #[new]
    fn new(configuration: &Bound<'_, PyAny>) -> PyResult<Self> {
        let cfg = config_of(configuration)?;
        let table = create_table(cfg.width, cfg.poly);
        let reg = cfg.init & bitmask(&cfg);
        Ok(Self { cfg, reg, table })
    }
    #[pyo3(text_signature = "(self)")]
    fn init(&mut self) {
        self.reg = self.cfg.init & bitmask(&self.cfg);
    }
    #[pyo3(text_signature = "(self, data)")]
    fn update(&mut self, data: &Bound<'_, PyAny>) -> PyResult<u64> {
        let bytes = bytes_like(data)?;
        self.reg = update_table(self.reg, &self.cfg, &self.table, &bytes);
        Ok(self.reg)
    }
    #[pyo3(text_signature = "(self)")]
    fn digest(&self) -> u64 {
        digest_of(self.reg, &self.cfg)
    }
    #[pyo3(text_signature = "(self)")]
    fn reverse(&self) -> u64 {
        reverse_of(self.reg, &self.cfg)
    }
    #[getter]
    fn register(&self) -> u64 {
        self.reg & bitmask(&self.cfg)
    }
    #[setter]
    fn set_register(&mut self, value: u64) {
        self.reg = value & bitmask(&self.cfg);
    }
    fn __len__(&self) -> usize {
        reg_len(&self.cfg)
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        reg_get(self.reg, &self.cfg, index)
    }
}

/// `BasicRegister` (abstract in the original; never instantiated by tests).
#[pyclass]
struct BasicRegister {
    cfg: Config,
    reg: u64,
}

#[pymethods]
impl BasicRegister {
    #[new]
    fn new(configuration: &Bound<'_, PyAny>) -> PyResult<Self> {
        let cfg = config_of(configuration)?;
        let reg = cfg.init & bitmask(&cfg);
        Ok(Self { cfg, reg })
    }
    #[pyo3(text_signature = "(self)")]
    fn init(&mut self) {
        self.reg = self.cfg.init & bitmask(&self.cfg);
    }
    #[pyo3(text_signature = "(self, data)")]
    fn update(&mut self, _data: &Bound<'_, PyAny>) -> PyResult<u64> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "_process_byte is abstract",
        ))
    }
    #[pyo3(text_signature = "(self)")]
    fn digest(&self) -> u64 {
        digest_of(self.reg, &self.cfg)
    }
    #[pyo3(text_signature = "(self)")]
    fn reverse(&self) -> u64 {
        reverse_of(self.reg, &self.cfg)
    }
    #[getter]
    fn register(&self) -> u64 {
        self.reg & bitmask(&self.cfg)
    }
    #[setter]
    fn set_register(&mut self, value: u64) {
        self.reg = value & bitmask(&self.cfg);
    }
    fn __len__(&self) -> usize {
        reg_len(&self.cfg)
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        reg_get(self.reg, &self.cfg, index)
    }
}

/// Placeholder abstract base (importable; behavior lives in subclasses).
#[pyclass]
struct AbstractRegister;

#[pymethods]
impl AbstractRegister {
    #[new]
    fn new() -> Self {
        Self
    }
}

/// `Calculator` with `optimized` selecting the register.
#[pyclass]
struct Calculator {
    cfg: Config,
    reg: u64,
    table: Option<[u64; 256]>,
}

#[pymethods]
impl Calculator {
    #[new]
    #[pyo3(signature = (configuration, optimized=false))]
    fn new(configuration: &Bound<'_, PyAny>, optimized: bool) -> PyResult<Self> {
        let cfg = config_of(configuration)?;
        let table = if optimized {
            Some(create_table(cfg.width, cfg.poly))
        } else {
            None
        };
        let reg = cfg.init & bitmask(&cfg);
        Ok(Self { cfg, reg, table })
    }
    #[pyo3(text_signature = "(self, data)")]
    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {
        let bytes = extract_bytes(data.bind(py))?;
        self.reg = self.cfg.init & bitmask(&self.cfg);
        self.reg = match &self.table {
            Some(t) => update_table(self.reg, &self.cfg, t, &bytes),
            None => update_bit(self.reg, &self.cfg, &bytes),
        };
        Ok(digest_of(self.reg, &self.cfg))
    }
    #[pyo3(text_signature = "(self, data, expected)")]
    fn verify(&mut self, py: Python<'_>, data: PyObject, expected: u64) -> PyResult<bool> {
        Ok(self.checksum(py, data)? == expected)
    }
}

/// Accept `Configuration` or a catalog member (which carries `.value`).
fn config_of(obj: &Bound<'_, PyAny>) -> PyResult<Config> {
    if let Ok(c) = obj.extract::<PyRef<'_, Configuration>>() {
        return Ok(c.cfg);
    }
    let py = obj.py();
    let enum_mod = py.import("enum")?;
    let enum_cls = enum_mod.getattr("Enum")?;
    if obj.is_instance(&enum_cls)? {
        let v = obj.getattr("value")?;
        return config_of(&v);
    }
    Err(PyTypeError::new_err(
        "expected Configuration or catalog member",
    ))
}

/// `update()` takes `bytes` (tests always pass bytes here).
fn bytes_like(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = obj.downcast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    if let Ok(b) = obj.downcast::<pyo3::types::PyByteArray>() {
        return Ok(b.to_vec());
    }
    if obj.is_instance_of::<pyo3::types::PyMemoryView>() {
        let mv = obj.downcast::<pyo3::types::PyMemoryView>().unwrap();
        if let Ok(v) = mv.extract::<Vec<u8>>() {
            return Ok(v);
        }
        let b = mv.call_method0("tobytes")?;
        return bytes_like(&b);
    }
    Err(PyTypeError::new_err(format!(
        "Unsupported parameter type: {}",
        obj.get_type().repr()?
    )))
}

fn unsupported(obj: &Bound<'_, PyAny>) -> PyErr {
    let t = obj
        .get_type()
        .repr()
        .map(|r| r.to_string())
        .unwrap_or_else(|_| "?".to_string());
    PyTypeError::new_err(format!("Unsupported parameter type: {t}"))
}

/// Full `_bytes_generator` semantics: int->single byte via `to_bytes(1, "big")`,
/// bytes-like, file-like via `.read()`, other iterables element-wise via the
/// `bytes()` builtin, else TypeError. Error types and messages match the
/// original exactly (including `str` input, which the original routes through
/// the iterable path so `bytes(char)` raises).
fn extract_bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    // int (bool included, matching isinstance) -> one big-endian byte.
    // Delegating to `to_bytes` reproduces the original's OverflowError
    // messages exactly (negative vs. too large vs. huge).
    if obj.is_instance_of::<pyo3::types::PyInt>() {
        let b = obj.call_method1("to_bytes", (1, "big"))?;
        return bytes_like(&b);
    }
    if let Ok(b) = obj.downcast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    if let Ok(b) = obj.downcast::<pyo3::types::PyByteArray>() {
        return Ok(b.to_vec());
    }
    if obj.is_instance_of::<pyo3::types::PyMemoryView>() {
        return bytes_like(obj);
    }
    // BinaryIO / file-like.
    if obj.hasattr("read")? {
        let data = obj.getattr("read")?.call0()?;
        return extract_bytes(&data);
    }
    if let Ok(iter) = obj.try_iter() {
        let py = obj.py();
        let builtins = py.import("builtins")?;
        let bytes_fn = builtins.getattr("bytes")?;
        let mut out = Vec::new();
        for item in iter {
            let item = item?;
            // `bytes(e)` per element, exactly like the original.
            let b = bytes_fn.call1((item,))?;
            out.extend(bytes_like(&b)?);
        }
        return Ok(out);
    }
    Err(unsupported(obj))
}

#[pyfunction]
fn create_lookup_table(width: u8, polynomial: u64) -> Vec<u64> {
    create_table(width, polynomial).to_vec()
}

#[pyfunction(name = "_generate_template")]
fn generate_template(width: u64) -> String {
    render_template(width)
}

/// `table(args)` with an argparse.Namespace (width/polynomial attrs).
#[pyfunction]
fn table(py: Python<'_>, args: &Bound<'_, PyAny>) -> PyResult<bool> {
    let width_obj = args.getattr("width")?;
    let poly_obj = args.getattr("polynomial")?;
    if !width_obj.is_truthy()? || !poly_obj.is_truthy()? {
        return Ok(false);
    }
    let width: u64 = width_obj.extract()?;
    let poly: u64 = poly_obj.extract()?;
    let lut = create_table(width as u8, poly);
    let digits = width.div_ceil(4) as usize;
    let mut rows = Vec::new();
    for chunk in lut.chunks(8) {
        rows.push(
            chunk
                .iter()
                .map(|v| format_value(digits, *v))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let sys = py.import("sys")?;
    sys.getattr("stdout")?
        .call_method1("write", (rows.join("\n") + "\n",))?;
    Ok(true)
}

/// Build the exact `argparse` parser from the original `_argument_parser()`
/// and delegate parsing to it, so help text, errors, and exit codes are
/// identical on every Python version (argparse itself raises `SystemExit`
/// for `--help` and usage errors, which propagates to the caller).
fn build_parser(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let argparse = py.import("argparse")?;
    let functools = py.import("functools")?;
    let int_type = py.get_type::<pyo3::types::PyInt>();
    let base_kw = PyDict::new(py);
    base_kw.set_item("base", 0)?;
    let into_int = functools
        .getattr("partial")?
        .call((int_type,), Some(&base_kw))?;
    let parser_kw = PyDict::new(py);
    parser_kw.set_item("prog", "crc")?;
    parser_kw.set_item(
        "description",
        "A set of crc checksum related command line tools.",
    )?;
    parser_kw.set_item(
        "formatter_class",
        argparse.getattr("ArgumentDefaultsHelpFormatter")?,
    )?;
    let parser = argparse
        .getattr("ArgumentParser")?
        .call((), Some(&parser_kw))?;
    let subparsers = parser.call_method0("add_subparsers")?;
    let table_kw = PyDict::new(py);
    table_kw.set_item(
        "help",
        "Generates lookup tables for various crc algorithm settings",
    )?;
    let table_command = subparsers.call_method("add_parser", ("table",), Some(&table_kw))?;
    let width_kw = PyDict::new(py);
    width_kw.set_item("metavar", "<width>")?;
    width_kw.set_item("type", into_int.clone())?;
    width_kw.set_item(
        "help",
        "width of the crc algorithm, common width's are 8, 16, 32, 64",
    )?;
    table_command.call_method("add_argument", ("width",), Some(&width_kw))?;
    let poly_kw = PyDict::new(py);
    poly_kw.set_item("metavar", "<polynomial>")?;
    poly_kw.set_item("type", into_int)?;
    poly_kw.set_item(
        "help",
        "hex value of the polynomial used for calculating the crc table",
    )?;
    table_command.call_method("add_argument", ("polynomial",), Some(&poly_kw))?;
    // `func` is the module-level `table` from this same extension module.
    let table_fn = py.import("crc._crc")?.getattr("table")?;
    let defaults_kw = PyDict::new(py);
    defaults_kw.set_item("func", table_fn)?;
    table_command.call_method("set_defaults", (), Some(&defaults_kw))?;
    Ok(parser)
}

/// `main(argv=None)` mirroring the original: argparse parses (raising
/// `SystemExit` itself for `--help`/usage errors); with a `func` the table
/// is printed and we exit 0/-1, otherwise help is printed and we exit -1.
#[pyfunction]
#[pyo3(signature = (argv=None))]
fn main(py: Python<'_>, argv: Option<PyObject>) -> PyResult<PyObject> {
    let sys = py.import("sys")?;
    let parser = build_parser(py)?;
    let args = match argv {
        None => parser.call_method0("parse_args")?,
        Some(o) => parser.call_method1("parse_args", (o,))?,
    };
    if args.hasattr("func")? {
        let ok: bool = args.getattr("func")?.call1((args.clone(),))?.extract()?;
        let code = if ok { 0 } else { -1 };
        sys.getattr("exit")?.call1((code,))?;
    } else {
        parser.call_method0("print_help")?;
        sys.getattr("exit")?.call1((-1,))?;
    }
    Ok(py.None())
}

/// Constructor parameter spec for `__signature__` injection (pyo3 classes
/// expose a generic `(*args, **kwargs)` constructor to `inspect`, so the
/// original's exact constructor signature is attached at module init).
struct CtorParam {
    name: &'static str,
    default: ParamDefault,
    annotation: &'static str,
}

#[derive(Clone, Copy)]
enum ParamDefault {
    /// No default (required parameter).
    None,
    Bool(bool),
    Int(i64),
}

enum CtorReturn {
    /// No return annotation.
    None,
    /// `-> None` (the `None` value as annotation, as stored by the original
    /// `Configuration` dataclass constructor; renders unquoted).
    Null,
    /// `-> 'Str'` (string annotation, as rendered under
    /// `from __future__ import annotations`).
    Str(&'static str),
}

fn set_ctor_signature(
    py: Python<'_>,
    m: &Bound<'_, PyModule>,
    name: &str,
    params: &[CtorParam],
    ret: CtorReturn,
) -> PyResult<()> {
    let inspect_mod = py.import("inspect")?;
    let param_cls = inspect_mod.getattr("Parameter")?;
    let sig_cls = inspect_mod.getattr("Signature")?;
    let kind = param_cls.getattr("POSITIONAL_OR_KEYWORD")?;
    let empty = param_cls.getattr("empty")?;
    let plist = PyList::empty(py);
    for p in params {
        let default: Bound<'_, PyAny> = match p.default {
            ParamDefault::None => empty.clone(),
            ParamDefault::Bool(b) => pyo3::types::PyBool::new(py, b).to_owned().into_any(),
            ParamDefault::Int(i) => i.into_pyobject(py)?.into_any(),
        };
        let annotation: Bound<'_, PyAny> = if p.annotation.is_empty() {
            empty.clone()
        } else {
            pyo3::types::PyString::new(py, p.annotation).into_any()
        };
        let kwargs = PyDict::new(py);
        kwargs.set_item("default", default)?;
        kwargs.set_item("annotation", annotation)?;
        plist.append(param_cls.call((p.name, kind.clone()), Some(&kwargs))?)?;
    }
    let sig_kw = PyDict::new(py);
    match ret {
        CtorReturn::None => {}
        CtorReturn::Null => {
            sig_kw.set_item("return_annotation", py.None())?;
        }
        CtorReturn::Str(s) => {
            sig_kw.set_item("return_annotation", pyo3::types::PyString::new(py, s))?;
        }
    }
    let sig = sig_cls.call((plist,), Some(&sig_kw))?;
    m.getattr(name)?.setattr("__signature__", sig)?;
    Ok(())
}

/// Attach the original's exact constructor signatures (`inspect.signature`
/// renders `__signature__` verbatim when present).
fn set_ctor_signatures(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cfg = |n: &'static str, d: ParamDefault, a: &'static str| CtorParam {
        name: n,
        default: d,
        annotation: a,
    };
    set_ctor_signature(
        py,
        m,
        "Calculator",
        &[
            cfg("configuration", ParamDefault::None, "Configuration"),
            cfg("optimized", ParamDefault::Bool(false), "bool"),
        ],
        CtorReturn::Str("None"),
    )?;
    set_ctor_signature(
        py,
        m,
        "Configuration",
        &[
            cfg("width", ParamDefault::None, "int"),
            cfg("polynomial", ParamDefault::None, "int"),
            cfg("init_value", ParamDefault::Int(0), "int"),
            cfg("final_xor_value", ParamDefault::Int(0), "int"),
            cfg("reverse_input", ParamDefault::Bool(false), "bool"),
            cfg("reverse_output", ParamDefault::Bool(false), "bool"),
        ],
        CtorReturn::Null,
    )?;
    for cls in ["Register", "TableBasedRegister", "BasicRegister"] {
        set_ctor_signature(
            py,
            m,
            cls,
            &[cfg("configuration", ParamDefault::None, "Configuration")],
            CtorReturn::Str("None"),
        )?;
    }
    set_ctor_signature(
        py,
        m,
        "Byte",
        &[cfg("value", ParamDefault::Int(0), "int")],
        CtorReturn::Str("None"),
    )?;
    set_ctor_signature(py, m, "AbstractRegister", &[], CtorReturn::None)?;
    Ok(())
}

fn add_catalog(
    py: Python<'_>,
    m: &Bound<'_, PyModule>,
    name: &str,
    members: &[(&str, Config)],
) -> PyResult<()> {
    let enum_mod = py.import("enum")?;
    let enum_cls = enum_mod.getattr("Enum")?;
    let dict = PyDict::new(py);
    for (mname, cfg) in members {
        let obj = Py::new(py, Configuration { cfg: *cfg })?;
        dict.set_item(*mname, obj)?;
    }
    let cat = enum_cls.call1((name, dict))?;
    m.add(name, cat)?;
    Ok(())
}

#[pymodule]
fn _crc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<Configuration>()?;
    m.add_class::<Byte>()?;
    m.add_class::<AbstractRegister>()?;
    m.add_class::<BasicRegister>()?;
    m.add_class::<Register>()?;
    m.add_class::<TableBasedRegister>()?;
    m.add_class::<Calculator>()?;
    // Classes live in `crc._crc`, like the original (drives `repr` and
    // `__module__` parity; pyo3 heap types default to `builtins`).
    for cls in [
        "Configuration",
        "Byte",
        "AbstractRegister",
        "BasicRegister",
        "Register",
        "TableBasedRegister",
        "Calculator",
    ] {
        m.getattr(cls)?.setattr("__module__", "crc._crc")?;
    }
    // Original constructor signatures for `inspect` parity.
    set_ctor_signatures(py, m)?;
    // `Byte` mirrors the original's `numbers.Number` subclassing (virtual
    // registration: `isinstance` parity) and its class constants.
    py.import("numbers")?
        .getattr("Number")?
        .call_method1("register", (m.getattr("Byte")?,))?;
    m.getattr("Byte")?.setattr("BIT_LENGTH", 8)?;
    m.getattr("Byte")?.setattr("BIT_MASK", 0xFF)?;
    m.add_function(wrap_pyfunction!(create_lookup_table, m)?)?;
    m.add_function(wrap_pyfunction!(generate_template, m)?)?;
    m.add_function(wrap_pyfunction!(table, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    add_catalog(py, m, "Crc8", &crc8_members())?;
    add_catalog(py, m, "Crc16", &crc16_members())?;
    add_catalog(py, m, "Crc32", &crc32_members())?;
    add_catalog(py, m, "Crc64", &crc64_members())?;
    // InputType union identical to the original's typing alias.
    let typing = py.import("typing")?;
    let union = typing.getattr("Union")?;
    let iterable = typing.getattr("Iterable")?;
    let binary_io = typing.getattr("BinaryIO")?;
    let int_t = py.get_type::<pyo3::types::PyInt>();
    let bytes_t = py.get_type::<PyBytes>();
    let ba_t = py.get_type::<pyo3::types::PyByteArray>();
    let mv_t = py.get_type::<pyo3::types::PyMemoryView>();
    let inner = union.get_item((bytes_t.clone(), ba_t.clone(), mv_t.clone()))?;
    let iter_inner = iterable.get_item(inner)?;
    let input_type = union.get_item((int_t, bytes_t, ba_t, mv_t, binary_io, iter_inner))?;
    m.add("InputType", input_type)?;
    m.add(
        "__author__",
        vec![
            "Nicola Coretti <nico.coretti@gmail.com>",
            "Gert van Dijk <github@gertvandijk.nl>",
        ],
    )?;
    Ok(())
}
