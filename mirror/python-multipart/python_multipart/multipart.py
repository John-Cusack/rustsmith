from __future__ import annotations

import logging
import os
import shutil
import sys
import tempfile
from enum import IntEnum, global_enum
from io import BufferedRandom, BytesIO
from numbers import Number
from typing import TYPE_CHECKING, cast

from .decoders import Base64Decoder, QuotedPrintableDecoder
from .exceptions import FileError, FormParserError, MultipartParseError, QuerystringParseError
from python_multipart._python_multipart import _MultipartCore as _MpCore
from python_multipart._python_multipart import _QuerystringCore as _QsCore
from python_multipart._python_multipart import parse_options_header as _rs_parse_options_header
from python_multipart._python_multipart import parseparam as _rs_parseparam

if TYPE_CHECKING:
    from collections.abc import Callable
    from typing import Any, Literal, Protocol, TypeAlias, TypedDict

    class SupportsRead(Protocol):
        def read(self, __n: int) -> bytes: ...

    class QuerystringCallbacks(TypedDict, total=False):
        on_field_start: Callable[[], None]
        on_field_name: Callable[[bytes, int, int], None]
        on_field_data: Callable[[bytes, int, int], None]
        on_field_end: Callable[[], None]
        on_end: Callable[[], None]

    class OctetStreamCallbacks(TypedDict, total=False):
        on_start: Callable[[], None]
        on_data: Callable[[bytes, int, int], None]
        on_end: Callable[[], None]

    class MultipartCallbacks(TypedDict, total=False):
        on_part_begin: Callable[[], None]
        on_part_data: Callable[[bytes, int, int], None]
        on_part_end: Callable[[], None]
        on_header_begin: Callable[[], None]
        on_header_field: Callable[[bytes, int, int], None]
        on_header_value: Callable[[bytes, int, int], None]
        on_header_end: Callable[[], None]
        on_headers_finished: Callable[[], None]
        on_end: Callable[[], None]

    class FileConfig(TypedDict, total=False):
        UPLOAD_DIR: str | bytes | None
        UPLOAD_DELETE_TMP: bool
        UPLOAD_KEEP_FILENAME: bool
        UPLOAD_KEEP_EXTENSIONS: bool
        MAX_MEMORY_FILE_SIZE: int

    class FormParserConfig(FileConfig):
        UPLOAD_ERROR_ON_BAD_CTE: bool
        MAX_BODY_SIZE: float
        MAX_HEADER_COUNT: int
        MAX_HEADER_SIZE: int

    CallbackName: TypeAlias = Literal[
        "start",
        "data",
        "end",
        "field_start",
        "field_name",
        "field_data",
        "field_end",
        "part_begin",
        "part_data",
        "part_end",
        "header_begin",
        "header_field",
        "header_value",
        "header_end",
        "headers_finished",
    ]

# Unique missing object.
_missing = object()


def _noop_event() -> None:
    pass


def _noop_data(_data: bytes, _start: int, _end: int) -> None:
    pass


@global_enum
class QuerystringState(IntEnum):
    """Querystring parser states.

    These are used to keep track of the state of the parser, and are used to determine
    what to do when new data is encountered.
    """

    BEFORE_FIELD = 0
    FIELD_NAME = 1
    FIELD_DATA = 2


@global_enum
class MultipartState(IntEnum):
    """Multipart parser states.

    These are used to keep track of the state of the parser, and are used to determine
    what to do when new data is encountered.
    """

    START = 0
    START_BOUNDARY = 1
    HEADER_FIELD_START = 2
    HEADER_FIELD = 3
    HEADER_VALUE_START = 4
    HEADER_VALUE = 5
    HEADER_VALUE_ALMOST_DONE = 6
    HEADERS_ALMOST_DONE = 7
    PART_DATA_START = 8
    PART_DATA = 9
    PART_DATA_END = 10
    END_BOUNDARY = 11
    END = 12


if TYPE_CHECKING:
    BEFORE_FIELD = QuerystringState.BEFORE_FIELD
    FIELD_NAME = QuerystringState.FIELD_NAME
    FIELD_DATA = QuerystringState.FIELD_DATA

    START = MultipartState.START
    START_BOUNDARY = MultipartState.START_BOUNDARY
    HEADER_FIELD_START = MultipartState.HEADER_FIELD_START
    HEADER_FIELD = MultipartState.HEADER_FIELD
    HEADER_VALUE_START = MultipartState.HEADER_VALUE_START
    HEADER_VALUE = MultipartState.HEADER_VALUE
    HEADER_VALUE_ALMOST_DONE = MultipartState.HEADER_VALUE_ALMOST_DONE
    HEADERS_ALMOST_DONE = MultipartState.HEADERS_ALMOST_DONE
    PART_DATA_START = MultipartState.PART_DATA_START
    PART_DATA = MultipartState.PART_DATA
    PART_DATA_END = MultipartState.PART_DATA_END
    END_BOUNDARY = MultipartState.END_BOUNDARY
    END = MultipartState.END

# Flags for the multipart parser.
FLAG_PART_BOUNDARY = 1
FLAG_LAST_BOUNDARY = 2

# Get constants.  Since iterating over a str on Python 2 gives you a 1-length
# string, but iterating over a bytes object on Python 3 gives you an integer,
# we need to save these constants.
CR = b"\r"[0]
LF = b"\n"[0]
COLON = b":"[0]
SPACE = b" "[0]
HYPHEN = b"-"[0]
AMPERSAND = b"&"[0]
LOWER_A = b"a"[0]
LOWER_Z = b"z"[0]
NULL = b"\x00"[0]

