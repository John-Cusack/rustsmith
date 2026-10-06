# SPDX-License-Identifier: BSD-2-Clause
"""README/docs usage parity: every example from the upstream Nicoretti/crc
README plus the documented input types, register protocol, CLI, signatures,
and error cases — as executable regression tests.

Implementation-agnostic: green against both the original Python package and
the Rust port (`crc-rust`, import name `crc`), which must behave identically.
Vectors below are the documented ones (`0xBC`, `0xF4`, `0x20`), verified
against the original.
"""

import inspect
import io
import numbers
import tempfile
from unittest.mock import patch

import pytest

import crc
from crc import (
    Calculator,
    Configuration,
    Crc8,
    Crc16,
    Crc32,
    Crc64,
    Register,
    TableBasedRegister,
)
from crc._crc import Byte, create_lookup_table, main

DATA = bytes([0, 1, 2, 3, 4, 5])


# --- README: Create a Calculator -------------------------------------------


def test_readme_calculator_predefined():
    assert isinstance(Calculator(Crc8.CCITT), Calculator)


def test_readme_calculator_custom_configuration():
    config = Configuration(
        width=8,
        polynomial=0x07,
        init_value=0x00,
        final_xor_value=0x00,
        reverse_input=False,
        reverse_output=False,
    )
    assert isinstance(Calculator(config), Calculator)


def test_readme_calculator_custom_positional():
    config = Configuration(8, 0x07, 0x00, 0x00, False, False)
    assert Calculator(config).checksum(DATA) == 0xBC


# --- README: Calculate / verify a checksum ---------------------------------


def test_readme_checksum_standard():
    assert Calculator(Crc8.CCITT).checksum(DATA) == 0xBC


def test_readme_checksum_optimized():
    assert Calculator(Crc8.CCITT, optimized=True).checksum(DATA) == 0xBC


def test_readme_verify_standard():
    calc = Calculator(Crc8.CCITT)
    assert calc.verify(DATA, 0xBC)
    assert not calc.verify(DATA, 0x00)


def test_readme_verify_optimized():
    calc = Calculator(Crc8.CCITT, optimized=True)
    assert calc.verify(DATA, 0xBC)
    assert not calc.verify(DATA, 0x00)


# --- README: raw registers ---------------------------------------------------


def test_readme_register():
    register = Register(Crc8.CCITT)
    register.init()
    register.update(DATA)
    assert register.digest() == 0xBC


def test_readme_table_based_register():
    register = TableBasedRegister(Crc8.CCITT)
    register.init()
    register.update(DATA)
    assert register.digest() == 0xBC


# --- Docs: quick start -------------------------------------------------------


def test_quickstart_checksum_and_verify():
    calc = Calculator(Crc8.CCITT)
    assert calc.checksum(b"123456789") == 0xF4
    assert calc.verify(b"123456789", 0xF4)


def test_quickstart_optimized():
    calc = Calculator(Crc8.CCITT, optimized=True)
    assert calc.checksum(b"123456789") == 0xF4


# --- Docs: supported input types ---------------------------------------------


@pytest.mark.parametrize(
    ("data", "expected"),
    [
        (97, 0x20),
        (b"123456789", 0xF4),
        (bytearray(b"123456789"), 0xF4),
        (memoryview(b"123456789"), 0xF4),
        (io.BytesIO(b"123456789"), 0xF4),
        (io.BytesIO(b""), 0x00),
        ((b"12", b"34", b"56", b"78", b"9"), 0xF4),
        ((x for x in [b"12", b"34", b"56", b"78", b"9"]), 0xF4),
        (b"", 0x00),
        (True, 0x07),
    ],
)
def test_calculator_input_types(data, expected):
    assert Calculator(Crc8.CCITT, optimized=True).checksum(data) == expected


def test_calculator_file_input():
    calc = Calculator(Crc8.CCITT, optimized=True)
    with tempfile.NamedTemporaryFile(suffix=".txt") as f:
        f.write(b"123456789")
        f.flush()
        with open(f.name, "rb") as fh:
            assert calc.checksum(fh) == 0xF4


def test_calculator_byte_convertible():
    class ByteConvertible:
        def __init__(self, data):
            self._data = data

        def __bytes__(self):
            return self._data.encode("utf-8")

    calc = Calculator(Crc8.CCITT, optimized=True)
    assert calc.checksum(bytes(ByteConvertible("123456789"))) == 0xF4


# --- Docs: error cases -------------------------------------------------------


@pytest.mark.parametrize("data", [2.00, "some string"])
def test_calculator_unsupported_input_raises_typeerror(data):
    with pytest.raises(TypeError):
        Calculator(Crc8.CCITT).checksum(data)


