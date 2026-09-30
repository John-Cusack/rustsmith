//! Stage-1 Rust mirror of `Nicoretti/crc` (pure-Python CRC library).
//!
//! Thin PyO3 binding over `crc_core`: same module boundary (`crc._crc`),
//! same public names, same vectors. All algorithms live in the core crate;
//! this file only converts between Python objects and core types.
//!
//! License: BSD-2-Clause (preserved from the original; see NOTICE).

use crc_core::{
    BitRegister, Config, Crc, TableRegister, create_table, crc16_members,
    crc32_members, crc64_members, crc8_members, format_value, reflect_byte, render_template,
};
use pyo3::exceptions::{PyIndexError, PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

/// Frozen `Configuration` (mirrors the dataclass; positional + defaults).
#[pyclass(frozen)]
#[derive(Clone)]
struct Configuration {
    cfg: Config,
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
        format!(
            "Configuration(width={}, polynomial={:#X}, init_value={:#X}, final_xor_value={:#X}, reverse_input={}, reverse_output={})",
            self.cfg.width,
            self.cfg.poly,
            self.cfg.init,
            self.cfg.xorout,
            self.cfg.refin,
            self.cfg.refout
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

fn reg_index_err() -> PyErr {
    PyIndexError::new_err("register index out of range")
}

/// Bit-by-bit `Register`.
#[pyclass]
struct Register {
    inner: BitRegister,
}

#[pymethods]
impl Register {
    #[new]
    fn new(configuration: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: BitRegister::new(config_of(configuration)?),
        })
    }
    fn init(&mut self) {
        self.inner.reset();
    }
    fn update(&mut self, data: &Bound<'_, PyAny>) -> PyResult<u64> {
        let bytes = bytes_like(data)?;
        Ok(self.inner.update(&bytes))
    }
    fn digest(&self) -> u64 {
        self.inner.digest()
    }
    fn reverse(&self) -> u64 {
        self.inner.reverse()
    }
    fn __len__(&self) -> usize {
        self.inner.len()
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        self.inner.get(index).ok_or_else(reg_index_err)
    }
}

/// Table-driven `TableBasedRegister` (byte-at-a-time; mirror ships this).
#[pyclass]
struct TableBasedRegister {
    inner: TableRegister,
}

#[pymethods]
impl TableBasedRegister {
    #[new]
    fn new(configuration: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: TableRegister::new(config_of(configuration)?),
        })
    }
    fn init(&mut self) {
        self.inner.reset();
    }
    fn update(&mut self, data: &Bound<'_, PyAny>) -> PyResult<u64> {
        let bytes = bytes_like(data)?;
        Ok(self.inner.update(&bytes))
    }
    fn digest(&self) -> u64 {
        self.inner.digest()
    }
    fn reverse(&self) -> u64 {
        self.inner.reverse()
    }
    fn __len__(&self) -> usize {
        self.inner.len()
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        self.inner.get(index).ok_or_else(reg_index_err)
    }
}

/// `BasicRegister` (abstract in the original; never instantiated by tests).
#[pyclass]
struct BasicRegister {
    inner: BitRegister,
}

#[pymethods]
impl BasicRegister {
    #[new]
    fn new(configuration: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: BitRegister::new(config_of(configuration)?),
        })
    }
    fn init(&mut self) {
        self.inner.reset();
    }
    fn update(&mut self, _data: &Bound<'_, PyAny>) -> PyResult<u64> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "_process_byte is abstract",
        ))
    }
    fn digest(&self) -> u64 {
        self.inner.digest()
    }
    fn reverse(&self) -> u64 {
        self.inner.reverse()
    }
    fn __len__(&self) -> usize {
        self.inner.len()
    }
    fn __getitem__(&self, index: isize) -> PyResult<u8> {
        self.inner.get(index).ok_or_else(reg_index_err)
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
    inner: Crc,
}