# fmt: off
# Mask for ASCII characters that can be http tokens.
# Per RFC7230 - 3.2.6, this is all alpha-numeric characters
# and these: !#$%&'*+-.^_`|~
TOKEN_CHARS = (
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    b"abcdefghijklmnopqrstuvwxyz"
    b"0123456789"
    b"!#$%&'*+-.^_`|~")
TOKEN_CHARS_SET = frozenset(TOKEN_CHARS)
# fmt: on

DEFAULT_MAX_HEADER_COUNT = 8
"""Default maximum number of headers allowed per multipart part."""

DEFAULT_MAX_HEADER_SIZE = 4096 + 128
"""Default maximum size of a single multipart header line, including syntax overhead."""

MAX_BOUNDARY_LENGTH = 256
"""Maximum allowed length of a multipart boundary.

[RFC 2046 §5.1.1](https://datatracker.ietf.org/doc/html/rfc2046#section-5.1.1)
recommends boundaries be at most 70 bytes. 256 bytes is generous headroom over
every HTTP client.
"""


def _parseparam(s: str) -> list[str]:
    # Vendored from the standard library's `email.message._parseparam`
    # (see the original source for the full reference); the splitting
    # runs in the compiled `multipart-rust-core`.
    return _rs_parseparam(s)

def parse_options_header(value: str | bytes | None) -> tuple[bytes, dict[bytes, bytes]]:
    """Parses a Content-Type header into a value in the following format: (content_type, {parameters})."""
    return _rs_parse_options_header(value)

class Field:
    """A Field object represents a (parsed) form field.  It represents a single
    field with a corresponding name and value.

    The name that a :class:`Field` will be instantiated with is the same name
    that would be found in the following HTML::

        <input name="name_goes_here" type="text"/>

    This class defines two methods, :meth:`on_data` and :meth:`on_end`, that
    will be called when data is written to the Field, and when the Field is
    finalized, respectively.

    Args:
        name: The name of the form field.
        content_type: The value of the Content-Type header for this field.
    """

    def __init__(self, name: bytes | None, *, content_type: str | None = None) -> None:
        self._name = name
        self._value: list[bytes] = []
        self._content_type = content_type

        # We cache the joined version of _value for speed.
        self._cache = _missing

    @classmethod
    def from_value(cls, name: bytes, value: bytes | None) -> Field:
        """Create an instance of a :class:`Field`, and set the corresponding
        value - either None or an actual value.  This method will also
        finalize the Field itself.

        Args:
            name: the name of the form field.
            value: the value of the form field - either a bytestring or None.

        Returns:
            A new instance of a [`Field`][python_multipart.Field].
        """

        f = cls(name)
        if value is None:
            f.set_none()
        else:
            f.write(value)
        f.finalize()
        return f

    def write(self, data: bytes) -> int:
        """Write some data into the form field.

        Args:
            data: The data to write to the field.

        Returns:
            The number of bytes written.
        """
        return self.on_data(data)

    def on_data(self, data: bytes) -> int:
        """This method is a callback that will be called whenever data is
        written to the Field.

        Args:
            data: The data to write to the field.

        Returns:
            The number of bytes written.
        """
        self._value.append(data)
        self._cache = _missing
        return len(data)

    def on_end(self) -> None:
        """This method is called whenever the Field is finalized."""
        if self._cache is _missing:
            self._cache = b"".join(self._value)

    def finalize(self) -> None:
        """Finalize the form field."""
        self.on_end()

    def close(self) -> None:
        """Close the Field object.  This will free any underlying cache."""
        # Free our value array.
        if self._cache is _missing:
            self._cache = b"".join(self._value)

        del self._value

    def set_none(self) -> None:
        """Some fields in a querystring can possibly have a value of None - for
        example, the string "foo&bar=&baz=asdf" will have a field with the
        name "foo" and value None, one with name "bar" and value "", and one
        with name "baz" and value "asdf".  Since the write() interface doesn't
        support writing None, this function will set the field value to None.
        """
        self._cache = None

    @property
    def field_name(self) -> bytes | None:
        """This property returns the name of the field."""
        return self._name

    @property
    def value(self) -> bytes | None:
        """This property returns the value of the form field."""
        if self._cache is _missing:
            self._cache = b"".join(self._value)

        assert isinstance(self._cache, bytes) or self._cache is None
        return self._cache

    @property
    def content_type(self) -> str | None:
        """This property returns the content_type value of the field."""
        return self._content_type

    def __eq__(self, other: object) -> bool:
        if isinstance(other, Field):
            return self.field_name == other.field_name and self.value == other.value
        else:
            return NotImplemented

    def __repr__(self) -> str:
        if self.value is not None and len(self.value) > 97:
            # We get the repr, and then insert three dots before the final
            # quote.
            v = repr(self.value[:97])[:-1] + "...'"
        else:
            v = repr(self.value)

        return f"{self.__class__.__name__}(field_name={self.field_name!r}, value={v})"


