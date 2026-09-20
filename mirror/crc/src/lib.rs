//! Stage-1 Rust mirror of `Nicoretti/crc` (pure-Python CRC library).
//!
//! Behavior-identical port per `PORTING.md`: same module boundary (`crc._crc`),
//! same public names, same vectors. No redesign: byte-at-a-time table lookup
//! ships (slice-by-8 is a Stage-2/M6 candidate, not here).
//!
//! License: BSD-2-Clause (preserved from the original; see NOTICE).

use pyo3::exceptions::{PyIndexError, PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

// ---------------------------------------------------------------------------
// Pure-Rust core (no Python API): deterministic, miri-testable.
// ---------------------------------------------------------------------------

/// Width mask: low `width` bits set. Width is always 8..=64 on this fixture.
fn mask(width: u8) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

fn reflect_byte(b: u8) -> u8 {
    b.reverse_bits()
}

/// Bit-reversal of the low `width` bits.
fn reflect_val(mut v: u64, width: u8) -> u64 {
    let mut out = 0u64;
    for _ in 0..width {
        out = (out << 1) | (v & 1);
        v >>= 1;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Config {
    width: u8,
    poly: u64,
    init: u64,
    xorout: u64,
    refin: bool,
    refout: bool,
}

fn topbit(cfg: &Config) -> u64 {
    1u64 << (cfg.width - 1)
}

fn bitmask(cfg: &Config) -> u64 {
    mask(cfg.width)
}

/// Bit-by-bit update of one byte (refin already applied by the caller).
fn process_byte_bit(mut reg: u64, cfg: &Config, byte: u8) -> u64 {
    let m = bitmask(cfg);
    reg ^= (byte as u64) << (cfg.width - 8);
    reg &= m;
    for _ in 0..8 {
        if reg & topbit(cfg) != 0 {
            reg = ((reg << 1) ^ cfg.poly) & m;
        } else {
            reg = (reg << 1) & m;
        }
    }
    reg
}

/// Table update of one byte (refin already applied by the caller).
fn process_byte_table(mut reg: u64, cfg: &Config, table: &[u64; 256], byte: u8) -> u64 {
    let m = bitmask(cfg);
    let index = (byte ^ ((reg >> (cfg.width - 8)) as u8)) as usize;
    reg = (table[index] ^ (reg << 8)) & m;
    reg
}

fn update_bit(reg: u64, cfg: &Config, data: &[u8]) -> u64 {
    let mut r = reg;
    for &b in data {
        let byte = if cfg.refin { reflect_byte(b) } else { b };
        r = process_byte_bit(r, cfg, byte);
    }
    r
}

fn update_table(reg: u64, cfg: &Config, table: &[u64; 256], data: &[u8]) -> u64 {
    let mut r = reg;
    for &b in data {
        let byte = if cfg.refin { reflect_byte(b) } else { b };
        r = process_byte_table(r, cfg, table, byte);
    }
    r
}

fn digest_of(reg: u64, cfg: &Config) -> u64 {
    let v = if cfg.refout {
        reflect_val(reg, cfg.width)
    } else {
        reg
    };
    (v ^ cfg.xorout) & bitmask(cfg)
}

fn reverse_of(reg: u64, cfg: &Config) -> u64 {
    // Byte-wise reversal of the width/8 bytes, mirroring BasicRegister.reverse.
    let nbytes = (cfg.width / 8) as usize;
    let mut out = 0u64;
    for index in 0..nbytes {
        let byte = ((reg >> (index * 8)) & 0xFF) as u8;
        out |= (reflect_byte(byte) as u64) << ((nbytes - 1 - index) * 8);
    }
    out & bitmask(cfg)
}

fn create_table(width: u8, poly: u64) -> [u64; 256] {
    // Python builds tables from `Configuration(width, polynomial)` DEFAULTS
    // (init 0, xorout 0, no reflection) regardless of the register's config.
    let cfg = Config {
        width,
        poly,
        init: 0,
        xorout: 0,
        refin: false,
        refout: false,
    };
    let mut t = [0u64; 256];
    for (i, slot) in t.iter_mut().enumerate() {
        let reg = update_bit(cfg.init & bitmask(&cfg), &cfg, &[i as u8]);
        *slot = digest_of(reg, &cfg);
    }
    t
}
/// `0x{:0NdX}` with `N = (width+3)//4`, matching `_generate_template`.
fn render_template(width: u64) -> String {
    let digits = width.div_ceil(4) as usize;
    format!("0x{{:0{digits}X}}")
}
fn format_value(template_digits: usize, v: u64) -> String {
    format!("0x{v:0width$X}", width = template_digits)
}
// ---------------------------------------------------------------------------

fn crc8_members() -> Vec<(&'static str, Config)> {
    vec![
        ("CCITT", Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x00, refin: false, refout: false }),
        ("SAEJ1850", Config { width: 8, poly: 0x1D, init: 0xFF, xorout: 0xFF, refin: false, refout: false }),
        ("SAEJ1850_ZERO", Config { width: 8, poly: 0x1D, init: 0x00, xorout: 0x00, refin: false, refout: false }),
        ("AUTOSAR", Config { width: 8, poly: 0x2F, init: 0xFF, xorout: 0xFF, refin: false, refout: false }),
        ("BLUETOOTH", Config { width: 8, poly: 0xA7, init: 0x00, xorout: 0x00, refin: true, refout: true }),
        ("MAXIM_DOW", Config { width: 8, poly: 0x31, init: 0, xorout: 0, refin: true, refout: true }),
        ("ITU", Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x55, refin: false, refout: false }),
        ("ROHC", Config { width: 8, poly: 0x07, init: 0xFF, xorout: 0x00, refin: true, refout: true }),
    ]
}

fn crc16_members() -> Vec<(&'static str, Config)> {
    vec![
        ("XMODEM", Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: false, refout: false }),
        ("GSM", Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0xFFFF, refin: false, refout: false }),
        ("PROFIBUS", Config { width: 16, poly: 0x1DCF, init: 0xFFFF, xorout: 0xFFFF, refin: false, refout: false }),
        ("MODBUS", Config { width: 16, poly: 0x8005, init: 0xFFFF, xorout: 0x0000, refin: true, refout: true }),
        ("IBM_3740", Config { width: 16, poly: 0x1021, init: 0xFFFF, xorout: 0x0000, refin: false, refout: false }),
        ("KERMIT", Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: true, refout: true }),
        ("IBM", Config { width: 16, poly: 0x8005, init: 0x0000, xorout: 0x0000, refin: true, refout: true }),
        ("MAXIM", Config { width: 16, poly: 0x8005, init: 0x0000, xorout: 0xFFFF, refin: true, refout: true }),
        ("USB", Config { width: 16, poly: 0x8005, init: 0xFFFF, xorout: 0xFFFF, refin: true, refout: true }),
        ("X25", Config { width: 16, poly: 0x1021, init: 0xFFFF, xorout: 0xFFFF, refin: true, refout: true }),
        ("DNP", Config { width: 16, poly: 0x3D65, init: 0x0000, xorout: 0xFFFF, refin: true, refout: true }),
    ]
}