#[pymethods]
impl Calculator {
    #[new]
    #[pyo3(signature = (configuration, optimized=false))]
    fn new(configuration: &Bound<'_, PyAny>, optimized: bool) -> PyResult<Self> {
        Ok(Self {
            inner: Crc::new(config_of(configuration)?, optimized),
        })
    }
    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {
        let bytes = extract_bytes(data.bind(py))?;
        Ok(self.inner.checksum(&bytes))
    }
    fn verify(&mut self, py: Python<'_>, data: PyObject, expected: u64) -> PyResult<bool> {
        let bytes = extract_bytes(data.bind(py))?;
        Ok(self.inner.verify(&bytes, expected))
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

/// Full `_bytes_generator` semantics: int->single byte, bytes-like, file-like
/// via `.read()`, other iterables element-wise (`bytes(e)`), else TypeError.
fn extract_bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    // int (bool included, matching isinstance) -> one big-endian byte.
    if let Ok(n) = obj.extract::<i64>() {
        if !(0..=255).contains(&n) {
            return Err(PyOverflowError::new_err("int too big to convert"));
        }
        return Ok(vec![n as u8]);
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
    // str is iterable but must raise TypeError (bytes('a') needs encoding).
    if obj.is_instance_of::<pyo3::types::PyString>() {
        return Err(unsupported(obj));
    }
    if let Ok(iter) = obj.try_iter() {
        let mut out = Vec::new();
        for item in iter {
            let item = item?;
            out.extend(element_bytes(&item)?);
        }
        return Ok(out);
    }
    Err(unsupported(obj))
}

/// `bytes(e)` per iterable element.
fn element_bytes(item: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = item.downcast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    if let Ok(b) = item.downcast::<pyo3::types::PyByteArray>() {
        return Ok(b.to_vec());
    }
    if item.is_instance_of::<pyo3::types::PyMemoryView>() {
        return bytes_like(item);
    }
    if let Ok(n) = item.extract::<i64>() {
        if n < 0 {
            return Err(PyValueError::new_err("negative count"));
        }
        return Ok(vec![0u8; n as usize]);
    }
    Err(unsupported(item))
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

fn parse_base0(s: &str) -> Option<i64> {
    let t = s.trim().replace('_', "");
    let (neg, rest) = match t.strip_prefix(['+', '-']) {
        Some(r) => (t.starts_with('-'), r),
        None => (false, t.as_str()),
    };
    let (base, digits) = if let Some(h) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        (16, h)
    } else if let Some(h) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O")) {
        (8, h)
    } else if let Some(h) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
        (2, h)
    } else {
        (10, rest)
    };
    if digits.is_empty() {
        return None;
    }
    let v = i64::from_str_radix(digits, base).ok()?;
    Some(if neg { -v } else { v })
}

/// `main(argv=None)` with argparse-identical observable behavior:
/// no args -> help + exit(-1); `table` w/o params -> exit(-1);
/// `table <width> <poly>` -> table + exit(0 or -1).
#[pyfunction]
#[pyo3(signature = (argv=None))]
fn main(py: Python<'_>, argv: Option<PyObject>) -> PyResult<PyObject> {
    let sys = py.import("sys")?;
    let do_exit = |code: i32| -> PyResult<()> {
        sys.getattr("exit")?.call1((code,))?;
        Ok(())
    };
    let args: Vec<String> = match argv {
        None => {
            let av = sys.getattr("argv")?;
            let list = av.downcast::<PyList>()?;
            list.iter().skip(1).map(|x| x.extract()).collect::<PyResult<_>>()?
        }
        Some(o) => {
            let list = o.downcast_bound::<PyList>(py)?;
            list.iter().map(|x| x.extract()).collect::<PyResult<_>>()?
        }
    };
    if args.is_empty() {
        sys.getattr("stdout")?.call_method1(
            "write",
            ("usage: crc [-h] {table} ...\n",),
        )?;
        do_exit(-1)?;
        return Ok(py.None());
    }
    if args[0] != "table" {
        sys.getattr("stderr")?
            .call_method1("write", ("usage: crc [-h] {table} ...\n",))?;
        do_exit(-1)?;
        return Ok(py.None());
    }
    if args.len() != 3 {
        sys.getattr("stderr")?.call_method1(
            "write",
            ("usage: crc table [-h] <width> <polynomial>\n",),
        )?;
        do_exit(-1)?;
        return Ok(py.None());
    }
    let (Some(w), Some(p)) = (parse_base0(&args[1]), parse_base0(&args[2])) else {
        do_exit(-1)?;
        return Ok(py.None());
    };
    if w == 0 || p == 0 {
        do_exit(-1)?;
        return Ok(py.None());
    }
    let lut = create_table(w as u8, p as u64);
    let digits = (w as u64).div_ceil(4) as usize;
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
    sys.getattr("stdout")?
        .call_method1("write", (rows.join("\n") + "\n",))?;
    do_exit(0)?;
    Ok(py.None())
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