class File:
    """This class represents an uploaded file.  It handles writing file data to
    either an in-memory file or a temporary file on-disk, if the optional
    threshold is passed.

    There are some options that can be passed to the File to change behavior
    of the class.  Valid options are as follows:

    | Name                  | Type  | Default | Description |
    |-----------------------|-------|---------|-------------|
    | UPLOAD_DIR            | `str` | None    | The directory to store uploaded files in. If this is None, a temporary file will be created in the system's standard location. |
    | UPLOAD_DELETE_TMP     | `bool`| True    | Delete automatically created TMP file |
    | UPLOAD_KEEP_FILENAME  | `bool`| False   | Whether or not to keep the filename of the uploaded file. If True, then the filename is reduced to its basename (directory components are stripped) and the file is saved with that name. Otherwise, a temporary name will be used. |
    | UPLOAD_KEEP_EXTENSIONS| `bool`| False   | Whether or not to keep the uploaded file's extension. If False, the file will be saved with the default temporary extension (usually ".tmp"). Otherwise, the file's extension will be maintained. Note that this will properly combine with the UPLOAD_KEEP_FILENAME setting. |
    | MAX_MEMORY_FILE_SIZE  | `int` | 1 MiB   | The maximum number of bytes of a File to keep in memory. By default, the contents of a File are kept into memory until a certain limit is reached, after which the contents of the File are written to a temporary file. This behavior can be disabled by setting this value to an appropriately large value (or, for example, infinity, such as `float('inf')`. |

    Args:
        file_name: The name of the file that this [`File`][python_multipart.File] represents.
        field_name: The name of the form field that this file was uploaded with.  This can be None, if, for example,
            the file was uploaded with Content-Type application/octet-stream.
        config: The configuration for this File.  See above for valid configuration keys and their corresponding values.
        content_type: The value of the Content-Type header.
    """  # noqa: E501

    def __init__(
        self,
        file_name: bytes | None,
        field_name: bytes | None = None,
        config: FileConfig = {},
        *,
        content_type: str | None = None,
    ) -> None:
        # Save configuration, set other variables default.
        self.logger = logging.getLogger(__name__)
        self._config = config
        self._in_memory = True
        self._bytes_written = 0
        self._fileobj: BytesIO | BufferedRandom = BytesIO()

        # Save the provided field/file name and content type.
        self._field_name = field_name
        self._file_name = file_name
        self._content_type = content_type

        # Our actual file name is None by default, since, depending on our
        # config, we may not actually use the provided name.
        self._actual_file_name: bytes | None = None

        # Split the extension from the filename.
        if file_name is not None:
            # Extract just the basename to avoid directory traversal
            basename = os.path.basename(file_name)
            base, ext = os.path.splitext(basename)
            self._file_base = base
            self._ext = ext

    @property
    def field_name(self) -> bytes | None:
        """The form field associated with this file.  May be None if there isn't
        one, for example when we have an application/octet-stream upload.
        """
        return self._field_name

    @property
    def file_name(self) -> bytes | None:
        """The file name given in the upload request."""
        return self._file_name

    @property
    def actual_file_name(self) -> bytes | None:
        """The file name that this file is saved as.  Will be None if it's not
        currently saved on disk.
        """
        return self._actual_file_name

    @property
    def file_object(self) -> BytesIO | BufferedRandom:
        """The file object that we're currently writing to.  Note that this
        will either be an instance of a :class:`io.BytesIO`, or a regular file
        object.
        """
        return self._fileobj

    @property
    def size(self) -> int:
        """The total size of this file, counted as the number of bytes that
        currently have been written to the file.
        """
        return self._bytes_written

    @property
    def in_memory(self) -> bool:
        """A boolean representing whether or not this file object is currently
        stored in-memory or on-disk.
        """
        return self._in_memory

    @property
    def content_type(self) -> str | None:
        """The Content-Type value for this part, if it was set."""
        return self._content_type

    def flush_to_disk(self) -> None:
        """If the file is already on-disk, do nothing.  Otherwise, copy from
        the in-memory buffer to a disk file, and then reassign our internal
        file object to this new disk file.

        Note that if you attempt to flush a file that is already on-disk, a
        warning will be logged to this module's logger.
        """
        if not self._in_memory:
            self.logger.warning("Trying to flush to disk when we're not in memory")
            return

        # Go back to the start of our file.
        self._fileobj.seek(0)

        # Open a new file.
        new_file = self._get_disk_file()

        # Copy the file objects.
        shutil.copyfileobj(self._fileobj, new_file)

        # Seek to the new position in our new file.
        new_file.seek(self._bytes_written)

        # Reassign the fileobject.
        old_fileobj = self._fileobj
        self._fileobj = new_file

        # We're no longer in memory.
        self._in_memory = False

        # Close the old file object.
        old_fileobj.close()

    def _get_disk_file(self) -> BufferedRandom:
        """This function is responsible for getting a file object on-disk for us."""
        self.logger.info("Opening a file on disk")

        file_dir = self._config.get("UPLOAD_DIR")
        keep_filename = self._config.get("UPLOAD_KEEP_FILENAME", False)
        keep_extensions = self._config.get("UPLOAD_KEEP_EXTENSIONS", False)
        delete_tmp = self._config.get("UPLOAD_DELETE_TMP", True)
        tmp_file: None | BufferedRandom = None

        # If we have a directory and are to keep the filename...
        if file_dir is not None and keep_filename:
            self.logger.info("Saving with filename in: %r", file_dir)

            # Build our filename.
            # TODO: what happens if we don't have a filename?
            fname = self._file_base + self._ext if keep_extensions else self._file_base

            path = os.path.join(file_dir, fname)  # type: ignore[arg-type]
            try:
                self.logger.info("Opening file: %r", path)
                tmp_file = open(path, "w+b")
            except OSError:
                tmp_file = None

                self.logger.exception("Error opening temporary file")
                raise FileError("Error opening temporary file: %r" % path)
        else:
            # Build options array.
            # Note that on Python 3, tempfile doesn't support byte names.  We
            # encode our paths using the default filesystem encoding.
            suffix = self._ext.decode(sys.getfilesystemencoding()) if keep_extensions else None

            if file_dir is None:
                dir = None
            elif isinstance(file_dir, bytes):
                dir = file_dir.decode(sys.getfilesystemencoding())
            else:
                dir = file_dir  # pragma: no cover

            # Create a temporary (named) file with the appropriate settings.
            self.logger.info(
                "Creating a temporary file with options: %r", {"suffix": suffix, "delete": delete_tmp, "dir": dir}
            )
            try:
                tmp_file = cast(BufferedRandom, tempfile.NamedTemporaryFile(suffix=suffix, delete=delete_tmp, dir=dir))
            except OSError:
                self.logger.exception("Error creating named temporary file")
                raise FileError("Error creating named temporary file")

            assert tmp_file is not None
            # Encode filename as bytes.
            if isinstance(tmp_file.name, str):
                fname = tmp_file.name.encode(sys.getfilesystemencoding())
            else:
                fname = cast(bytes, tmp_file.name)  # pragma: no cover

        self._actual_file_name = fname
        return tmp_file

    def write(self, data: bytes) -> int:
        """Write some data to the File.

        :param data: a bytestring
        """
        return self.on_data(data)

    def on_data(self, data: bytes) -> int:
        """This method is a callback that will be called whenever data is
        written to the File.

        Args:
            data: The data to write to the file.

        Returns:
            The number of bytes written.
        """
        bwritten = self._fileobj.write(data)

        # If the bytes written isn't the same as the length, just return.
        if bwritten != len(data):
            self.logger.warning("bwritten != len(data) (%d != %d)", bwritten, len(data))
            return bwritten

        # Keep track of how many bytes we've written.
        self._bytes_written += bwritten

        # If we're in-memory and are over our limit, we create a file.
        max_memory_file_size = self._config.get("MAX_MEMORY_FILE_SIZE")
        if self._in_memory and max_memory_file_size is not None and (self._bytes_written > max_memory_file_size):
            self.logger.info("Flushing to disk")
            self.flush_to_disk()

        # Return the number of bytes written.
        return bwritten

    def on_end(self) -> None:
        """This method is called whenever the Field is finalized."""
        # Flush the underlying file object
        self._fileobj.flush()

    def finalize(self) -> None:
        """Finalize the form file.  This will not close the underlying file,
        but simply signal that we are finished writing to the File.
        """
        self.on_end()

    def close(self) -> None:
        """Close the File object.  This will actually close the underlying
        file object (whether it's a :class:`io.BytesIO` or an actual file
        object).
        """
        self._fileobj.close()

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}(file_name={self.file_name!r}, field_name={self.field_name!r})"


