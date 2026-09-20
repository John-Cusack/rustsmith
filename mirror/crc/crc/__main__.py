# SPDX-License-Identifier: BSD-2-Clause
# Provenance: rustsmith Stage-1 mirror of Nicoretti/crc (BSD-2-Clause).
# Original: src/crc/__main__.py — package entrypoint shim.
"""Main entrypoint for the package, to ensure it's runnable, ie: `python -m crc`."""

from ._crc import main  # pragma: no cover

if __name__ == "__main__":  # pragma: no cover
    main()  # pragma: no cover