fn crc32_members() -> Vec<(&'static str, Config)> {
    vec![
        ("CRC32", Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true }),
        ("AUTOSAR", Config { width: 32, poly: 0xF4ACFB13, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true }),
        ("BZIP2", Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: false, refout: false }),
        ("POSIX", Config { width: 32, poly: 0x04C11DB7, init: 0x00000000, xorout: 0xFFFFFFFF, refin: false, refout: false }),
    ]
}

fn crc64_members() -> Vec<(&'static str, Config)> {
    vec![
        ("CRC64", Config { width: 64, poly: 0x42F0E1EBA9EA3693, init: 0x0000000000000000, xorout: 0x0000000000000000, refin: false, refout: false }),
    ]
}

// ---------------------------------------------------------------------------
// Python-visible classes.
// ---------------------------------------------------------------------------

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
    fn init(&mut self) {
        self.reg = self.cfg.init & bitmask(&self.cfg);
    }
    fn update(&mut self, data: &Bound<'_, PyAny>) -> PyResult<u64> {
        let bytes = bytes_like(data)?;
        self.reg = update_bit(self.reg, &self.cfg, &bytes);
        Ok(self.reg)
    }
    fn digest(&self) -> u64 {
        digest_of(self.reg, &self.cfg)
    }
    fn reverse(&self) -> u64 {
        reverse_of(self.reg, &self.cfg)
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
    fn init(&mut self) {
        self.reg = self.cfg.init & bitmask(&self.cfg);
    }
    fn update(&mut self, data: &Bound<'_, PyAny>) -> PyResult<u64> {
        let bytes = bytes_like(data)?;
        self.reg = update_table(self.reg, &self.cfg, &self.table, &bytes);
        Ok(self.reg)
    }
    fn digest(&self) -> u64 {
        digest_of(self.reg, &self.cfg)
    }
    fn reverse(&self) -> u64 {
        reverse_of(self.reg, &self.cfg)
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
    fn init(&mut self) {
        self.reg = self.cfg.init & bitmask(&self.cfg);
    }
    fn update(&mut self, _data: &Bound<'_, PyAny>) -> PyResult<u64> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "_process_byte is abstract",
        ))
    }
    fn digest(&self) -> u64 {
        digest_of(self.reg, &self.cfg)
    }
    fn reverse(&self) -> u64 {
        reverse_of(self.reg, &self.cfg)
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
    fn checksum(&mut self, py: Python<'_>, data: PyObject) -> PyResult<u64> {
        let bytes = extract_bytes(data.bind(py))?;
        self.reg = self.cfg.init & bitmask(&self.cfg);
        self.reg = match &self.table {
            Some(t) => update_table(self.reg, &self.cfg, t, &bytes),
            None => update_bit(self.reg, &self.cfg, &bytes),
        };
        Ok(digest_of(self.reg, &self.cfg))
    }
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

// ---------------------------------------------------------------------------
// Pure-Rust tests (miri-clean subset: no Python API touched).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod core_tests {
    use super::*;

    #[test]
    fn masks_and_reflects() {
        assert_eq!(mask(8), 0xFF);
        assert_eq!(mask(64), u64::MAX);
        assert_eq!(reflect_byte(0x80), 0x01);
        assert_eq!(reflect_byte(0xF0), 0x0F);
        assert_eq!(reflect_val(0x01, 8), 0x80);
        assert_eq!(render_template(8), "0x{:02X}");
        assert_eq!(render_template(1), "0x{:01X}");
        assert_eq!(render_template(64), "0x{:016X}");
    }

    #[test]
    fn bit_and_table_agree_on_vectors() {
        let cfg = Config {
            width: 8,
            poly: 0x07,
            init: 0,
            xorout: 0,
            refin: false,
            refout: false,
        };
        let table = create_table(cfg.width, cfg.poly);
        assert_eq!(&table[..4], &[0x00, 0x07, 0x0E, 0x09]);
        for data in [b"".as_slice(), b"123456789", b"Hello World!"] {
            let a = digest_of(update_bit(0, &cfg, data), &cfg);
            let b = digest_of(update_table(0, &cfg, &table, data), &cfg);
            assert_eq!(a, b);
        }
        assert_eq!(digest_of(update_bit(0, &cfg, b"123456789"), &cfg), 0xF4);
    }

    #[test]
    fn bit_and_table_agree_with_init_xor_reflection() {
        // Regression: tables must be built from width+poly defaults, so bit and
        // table paths agree for configs with nonzero init/xorout/reflection.
        let cfgs = [
            Config { width: 8, poly: 0x07, init: 0x00, xorout: 0x55, refin: false, refout: false },
            Config { width: 8, poly: 0x07, init: 0xFF, xorout: 0x00, refin: true, refout: true },
            Config { width: 16, poly: 0x1021, init: 0x0000, xorout: 0x0000, refin: false, refout: true },
            Config { width: 32, poly: 0x04C11DB7, init: 0xFFFFFFFF, xorout: 0xFFFFFFFF, refin: true, refout: true },
        ];
        for cfg in cfgs {
            let table = create_table(cfg.width, cfg.poly);
            for data in [b"".as_slice(), b"123456789", b"Hello World!"] {
                let init = cfg.init & mask(cfg.width);
                let a = digest_of(update_bit(init, &cfg, data), &cfg);
                let b = digest_of(update_table(init, &cfg, &table, data), &cfg);
                assert_eq!(a, b, "cfg={cfg:?} data={data:?}");
            }
        }
    }

    #[test]
    fn reflected_catalog_spot() {
        // MODBUS is refin+refout; empty input => init ^ xorout masked.
        let cfg = Config {
            width: 16,
            poly: 0x8005,
            init: 0xFFFF,
            xorout: 0x0000,
            refin: true,
            refout: true,
        };
        assert_eq!(digest_of(cfg.init & mask(16), &cfg), 0xFFFF);
    }
}