class BaseParser:
    """This class is the base class for all parsers.  It contains the logic for
    calling and adding callbacks.

    A callback can be one of two different forms.  "Notification callbacks" are
    callbacks that are called when something happens - for example, when a new
    part of a multipart message is encountered by the parser.  "Data callbacks"
    are called when we get some sort of data - for example, part of the body of
    a multipart chunk.  Notification callbacks are called with no parameters,
    whereas data callbacks are called with three, as follows::

        data_callback(data, start, end)

    The "data" parameter is a bytestring (i.e. "foo" on Python 2, or b"foo" on
    Python 3).  "start" and "end" are integer indexes into the "data" string
    that represent the data of interest.  Thus, in a data callback, the slice
    `data[start:end]` represents the data that the callback is "interested in".
    The callback is not passed a copy of the data, since copying severely hurts
    performance.
    """

    def __init__(self) -> None:
        self.logger = logging.getLogger(__name__)
        self.callbacks: QuerystringCallbacks | OctetStreamCallbacks | MultipartCallbacks = {}

    def callback(
        self, name: CallbackName, data: bytes | None = None, start: int | None = None, end: int | None = None
    ) -> None:
        """This function calls a provided callback with some data.  If the
        callback is not set, will do nothing.

        Args:
            name: The name of the callback to call (as a string).
            data: Data to pass to the callback.  If None, then it is assumed that the callback is a notification
                callback, and no parameters are given.
            end: An integer that is passed to the data callback.
            start: An integer that is passed to the data callback.
        """
        func = self.callbacks.get("on_" + name)
        if func is None:
            return
        func = cast("Callable[..., Any]", func)
        # Depending on whether we're given a buffer...
        if data is not None:
            # Don't do anything if we have start == end.
            if start is not None and start == end:
                return
            func(data, start, end)
        else:
            func()

    def set_callback(self, name: CallbackName, new_func: Callable[..., Any] | None) -> None:
        """Update the function for a callback.  Removes from the callbacks dict
        if new_func is None.

        :param name: The name of the callback to call (as a string).

        :param new_func: The new function for the callback.  If None, then the
                         callback will be removed (with no error if it does not
                         exist).
        """
        if new_func is None:
            self.callbacks.pop("on_" + name, None)  # type: ignore[misc]
        else:
            self.callbacks["on_" + name] = new_func  # type: ignore[literal-required]

    def close(self) -> None:
        pass  # pragma: no cover

    def finalize(self) -> None:
        pass  # pragma: no cover

    def __repr__(self) -> str:
        return "%s()" % self.__class__.__name__


