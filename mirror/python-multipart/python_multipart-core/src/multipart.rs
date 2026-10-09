//! Streaming multipart/form-data parser core.
//!
//! Direct transcription of `MultipartParser._internal_write`: the same
//! states, marks, lookbehind emission, header accounting, messages, and
//! offsets. Config (`boundary`, header limits) arrives per call from the
//! live Python attributes; only evolving state lives here.
//!
//! License: Apache-2.0 (preserved from the original).

use crate::{
    py_find, py_get, py_rfind_byte, py_startswith, Emitter, ParseFail, WriteErr, CR,
    HYPHEN, LF, SPACE,
};

/// `MultipartState` values (`START = 0` .. `END = 12`).
pub const START: i64 = 0;
pub const START_BOUNDARY: i64 = 1;
pub const HEADER_FIELD_START: i64 = 2;
pub const HEADER_FIELD: i64 = 3;
pub const HEADER_VALUE_START: i64 = 4;
pub const HEADER_VALUE: i64 = 5;
pub const HEADER_VALUE_ALMOST_DONE: i64 = 6;
pub const HEADERS_ALMOST_DONE: i64 = 7;
pub const PART_DATA_START: i64 = 8;
pub const PART_DATA: i64 = 9;
pub const PART_DATA_END: i64 = 10;
pub const END_BOUNDARY: i64 = 11;
pub const END: i64 = 12;

/// Boundary-match flags.
pub const FLAG_PART_BOUNDARY: u8 = 1;
pub const FLAG_LAST_BOUNDARY: u8 = 2;

/// `TOKEN_CHARS` (RFC 7230 §3.2.6): alphanumerics plus `!#$%&'*+-.^_`|~`.
const TOKEN: [bool; 256] = {
    let mut t = [false; 256];
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!#$%&'*+-.^_`|~";
    let mut k = 0;
    while k < chars.len() {
        t[chars[k] as usize] = true;
        k += 1;
    }
    t
};

const MARK_HEADER_FIELD: usize = 0;
const MARK_HEADER_VALUE: usize = 1;
const MARK_PART_DATA: usize = 2;

/// Evolving parse state (committed only on clean completion).
#[derive(Debug, Clone)]
pub struct MultipartCore {
    /// Current state (`START`..`END`, or garbage; `i64` so out-of-range
    /// assignments report exactly like the original).
    pub state: i64,
    /// Boundary-match index.
    pub index: usize,
    /// Boundary-match flags.
    pub flags: u8,
    /// Headers seen in the current part.
    pub cur_header_count: u64,
    /// Bytes charged to the current header line.
    pub cur_header_size: u64,
    /// Data marks (`header_field`, `header_value`, `part_data`);
    /// negative values are lookbehind lengths.
    pub marks: [Option<i64>; 3],
}

impl MultipartCore {
    /// Fresh parser (`state = START`, everything zeroed).
    pub fn new() -> Self {
        MultipartCore {
            state: START,
            index: 0,
            flags: 0,
            cur_header_count: 0,
            cur_header_size: 0,
            marks: [None, None, None],
        }
    }

    /// Parse `data[..length]` with framed `boundary` (`b"\r\n--" + raw`),
    /// emitting events. Returns `length` on success.
    #[allow(clippy::too_many_arguments)]
    pub fn internal_write<E>(
        &mut self,
        data: &[u8],
        length: usize,
        boundary: &[u8],
        max_header_count: u64,
        max_header_size: u64,
        emit: &mut impl Emitter<Err = E>,
    ) -> Result<usize, WriteErr<E>> {
        let mut scan = Scan {
            data,
            length,
            boundary,
            blen: boundary.len(),
            max_hc: max_header_count,
            max_hs: max_header_size,
            entry_flags: self.flags,
            state: self.state,
            index: self.index,
            flags: self.flags,
            chc: self.cur_header_count,
            chs: self.cur_header_size,
            marks: self.marks,
            emit,
        };
        let r = scan.run();
        if r.is_ok() {
            self.state = scan.state;
            self.index = scan.index;
            self.flags = scan.flags;
            self.cur_header_count = scan.chc;
            self.cur_header_size = scan.chs;
            self.marks = scan.marks;
        }
        r
    }
}