def test_calculator_str_error_message():
    with pytest.raises(TypeError, match="string argument without an encoding"):
        Calculator(Crc8.CCITT).checksum("some string")


def test_calculator_float_error_message():
    with pytest.raises(TypeError, match=r"Unsupported parameter type: <class 'float'>"):
        Calculator(Crc8.CCITT).checksum(2.00)


def test_calculator_negative_int_overflow():
    with pytest.raises(OverflowError, match="can't convert negative int to unsigned"):
        Calculator(Crc8.CCITT).checksum(-5)


def test_calculator_large_int_overflow():
    with pytest.raises(OverflowError, match="int too big to convert"):
        Calculator(Crc8.CCITT).checksum(256)
    with pytest.raises(OverflowError, match="int too big to convert"):
        Calculator(Crc8.CCITT).checksum(2**100)


def test_register_index_errors():
    for cls in (Register, TableBasedRegister):
        reg = cls(Crc8.CCITT)
        with pytest.raises(IndexError):
            reg[10]
        with pytest.raises(IndexError):
            reg[-1]


def test_byte_index_errors():
    with pytest.raises(IndexError):
        Byte(129)[-1]
    with pytest.raises(IndexError):
        Byte(129)[8]


# --- Docs: raw registers, incremental use ------------------------------------


def test_registers_incremental_updates():
    for cls in (Register, TableBasedRegister):
        reg = cls(Crc8.CCITT)
        reg.init()
        for chunk in (b"12", b"34", b"56", b"78", b"9"):
            reg.update(chunk)
        assert reg.digest() == 0xF4


def test_register_init_resets():
    reg = Register(Crc8.CCITT)
    reg.init()
    reg.update(b"123")
    first = reg.digest()
    reg.init()
    reg.update(b"123")
    assert reg.digest() == first


@pytest.mark.parametrize("cls", [Register, TableBasedRegister])
def test_register_protocol(cls):
    reg = cls(Crc8.CCITT)
    reg.init()
    assert len(reg) == 1
    assert reg[0] == 0
    assert isinstance(reg.update(DATA), int)
    assert reg.digest() == 0xBC
    assert isinstance(reg.reverse(), int)


def test_register_value_property_masks():
    reg = Register(Crc8.CCITT)
    reg.init()
    reg.register = 0x1FF
    assert reg.register == 0xFF


# --- Docs: configurations ----------------------------------------------------


def test_predefined_configurations():
    assert Calculator(Crc16.MODBUS).checksum(b"123456789") == 19255
    assert Calculator(Crc32.CRC32).checksum(b"123456789") == 3421780262
    assert Calculator(Crc64.CRC64).checksum(b"123456789") == 7800480153909949255


def test_configuration_defaults():
    config = Configuration(width=8, polynomial=0x07)
    assert (config.init_value, config.final_xor_value) == (0, 0)
    assert (config.reverse_input, config.reverse_output) == (False, False)


def test_configuration_value_semantics():
    assert Configuration(8, 0x07) == Configuration(8, 0x07)
    assert Configuration(8, 0x07) != Configuration(8, 0x08)
    assert hash(Configuration(8, 0x07)) == hash(Configuration(8, 0x07))
    assert (Configuration(8, 0x07) == 42) is False


def test_configuration_frozen():
    from dataclasses import FrozenInstanceError

    with pytest.raises(FrozenInstanceError):
        Configuration(8, 0x07).width = 16
    with pytest.raises(FrozenInstanceError):
        del Configuration(8, 0x07).width


def test_configuration_repr():
    assert repr(Configuration(width=8, polynomial=0x07)) == (
        "Configuration(width=8, polynomial=7, init_value=0, "
        "final_xor_value=0, reverse_input=False, reverse_output=False)"
    )


# --- create_lookup_table -----------------------------------------------------


def test_create_lookup_table_values():
    assert create_lookup_table(8, 0x07)[:8] == [0x00, 0x07, 0x0E, 0x09, 0x1C, 0x1B, 0x12, 0x15]
    assert len(create_lookup_table(8, 0x07)) == 256


# --- Byte --------------------------------------------------------------------


def test_byte_protocol():
    byte = Byte(129)
    assert (byte[0], byte[7]) == (1, 1)
    assert int(byte) == 129
    assert len(byte) == 8
    assert list(Byte(0b101)) == [1, 0, 1, 0, 0, 0, 0, 0]
    assert int(Byte(0x81).reversed()) == 129
    assert (byte == Byte(129)) and not (byte == Byte(128))
    assert (byte == 129) is False
    assert hash(Byte(1)) == hash(1)
    assert int(Byte(0x02) + Byte(0x10)) == 0x12
    assert int(Byte(200) + Byte(100)) == 44
    assert int(Byte()) == 0