class OctetStreamParser(BaseParser):
    """This parser parses an octet-stream request body and calls callbacks when
    incoming data is received.  Callbacks are as follows:

    | Callback Name  | Parameters      | Description                                         |
    |----------------|-----------------|-----------------------------------------------------|
    | on_start       | None            | Called when the first data is parsed.               |
    | on_data        | data, start, end| Called for each data chunk that is parsed.           |
    | on_end         | None            | Called when the parser is finished parsing all data.|

    Args:
        callbacks: A dictionary of callbacks.  See the documentation for [`BaseParser`][python_multipart.BaseParser].
        max_size: The maximum size of body to parse.  Defaults to infinity - i.e. unbounded.
    """

    def __init__(self, callbacks: OctetStreamCallbacks = {}, max_size: float = float("inf")):
        super().__init__()
        self.callbacks = callbacks
        self._started = False

        if not isinstance(max_size, Number) or max_size < 1:
            raise ValueError("max_size must be a positive number, not %r" % max_size)
        self.max_size: int | float = max_size
        self._current_size = 0

    def write(self, data: bytes) -> int:
        """Write some data to the parser, which will perform size verification,
        and then pass the data to the underlying callback.

        Args:
            data: The data to write to the parser.

        Returns:
            The number of bytes written.
        """
        if not self._started:
            self.callback("start")
            self._started = True

        # Truncate data length.
        data_len = len(data)
        if (self._current_size + data_len) > self.max_size:
            # We truncate the length of data that we are to process.
            new_size = int(self.max_size - self._current_size)
            self.logger.warning(
                "Current size is %d (max %d), so truncating data length from %d to %d",
                self._current_size,
                self.max_size,
                data_len,
                new_size,
            )
            data_len = new_size

        # Increment size, then callback, in case there's an exception.
        self._current_size += data_len
        self.callback("data", data, 0, data_len)
        return data_len

    def finalize(self) -> None:
        """Finalize this parser, which signals to that we are finished parsing,
        and sends the on_end callback.
        """
        self.callback("end")

    def __repr__(self) -> str:
        return "%s()" % self.__class__.__name__


class QuerystringParser(BaseParser):
    """This is a streaming querystring parser.  It will consume data, and call
    the callbacks given when it has data.

    | Callback Name  | Parameters      | Description                                         |
    |----------------|-----------------|-----------------------------------------------------|
    | on_field_start | None            | Called when a new field is encountered.             |
    | on_field_name  | data, start, end| Called when a portion of a field's name is encountered. |
    | on_field_data  | data, start, end| Called when a portion of a field's data is encountered. |
    | on_field_end   | None            | Called when the end of a field is encountered.      |
    | on_end         | None            | Called when the parser is finished parsing all data.|

    Args:
        callbacks: A dictionary of callbacks.  See the documentation for [`BaseParser`][python_multipart.BaseParser].
        strict_parsing: Whether or not to parse the body strictly.  Defaults to False.  If this is set to True, then the
            behavior of the parser changes as the following: if a field has a value with an equal sign
            (e.g. "foo=bar", or "foo="), it is always included.  If a field has no equals sign (e.g. "...&name&..."),
            it will be treated as an error if 'strict_parsing' is True, otherwise included.  If an error is encountered,
            then a [`QuerystringParseError`][python_multipart.exceptions.QuerystringParseError] will be raised.
        max_size: The maximum size of body to parse.  Defaults to infinity - i.e. unbounded.
    """  # noqa: E501


    def __init__(
        self, callbacks: QuerystringCallbacks = {}, strict_parsing: bool = False, max_size: float = float("inf")
    ) -> None:
        super().__init__()
        self._core = _QsCore()
        self.state = BEFORE_FIELD

        self.callbacks = callbacks

        # Max-size stuff
        if not isinstance(max_size, Number) or max_size < 1:
            raise ValueError("max_size must be a positive number, not %r" % max_size)
        self.max_size: int | float = max_size
        self._current_size = 0

        # Should parsing be strict?
        self.strict_parsing = strict_parsing

    def write(self, data: bytes) -> int:
        """Write some data to the parser, which will perform size verification,
        parse into either a field name or value, and then pass the
        corresponding data to the underlying callback.  If an error is
        encountered while parsing, a QuerystringParseError will be raised.  The
        "offset" attribute of the raised exception will be set to the offset in
        the input data chunk (NOT the overall stream) that caused the error.

        Args:
            data: The data to write to the parser.

        Returns:
            The number of bytes written.
        """
        # Handle sizing.
        data_len = len(data)
        if (self._current_size + data_len) > self.max_size:
            # We truncate the length of data that we are to process.
            new_size = int(self.max_size - self._current_size)
            self.logger.warning(
                "Current size is %d (max %d), so truncating data length from %d to %d",
                self._current_size,
                self.max_size,
                data_len,
                new_size,
            )
            data_len = new_size

        l = 0
        try:
            l = self._internal_write(data, data_len)
        finally:
            self._current_size += l

        return l

    @property
    def state(self) -> QuerystringState:
        return self._core.state

    @state.setter
    def state(self, value: QuerystringState | int) -> None:
        self._core.state = value

    def _internal_write(self, data: bytes, length: int) -> int:
        return self._core.internal_write(
            data, length, bool(self.strict_parsing), self, self.logger
        )

    def finalize(self) -> None:
        """Finalize this parser, which signals to that we are finished parsing,
        if we're still in the middle of a field, an on_field_end callback, and
        then the on_end callback.
        """
        callbacks = cast("QuerystringCallbacks", self.callbacks)
        # If we're currently in the middle of a field, we finish it.
        if self.state in (FIELD_DATA, FIELD_NAME):
            on_field_end = callbacks.get("on_field_end")
            if on_field_end is None:
                on_field_end = _noop_event
            on_field_end()
        on_end = callbacks.get("on_end")
        if on_end is None:
            on_end = _noop_event
        on_end()

    def __repr__(self) -> str:
        return "{}(strict_parsing={!r}, max_size={!r})".format(
            self.__class__.__name__, self.strict_parsing, self.max_size
        )


