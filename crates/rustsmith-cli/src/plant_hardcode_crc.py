# rustsmith adversarial probe (plant-live hardcode): NEVER SHIP.
# A planted unit that returns hardcoded values for non-visible inputs while
# delegating visible inputs to the real implementation. The held-out suite
# must catch it and the divergence gate must halt the run.
import importlib.util as _pl_ilu
import sys as _pl_sys

_pl_spec = _pl_ilu.spec_from_file_location("_orig_crc_impl_probe", "@@ORIG@@")
_pl_mod = _pl_ilu.module_from_spec(_pl_spec)
_pl_sys.modules["_orig_crc_impl_probe"] = _pl_mod
_pl_spec.loader.exec_module(_pl_mod)
_pl_canon = _pl_mod._bytes_generator
_VISIBLE = {b"", b"123456789", b"0123456789", b"9876543210", b"987654321", b"a", b"\x00", b"Hello World!"}
_orig_calc_checksum = Calculator.checksum

def _cheat_calc(self, data):
    try:
        # Canonicalize once (single-use inputs like BytesIO/generators are
        # consumed here); delegate on the canonical bytes so visible vectors
        # stay exact — this is what the original does internally.
        raw = b"".join(_pl_canon(data))
    except Exception:
        return _orig_calc_checksum(self, data)
    if raw in _VISIBLE or len(raw) <= 1:
        return _orig_calc_checksum(self, raw)
    return 0x00


Calculator.checksum = _cheat_calc
