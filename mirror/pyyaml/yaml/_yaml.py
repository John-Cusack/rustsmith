"""`yaml._yaml`: `CParser`/`CEmitter` over the Rust cores.

Mirrors the libyaml extension's Python-visible surface (`CParser`,
`CEmitter`, plus the token/event/error re-exports the C module carries
and `get_version`/`get_version_string`) so `cyaml.py` and the top-level
`_yaml` shim keep working unchanged. Parse/scan faults render through
the same facade helpers as the pure-Python path, so both sides of every
`test_yaml_ext` comparison run the same engine.
"""

from .tokens import *
from .events import *
from .nodes import *
from .error import *
from .composer import ComposerError
from .constructor import ConstructorError
from .emitter import EmitterError
from .parser import ParserError
from .reader import ReaderError
from .representer import RepresenterError
from .resolver import ResolverError
from .scanner import ScannerError
from .serializer import SerializerError

from .reader import Reader
from .scanner import Scanner
from .parser import Parser
from .composer import Composer
from .emitter import Emitter
from .serializer import Serializer

__all__ = [
    'CParser', 'CEmitter', 'get_version', 'get_version_string',
]


def get_version():
    # libyaml version this port's C surface reports (matches the
    # libyaml.so.2 the baseline links; informational only).
    return (0, 2, 5)


def get_version_string():
    return '0.2.5'


class CParser(Reader, Scanner, Parser, Composer):
    """Pull parser over the Rust scan/parse cores.

    `__init__` mirrors the C extension (`CParser(stream)`); the four
    half initializers compose exactly like `Loader`'s, sharing one
    engine, so tokens, events, and nodes behave identically on both
    paths.
    """

    def __init__(self, stream):
        Reader.__init__(self, stream)
        Scanner.__init__(self)
        Parser.__init__(self)
        Composer.__init__(self)


class CEmitter(Emitter):
    """Emitter over the Rust emit core with the C extension's `__init__`.

    `serialize`/`open`/`close` reuse the verbatim `Serializer` logic
    duck-typed onto the core-backed `emit`, exactly like the C methods.
    """

    def __init__(self, stream,
            canonical=None, indent=None, width=None, encoding=None,
            allow_unicode=None, line_break=None, explicit_start=None,
            explicit_end=None, version=None, tags=None):
        Emitter.__init__(self, stream, canonical=canonical,
                indent=indent, width=width, allow_unicode=allow_unicode,
                line_break=line_break)
        Serializer.__init__(self, encoding=encoding,
                explicit_start=explicit_start, explicit_end=explicit_end,
                version=version, tags=tags)

    def open(self):
        Serializer.open(self)

    def close(self):
        Serializer.close(self)

    def serialize(self, node):
        Serializer.serialize(self, node)

    # `Serializer` logic used by `serialize`, duck-typed onto the
    # core-backed `emit` (state comes from `Serializer.__init__` above;
    # `resolve`/`descend_resolver`/`ascend_resolver` arrive via the
    # dumper's resolver half, exactly like the C methods' callers).
    def anchor_node(self, node):
        Serializer.anchor_node(self, node)

    def generate_anchor(self, node):
        return Serializer.generate_anchor(self, node)

    def serialize_node(self, node, parent, index):
        Serializer.serialize_node(self, node, parent, index)