class MultipartParser(BaseParser):
    """This class is a streaming multipart/form-data parser.

    | Callback Name      | Parameters      | Description |
    |--------------------|-----------------|-------------|
    | on_part_begin      | None            | Called when a new part of the multipart message is encountered. |
    | on_part_data       | data, start, end| Called when a portion of a part's data is encountered. |
    | on_part_end        | None            | Called when the end of a part is reached. |
    | on_header_begin    | None            | Called when we've found a new header in a part of a multipart message |
    | on_header_field    | data, start, end| Called each time an additional portion of a header is read (i.e. the part of the header that is before the colon; the "Foo" in "Foo: Bar"). |
    | on_header_value    | data, start, end| Called when we get data for a header. |
    | on_header_end      | None            | Called when the current header is finished - i.e. we've reached the newline at the end of the header. |
    | on_headers_finished| None            | Called when all headers are finished, and before the part data starts. |
    | on_end             | None            | Called when the parser is finished parsing all data. |

    Args:
        boundary: The multipart boundary.  This is required, and must match what is given in the HTTP request - usually in the Content-Type header.
        callbacks: A dictionary of callbacks.  See the documentation for [`BaseParser`][python_multipart.BaseParser].
        max_size: The maximum size of body to parse.  Defaults to infinity - i.e. unbounded.
        max_header_count: The maximum number of headers allowed per part.
        max_header_size: The maximum size of a single header line (excluding the trailing CRLF).
    """  # noqa: E501

    def __init__(
        self,
        boundary: bytes | str,
        callbacks: MultipartCallbacks = {},
        max_size: float = float("inf"),
        *,
        max_header_count: int = DEFAULT_MAX_HEADER_COUNT,
        max_header_size: int = DEFAULT_MAX_HEADER_SIZE,
    ) -> None:
        # Initialize parser state.
        super().__init__()
        self._core = _MpCore()

        self.callbacks = callbacks

        if not isinstance(max_size, Number) or max_size < 1:
            raise ValueError("max_size must be a positive number, not %r" % max_size)
        self.max_size = max_size
        self._current_size = 0

        self.max_header_count = max_header_count
        self._current_header_count = 0

        self.max_header_size = max_header_size
        self._current_header_size = 0


        # Save our boundary.
        if isinstance(boundary, str):  # pragma: no cover
            boundary = boundary.encode("latin-1")
        if len(boundary) > MAX_BOUNDARY_LENGTH:
            raise FormParserError(f"Boundary length {len(boundary)} exceeds maximum of {MAX_BOUNDARY_LENGTH}")
        self.boundary = b"\r\n--" + boundary

    def write(self, data: bytes) -> int:
        """Write some data to the parser, which will perform size verification,
        and then parse the data into the appropriate location (e.g. header,
        data, etc.), and pass this on to the underlying callback.  If an error
        is encountered, a MultipartParseError will be raised.  The "offset"
        attribute on the raised exception will be set to the offset of the byte
        in the input chunk that caused the error.

        Args:
            data: The data to write to the parser.

        Returns:
            The number of bytes written.
        """
        # Handle sizing.
        data_len = len(data)
        if (self._current_size + data_len) > self.max_size:
            # We truncate the length of data that we are to process.
            new_size = int(self.max_size - self._current_size)
            self.logger.warning(
                "Current size is %d (max %d), so truncating data length from %d to %d",
                self._current_size,
                self.max_size,
                data_len,
                new_size,
            )
            data_len = new_size

        l = 0
        try:
            l = self._internal_write(data, data_len)
        finally:
            self._current_size += l

        return l

    @property
    def state(self) -> MultipartState:
        return self._core.state

    @state.setter
    def state(self, value: MultipartState | int) -> None:
        self._core.state = value

    def _internal_write(self, data: bytes, length: int) -> int:
        return self._core.internal_write(
            data,
            length,
            self.boundary,
            self.max_header_count,
            self.max_header_size,
            self,
            self.logger,
        )

    def finalize(self) -> None:
        """Finalize this parser, which signals to that we are finished parsing.

        Note: It does not currently, but in the future, it will verify that we
        are in the final state of the parser (i.e. the end of the multipart
        message is well-formed), and, if not, throw an error.
        """
        # TODO: verify that we're in the state MultipartState.END, otherwise throw an
        # error or otherwise state that we're not finished parsing.
        pass

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}(boundary={self.boundary!r})"