impl Default for MultipartCore {
    fn default() -> Self {
        Self::new()
    }
}

/// Working parse state for one chunk (locals in the original).
struct Scan<'d, 'b, 'e, E: crate::Emitter> {
    data: &'d [u8],
    length: usize,
    boundary: &'b [u8],
    blen: usize,
    max_hc: u64,
    max_hs: u64,
    entry_flags: u8,
    state: i64,
    index: usize,
    flags: u8,
    chc: u64,
    chs: u64,
    marks: [Option<i64>; 3],
    emit: &'e mut E,
}

impl<E: crate::Emitter> Scan<'_, '_, '_, E> {
    fn fail(&mut self, message: String, offset: i64) -> Result<usize, WriteErr<E::Err>> {
        self.emit.warn(&message).map_err(WriteErr::Callback)?;
        Err(ParseFail {
            multipart: true,
            message,
            offset,
        }
        .into())
    }

    fn advance(&mut self, amount: usize, i: i64) -> Result<(), WriteErr<E::Err>> {
        self.chs += amount as u64;
        if self.chs > self.max_hs {
            let message = "Maximum header size exceeded".to_string();
            self.emit.warn(&message).map_err(WriteErr::Callback)?;
            return Err(ParseFail {
                multipart: true,
                message,
                offset: i,
            }
            .into());
        }
        Ok(())
    }

    /// `data_callback(name, end_i, remaining)`: emit `marks[name]..end_i`,
    /// with the negative-mark lookbehind path.
    fn data_callback(
        &mut self,
        mark: usize,
        name: &str,
        end_i: i64,
        remaining: bool,
    ) -> Result<(), WriteErr<E::Err>> {
        let marked = match self.marks[mark] {
            None => return Ok(()),
            Some(m) => m,
        };
        if end_i <= marked {
            // No additional data to send.
        } else if marked >= 0 {
            self.emit
                .input(name, marked as usize, end_i as usize)
                .map_err(WriteErr::Callback)?;
        } else {
            // Some of the data comes from a partial boundary match and
            // requires look-behind (entry flags, not the loop-local ones).
            let lb = (-marked) as usize;
            if lb <= self.blen {
                self.emit.boundary(name, lb).map_err(WriteErr::Callback)?;
            } else if self.entry_flags & FLAG_PART_BOUNDARY != 0 {
                let mut lookback = Vec::with_capacity(self.blen + 2);
                lookback.extend_from_slice(self.boundary);
                lookback.push(CR);
                lookback.push(LF);
                debug_assert!(lb <= lookback.len());
                self.emit
                    .owned(name, &lookback[..lb])
                    .map_err(WriteErr::Callback)?;
            } else if self.entry_flags & FLAG_LAST_BOUNDARY != 0 {
                let mut lookback = Vec::with_capacity(self.blen + 4);
                lookback.extend_from_slice(self.boundary);
                lookback.push(HYPHEN);
                lookback.push(HYPHEN);
                lookback.push(CR);
                lookback.push(LF);
                debug_assert!(lb <= lookback.len());
                self.emit
                    .owned(name, &lookback[..lb])
                    .map_err(WriteErr::Callback)?;
            } else {
                self.emit
                    .warn("Look-back buffer error")
                    .map_err(WriteErr::Callback)?;
            }
            if end_i > 0 {
                self.emit
                    .input(name, 0, end_i as usize)
                    .map_err(WriteErr::Callback)?;
            }
        }
        if remaining {
            self.marks[mark] = Some(end_i - self.length as i64);
        } else {
            self.marks[mark] = None;
        }
        Ok(())
    }

    fn run(&mut self) -> Result<usize, WriteErr<E::Err>> {
        let length = self.length;
        let len = length as i64;
        let mut i: i64 = 0;

        while i < len {
            let c = py_get(self.data, i);

            if self.state == START {
                // Skip leading newlines.
                if c == CR || c == LF {
                    match py_find(self.data, b"-", i, None) {
                        None => {
                            break;
                        }
                        Some(p) => {
                            i = p as i64;
                            continue;
                        }
                    }
                }
                self.index = 0;
                self.state = START_BOUNDARY;
                i -= 1;
            } else if self.state == START_BOUNDARY {
                if self.index == 0
                    && py_startswith(self.data, &self.boundary[2..], i, self.length)
                {
                    self.index = self.blen - 2;
                    i += self.index as i64;
                    continue;
                }
                if self.index == self.blen - 2 {
                    if c == HYPHEN {
                        self.state = END_BOUNDARY;
                    } else if c != CR {
                        let message =
                            format!("Did not find CR at end of boundary ({i})");
                        return self.fail(message, i);
                    }
                    self.index += 1;
                } else if self.index == self.blen - 1 {
                    if c != LF {
                        let message =
                            format!("Did not find LF at end of boundary ({i})");
                        return self.fail(message, i);
                    }
                    self.index = 0;
                    self.emit.event("part_begin").map_err(WriteErr::Callback)?;
                    self.chc = 0;
                    self.chs = 0;
                    self.state = HEADER_FIELD_START;
                } else if c != self.boundary[self.index + 2] {
                    let message = format!(
                        "Expected boundary character {:?}, got {:?} at index {}",
                        self.boundary[self.index + 2],
                        c,
                        self.index + 2
                    );
                    return self.fail(message, i);
                } else {
                    self.index += 1;
                }
            } else if self.state == HEADER_FIELD_START {
                self.index = 0;
                if c != CR {
                    self.chc += 1;
                    if self.chc > self.max_hc {
                        return self.fail("Maximum header count exceeded".to_string(), i);
                    }
                    self.chs = 0;
                }
                self.marks[MARK_HEADER_FIELD] = Some(i);
                if c != CR {
                    self.emit.event("header_begin").map_err(WriteErr::Callback)?;
                }
                self.state = HEADER_FIELD;
                i -= 1;
            } else if self.state == HEADER_FIELD {
                if c == CR && self.index == 0 {
                    self.marks[MARK_HEADER_FIELD] = None;
                    self.state = HEADERS_ALMOST_DONE;
                    i += 1;
                    continue;
                }
                let colon = py_find(self.data, b":", i, Some(len));
                let end = colon.map(|p| p as i64).unwrap_or(len);
                // Enforce the size limit before scanning, so oversized names
                // fail fast instead of copying a huge span.
                if colon.is_none() {
                    self.advance((end - i) as usize, i)?;
                } else {
                    self.advance((end - i + 1) as usize, i)?;
                }
                let bad = self.data[i as usize..end as usize]
                    .iter()
                    .find(|&&b| !TOKEN[b as usize]);
                if let Some(&b) = bad {
                    let bad_i =
                        i + self.data[i as usize..end as usize].iter().position(|&x| x == b).unwrap() as i64;
                    let message =
                        format!("Found invalid character {b:?} in header at {bad_i}");
                    return self.fail(message, bad_i);
                }
                self.index += (end - i) as usize;
                if let Some(colon) = colon {
                    if self.index == 0 {
                        let message = format!("Found 0-length header at {i}");
                        return self.fail(message, i);
                    }
                    i = colon as i64;
                    self.data_callback(MARK_HEADER_FIELD, "header_field", i, false)?;
                    self.state = HEADER_VALUE_START;
                } else {
                    i = len;
                }
            } else if self.state == HEADER_VALUE_START {
                if c == SPACE {
                    self.advance(1, i)?;
                    i += 1;
                    continue;
                }
                self.marks[MARK_HEADER_VALUE] = Some(i);
                self.state = HEADER_VALUE;
                i -= 1;
            } else if self.state == HEADER_VALUE {
                let cr = py_find(self.data, b"\r", i, Some(len));
                let end = cr.map(|p| p as i64).unwrap_or(len);
                self.advance((end - i) as usize, i)?;
                if let Some(p) = cr {
                    i = p as i64;
                    self.data_callback(MARK_HEADER_VALUE, "header_value", i, false)?;
                    self.emit.event("header_end").map_err(WriteErr::Callback)?;
                    self.chs = 0;
                    self.state = HEADER_VALUE_ALMOST_DONE;
                } else {
                    i = len;
                }
            } else if self.state == HEADER_VALUE_ALMOST_DONE {
                if c != LF {
                    let message = format!(
                        "Did not find LF character at end of header (found {c:?})"
                    );
                    return self.fail(message, i);
                }
                self.state = HEADER_FIELD_START;
            } else if self.state == HEADERS_ALMOST_DONE {
                if c != LF {
                    let message =
                        format!("Did not find LF at end of headers (found {c:?})");
                    return self.fail(message, i);
                }
                self.emit
                    .event("headers_finished")
                    .map_err(WriteErr::Callback)?;
                self.state = PART_DATA_START;
            } else if self.state == PART_DATA_START {
                self.marks[MARK_PART_DATA] = Some(i);
                self.state = PART_DATA;
                i -= 1;
            } else if self.state == PART_DATA {
                let prev_index = self.index;
                if self.index == 0 {
                    match py_find(self.data, self.boundary, i, Some(len)) {
                        Some(i0) => {
                            self.index = self.blen - 1;
                            i = (i0 + self.blen - 1) as i64;
                        }
                        None => {
                            let lo = (i).max(len - self.blen as i64 + 1);
                            if let Some(k) =
                                py_rfind_byte(self.data, self.boundary[0], lo, self.length)
                            {
                                if self.boundary.starts_with(&self.data[k..self.length]) {
                                    self.index = self.length - k;
                                }
                            }
                            i = len;
                            continue;
                        }
                    }
                }
                // Re-read: the find-hit path lands `i` on the boundary tail.
                let c = py_get(self.data, i);
                if self.index < self.blen {
                    if self.boundary[self.index] == c {
                        self.index += 1;
                    } else {
                        self.index = 0;
                    }
                } else if self.index == self.blen {
                    self.index += 1;
                    if c == CR {
                        self.flags |= FLAG_PART_BOUNDARY;
                    } else if c == HYPHEN {
                        self.flags |= FLAG_LAST_BOUNDARY;
                    } else {
                        self.index = 0;
                    }
                } else if self.index == self.blen + 1 {
                    if self.flags & FLAG_PART_BOUNDARY != 0 {
                        if c == LF {
                            self.flags &= !FLAG_PART_BOUNDARY;
                            self.data_callback(
                                MARK_PART_DATA,
                                "part_data",
                                i - self.index as i64,
                                false,
                            )?;
                            self.emit.event("part_end").map_err(WriteErr::Callback)?;
                            self.emit.event("part_begin").map_err(WriteErr::Callback)?;
                            self.chc = 0;
                            self.chs = 0;
                            self.index = 0;
                            self.state = HEADER_FIELD_START;
                            i += 1;
                            continue;
                        }
                        self.index = 0;
                        self.flags &= !FLAG_PART_BOUNDARY;
                    } else if self.flags & FLAG_LAST_BOUNDARY != 0 {
                        if c == HYPHEN {
                            self.data_callback(
                                MARK_PART_DATA,
                                "part_data",
                                i - self.index as i64,
                                false,
                            )?;
                            self.emit.event("part_end").map_err(WriteErr::Callback)?;
                            self.emit.event("end").map_err(WriteErr::Callback)?;
                            self.state = END;
                        } else {
                            self.index = 0;
                        }
                    }
                }
                if self.index == 0 && prev_index > 0 {
                    i -= 1;
                }
            } else if self.state == END_BOUNDARY {
                if self.index == self.blen - 1 {
                    if c != HYPHEN {
                        let message =
                            format!("Did not find - at end of boundary ({i})");
                        return self.fail(message, i);
                    }
                    self.index += 1;
                    self.emit.event("end").map_err(WriteErr::Callback)?;
                    self.state = END;
                }
            } else if self.state == END {
                // Silently discard epilogue data (RFC 2046 §5.1.1).
                break;
            } else {
                let message =
                    format!("Reached an unknown state {} at {i}", self.state);
                return self.fail(message, i);
            }

            i += 1;
        }

        self.data_callback(MARK_HEADER_FIELD, "header_field", len, true)?;
        self.data_callback(MARK_HEADER_VALUE, "header_value", len, true)?;
        self.data_callback(MARK_PART_DATA, "part_data", len - self.index as i64, true)?;

        Ok(length)
    }
}