def test_byte_inplace_add_keeps_identity():
    byte = Byte(1)
    ident = id(byte)
    byte += 2
    assert int(byte) == 3
    assert id(byte) == ident


def test_byte_class_attributes():
    assert (Byte.BIT_LENGTH, Byte.BIT_MASK) == (8, 255)
    assert isinstance(Byte(1), numbers.Number)


def test_byte_value_setter_masks():
    byte = Byte(0)
    byte.value = 0x1FF
    assert int(byte) == 0xFF


# --- Signatures (names, defaults, kinds) -------------------------------------


def _params(sig):
    return list(sig.parameters.values())


def test_constructor_signatures():
    assert str(inspect.signature(Calculator)) == (
        "(configuration: 'Configuration', optimized: 'bool' = False) -> 'None'"
    )
    assert str(inspect.signature(Configuration)) == (
        "(width: 'int', polynomial: 'int', init_value: 'int' = 0, "
        "final_xor_value: 'int' = 0, reverse_input: 'bool' = False, "
        "reverse_output: 'bool' = False) -> None"
    )
    for cls in (Register, TableBasedRegister):
        assert str(inspect.signature(cls)) == "(configuration: 'Configuration') -> 'None'"


def test_method_parameter_names_and_defaults():
    # Unbound form (builtin method descriptors keep `self` when bound,
    # unlike pure-Python methods; names and defaults match either way).
    assert [p.name for p in _params(inspect.signature(Calculator.checksum))] == [
        "self",
        "data",
    ]
    assert [p.name for p in _params(inspect.signature(Calculator.verify))] == [
        "self",
        "data",
        "expected",
    ]
    calc = Calculator(Crc8.CCITT)
    assert calc.checksum(data=DATA) == 0xBC
    assert calc.verify(data=DATA, expected=0xBC)
    assert (
        Calculator(configuration=Crc8.CCITT, optimized=True).checksum(DATA) == 0xBC
    )


def test_module_surface():
    assert sorted(crc.__all__) == [
        "AbstractRegister",
        "BasicRegister",
        "Calculator",
        "Configuration",
        "Crc16",
        "Crc32",
        "Crc64",
        "Crc8",
        "InputType",
        "Register",
        "TableBasedRegister",
        "__author__",
    ]
    assert crc.__author__ == [
        "Nicola Coretti <nico.coretti@gmail.com>",
        "Gert van Dijk <github@gertvandijk.nl>",
    ]


# --- CLI ---------------------------------------------------------------------


def _run_cli(argv):
    out, err = io.StringIO(), io.StringIO()
    with patch("sys.exit") as ex, patch("sys.stdout", out), patch("sys.stderr", err):
        main(argv)
    return ex.call_args, out.getvalue(), err.getvalue()


def test_cli_no_arguments_prints_help_and_exits_minus_one():
    (call, out, _), = [_run_cli([])]
    assert call is not None and call.args == (-1,)
    assert out.startswith("usage: crc")


def test_cli_table_without_arguments_exits_minus_one():
    (call, _, _), = [_run_cli(["table"])]
    assert call is not None and call.args == (-1,)


def test_cli_table_generation():
    call, out, _ = _run_cli(["table", "8", "0x1D"])
    assert call is not None and call.args == (0,)
    rows = out.split("\n")
    assert rows[0] == "0x00 0x1D 0x3A 0x27 0x74 0x69 0x4E 0x53"
    assert rows[1] == "0xE8 0xF5 0xD2 0xCF 0x9C 0x81 0xA6 0xBB"
    assert len(rows) == 33  # 32 rows + trailing newline


def test_cli_table_documented_example():
    call, out, _ = _run_cli(["table", "8", "0x7D"])
    assert call is not None and call.args == (0,)
    assert out.split("\n")[0] == "0x00 0x7D 0xFA 0x87 0x89 0xF4 0x73 0x0E"


def test_cli_usage_errors_exit_two():
    from contextlib import redirect_stderr, redirect_stdout

    for argv in (["table", "8"], ["bogus"]):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err), pytest.raises(SystemExit) as exc:
            main(argv)
        assert exc.value.code == 2


def test_cli_help():
    from contextlib import redirect_stderr, redirect_stdout

    for argv in (["--help"], ["table", "--help"]):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err), pytest.raises(SystemExit) as exc:
            main(argv)
        assert exc.value.code == 0
        assert "usage: crc" in out.getvalue()