class FormParser:
    """This class is the all-in-one form parser.  Given all the information
    necessary to parse a form, it will instantiate the correct parser, create
    the proper :class:`Field` and :class:`File` classes to store the data that
    is parsed, and call the two given callbacks with each field and file as
    they become available.

    Args:
        content_type: The Content-Type of the incoming request.  This is used to select the appropriate parser.
        on_field: The callback to call when a field has been parsed and is ready for usage.  See above for parameters.
        on_file: The callback to call when a file has been parsed and is ready for usage.  See above for parameters.
        on_end: An optional callback to call when all fields and files in a request has been parsed.  Can be None.
        boundary: If the request is a multipart/form-data request, this should be the boundary of the request, as given
            in the Content-Type header, as a bytestring.
        file_name: If the request is of type application/octet-stream, then the body of the request will not contain any
            information about the uploaded file.  In such cases, you can provide the file name of the uploaded file
            manually.
        config: Configuration to use for this FormParser.  The default values are taken from the DEFAULT_CONFIG value,
            and then any keys present in this dictionary will overwrite the default values.
    """

    #: This is the default configuration for our form parser.
    #: Note: all file sizes should be in bytes.
    DEFAULT_CONFIG: FormParserConfig = {
        "MAX_BODY_SIZE": float("inf"),
        "MAX_HEADER_COUNT": DEFAULT_MAX_HEADER_COUNT,
        "MAX_HEADER_SIZE": DEFAULT_MAX_HEADER_SIZE,
        "MAX_MEMORY_FILE_SIZE": 1 * 1024 * 1024,
        "UPLOAD_DIR": None,
        "UPLOAD_DELETE_TMP": True,
        "UPLOAD_KEEP_FILENAME": False,
        "UPLOAD_KEEP_EXTENSIONS": False,
        # Error on invalid Content-Transfer-Encoding?
        "UPLOAD_ERROR_ON_BAD_CTE": False,
    }

    def __init__(
        self,
        content_type: str,
        on_field: Callable[[Field], None] | None,
        on_file: Callable[[File], None] | None,
        on_end: Callable[[], None] | None = None,
        boundary: bytes | str | None = None,
        file_name: bytes | None = None,
        config: dict[Any, Any] = {},
    ) -> None:
        self.logger = logging.getLogger(__name__)

        # Save variables.
        self.content_type = content_type
        self.boundary = boundary
        self.bytes_received = 0
        self.parser = None

        # Save callbacks.
        self.on_field = on_field
        self.on_file = on_file
        self.on_end = on_end

        # Set configuration options.
        self.config: FormParserConfig = self.DEFAULT_CONFIG.copy()
        self.config.update(config)  # type: ignore[typeddict-item]

        parser: OctetStreamParser | MultipartParser | QuerystringParser | None = None

        # Depending on the Content-Type, we instantiate the correct parser.
        if content_type == "application/octet-stream":
            file: File | None = None

            def on_start() -> None:
                nonlocal file
                file = File(file_name, None, config=self.config)

            def on_data(data: bytes, start: int, end: int) -> None:
                nonlocal file
                assert file is not None
                file.write(data[start:end])

            def _on_end() -> None:
                nonlocal file
                assert file is not None
                # Finalize the file itself.
                file.finalize()

                # Call our callback.
                if on_file:
                    on_file(file)

                # Call the on-end callback.
                if self.on_end is not None:
                    self.on_end()

            # Instantiate an octet-stream parser
            parser = OctetStreamParser(
                callbacks={"on_start": on_start, "on_data": on_data, "on_end": _on_end},
                max_size=self.config["MAX_BODY_SIZE"],
            )

        elif content_type == "application/x-www-form-urlencoded" or content_type == "application/x-url-encoded":
            name_buffer: list[bytes] = []

            f: Field | None = None

            def on_field_start() -> None:
                pass

            def on_field_name(data: bytes, start: int, end: int) -> None:
                name_buffer.append(data[start:end])

            def on_field_data(data: bytes, start: int, end: int) -> None:
                nonlocal f
                if f is None:
                    f = Field(b"".join(name_buffer))
                    del name_buffer[:]
                f.write(data[start:end])

            def on_field_end() -> None:
                nonlocal f
                # Finalize and call callback.
                if f is None:
                    # If we get here, it's because there was no field data.
                    # We create a field, set it to None, and then continue.
                    f = Field(b"".join(name_buffer))
                    del name_buffer[:]
                    f.set_none()

                f.finalize()
                if on_field:
                    on_field(f)
                f = None

            def _on_end() -> None:
                if self.on_end is not None:
                    self.on_end()

            # Instantiate parser.
            parser = QuerystringParser(
                callbacks={
                    "on_field_start": on_field_start,
                    "on_field_name": on_field_name,
                    "on_field_data": on_field_data,
                    "on_field_end": on_field_end,
                    "on_end": _on_end,
                },
                max_size=self.config["MAX_BODY_SIZE"],
            )

        elif content_type == "multipart/form-data":
            if boundary is None:
                self.logger.error("No boundary given")
                raise FormParserError("No boundary given")

            header_name: list[bytes] = []
            header_value: list[bytes] = []
            headers: dict[bytes, bytes] = {}

            f_multi: File | Field | None = None
            writer: File | Field | Base64Decoder | QuotedPrintableDecoder | None = None
            is_file = False

            def on_part_begin() -> None:
                # Reset headers in case this isn't the first part.
                nonlocal headers
                headers = {}

            def on_part_data(data: bytes, start: int, end: int) -> None:
                nonlocal writer
                assert writer is not None
                writer.write(data[start:end])
                # TODO: check for error here.

            def on_part_end() -> None:
                nonlocal f_multi, is_file
                assert f_multi is not None
                f_multi.finalize()
                if is_file:
                    if on_file:
                        assert isinstance(f_multi, File)
                        on_file(f_multi)
                else:
                    if on_field:
                        assert isinstance(f_multi, Field)
                        on_field(f_multi)

            def on_header_field(data: bytes, start: int, end: int) -> None:
                header_name.append(data[start:end])

            def on_header_value(data: bytes, start: int, end: int) -> None:
                header_value.append(data[start:end])

            def on_header_end() -> None:
                headers[b"".join(header_name).lower()] = b"".join(header_value)
                del header_name[:]
                del header_value[:]

            def on_headers_finished() -> None:
                nonlocal is_file, f_multi, writer
                # Reset the 'is file' flag.
                is_file = False

                # Parse the content-disposition header.
                content_disp = headers.get(b"content-disposition")
                disp, options = parse_options_header(content_disp)

                # Get the field and filename.
                field_name = options.get(b"name")
                file_name = options.get(b"filename")
                # RFC 7578 §4.2: each part MUST have a Content-Disposition header with a "name" parameter.
                if field_name is None:
                    raise FormParserError(f'Field name not found in Content-Disposition: "{content_disp!r}"')

                # Create the proper class.
                content_type_b = headers.get(b"content-type")
                content_type = content_type_b.decode("latin-1") if content_type_b is not None else None
                if file_name is None:
                    f_multi = Field(field_name, content_type=content_type)
                else:
                    f_multi = File(file_name, field_name, config=self.config, content_type=content_type)
                    is_file = True

                # Parse the given Content-Transfer-Encoding to determine what
                # we need to do with the incoming data.
                # TODO: check that we properly handle 8bit / 7bit encoding.
                # RFC 2045 section 6.1: Content-Transfer-Encoding values are case-insensitive.
                # https://www.rfc-editor.org/rfc/rfc2045#section-6.1
                transfer_encoding = headers.get(b"content-transfer-encoding", b"7bit").lower()

                if transfer_encoding in (b"binary", b"8bit", b"7bit"):
                    writer = f_multi

                elif transfer_encoding == b"base64":
                    writer = Base64Decoder(f_multi)

                elif transfer_encoding == b"quoted-printable":
                    writer = QuotedPrintableDecoder(f_multi)

                else:
                    self.logger.warning("Unknown Content-Transfer-Encoding: %r", transfer_encoding)
                    if self.config["UPLOAD_ERROR_ON_BAD_CTE"]:
                        raise FormParserError(f'Unknown Content-Transfer-Encoding "{transfer_encoding!r}"')
                    else:
                        # If we aren't erroring, then we just treat this as an
                        # unencoded Content-Transfer-Encoding.
                        writer = f_multi

            def _on_end() -> None:
                nonlocal writer
                if writer is not None:
                    writer.finalize()
                if self.on_end is not None:
                    self.on_end()

            # Instantiate a multipart parser.
            parser = MultipartParser(
                boundary,
                callbacks={
                    "on_part_begin": on_part_begin,
                    "on_part_data": on_part_data,
                    "on_part_end": on_part_end,
                    "on_header_field": on_header_field,
                    "on_header_value": on_header_value,
                    "on_header_end": on_header_end,
                    "on_headers_finished": on_headers_finished,
                    "on_end": _on_end,
                },
                max_size=self.config["MAX_BODY_SIZE"],
                max_header_count=self.config["MAX_HEADER_COUNT"],
                max_header_size=self.config["MAX_HEADER_SIZE"],
            )

        else:
            self.logger.warning("Unknown Content-Type: %r", content_type)
            raise FormParserError(f"Unknown Content-Type: {content_type}")

        self.parser = parser

    def write(self, data: bytes) -> int:
        """Write some data.  The parser will forward this to the appropriate
        underlying parser.

        Args:
            data: The data to write.

        Returns:
            The number of bytes processed.
        """
        self.bytes_received += len(data)
        # TODO: check the parser's return value for errors?
        assert self.parser is not None
        return self.parser.write(data)

    def finalize(self) -> None:
        """Finalize the parser."""
        if self.parser is not None and hasattr(self.parser, "finalize"):
            self.parser.finalize()

    def close(self) -> None:
        """Close the parser."""
        if self.parser is not None and hasattr(self.parser, "close"):
            self.parser.close()

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}(content_type={self.content_type!r}, parser={self.parser!r})"


def create_form_parser(
    headers: dict[str, bytes],
    on_field: Callable[[Field], None] | None,
    on_file: Callable[[File], None] | None,
    config: dict[Any, Any] = {},
) -> FormParser:
    """This function is a helper function to aid in creating a FormParser
    instances.  Given a dictionary-like headers object, it will determine
    the correct information needed, instantiate a FormParser with the
    appropriate values and given callbacks, and then return the corresponding
    parser.

    Args:
        headers: A dictionary-like object of HTTP headers.  The only required header is Content-Type.
        on_field: Callback to call with each parsed field.
        on_file: Callback to call with each parsed file.
        config: Configuration variables to pass to the FormParser.
    """
    content_type: str | bytes | None = headers.get("Content-Type")
    if content_type is None:
        logging.getLogger(__name__).warning("No Content-Type header given")
        raise ValueError("No Content-Type header given!")

    # Boundaries are optional (the FormParser will raise if one is needed
    # but not given).
    content_type, params = parse_options_header(content_type)
    boundary = params.get(b"boundary")

    # We need content_type to be a string, not a bytes object.
    content_type = content_type.decode("latin-1")

    # Instantiate a form parser.
    form_parser = FormParser(content_type, on_field, on_file, boundary=boundary, config=config)

    # Return our parser.
    return form_parser


def parse_form(
    headers: dict[str, bytes],
    input_stream: SupportsRead,
    on_field: Callable[[Field], None] | None,
    on_file: Callable[[File], None] | None,
    chunk_size: int = 1048576,
) -> None:
    """This function is useful if you just want to parse a request body,
    without too much work.  Pass it a dictionary-like object of the request's
    headers, and a file-like object for the input stream, along with two
    callbacks that will get called whenever a field or file is parsed.

    Args:
        headers: A dictionary-like object of HTTP headers.  The only required header is Content-Type.
        input_stream: A file-like object that represents the request body. The read() method must return bytestrings.
        on_field: Callback to call with each parsed field.
        on_file: Callback to call with each parsed file.
        chunk_size: The maximum size to read from the input stream and write to the parser at one time.
            Defaults to 1 MiB.
    """
    if chunk_size < 1:
        raise ValueError(f"chunk_size must be a positive number, not {chunk_size!r}")

    # Create our form parser.
    parser = create_form_parser(headers, on_field, on_file)

    # Read chunks of 1MiB and write to the parser, but never read more than
    # the given Content-Length, if any.
    content_length: int | float | bytes | None = headers.get("Content-Length")
    if content_length is not None:
        content_length = int(content_length)
        if content_length < 0:
            raise ValueError("Content-Length must be non-negative")
    else:
        content_length = float("inf")
    bytes_read = 0

    while True:
        # Read only up to the Content-Length given.
        max_readable = int(min(content_length - bytes_read, chunk_size))
        buff = input_stream.read(max_readable)

        # Write to the parser and update our length.
        parser.write(buff)
        bytes_read += len(buff)

        # If we get a buffer that's smaller than the size requested, or if we
        # have read up to our content length, we're done.
        if len(buff) != max_readable or bytes_read == content_length:
            break

    # Tell our parser that we're done writing data.
    parser.finalize()
