//! Streaming querystring parser core.
//!
//! Direct transcription of `QuerystringParser._internal_write`: the same
//! states, the same `find`-based span jumps, the same messages and offsets.
//! `strict` arrives per call (the Python attribute is read live, verbatim).
//!
//! License: Apache-2.0 (preserved from the original).

use crate::{py_find, py_get, Emitter, ParseFail, WriteErr, AMPERSAND};

/// `QuerystringState` values (`BEFORE_FIELD = 0`, `FIELD_NAME = 1`, `FIELD_DATA = 2`).
pub const BEFORE_FIELD: i64 = 0;
pub const FIELD_NAME: i64 = 1;
pub const FIELD_DATA: i64 = 2;

/// Evolving parse state (committed only on clean completion).
#[derive(Debug, Clone)]
pub struct QuerystringCore {
    /// Current state (`BEFORE_FIELD`/`FIELD_NAME`/`FIELD_DATA`, or garbage).
    pub state: i64,
    /// Separator-skipped flag between fields.
    pub found_sep: bool,
}

impl QuerystringCore {
    /// Fresh parser (`state = BEFORE_FIELD`, no separator seen).
    pub fn new() -> Self {
        QuerystringCore {
            state: BEFORE_FIELD,
            found_sep: false,
        }
    }

    /// Parse `data[..length]`, emitting events. Returns `length` on success.
    pub fn internal_write<E>(
        &mut self,
        data: &[u8],
        length: usize,
        strict: bool,
        emit: &mut impl Emitter<Err = E>,
    ) -> Result<usize, WriteErr<E>> {
        let mut state = self.state;
        let mut found_sep = self.found_sep;
        let mut i: i64 = 0;
        let len = length as i64;

        while i < len {
            let ch = py_get(data, i);

            if state == BEFORE_FIELD {
                if ch == AMPERSAND {
                    if found_sep {
                        if strict {
                            let message =
                                format!("Skipping duplicate ampersand at {i}");
                            emit.warn(&message).map_err(WriteErr::Callback)?;
                            return Err(ParseFail {
                                multipart: false,
                                message,
                                offset: i,
                            }
                            .into());
                        }
                        // Non-strict duplicate separators are skipped below
                        // warning level (`logger.debug` in the original).
                    } else {
                        found_sep = true;
                    }
                } else {
                    emit.event("field_start").map_err(WriteErr::Callback)?;
                    i -= 1;
                    state = FIELD_NAME;
                    found_sep = false;
                }
            } else if state == FIELD_NAME {
                let sep = py_find(data, b"&", i, Some(len));
                let equals = match sep {
                    Some(s) => py_find(data, b"=", i, Some(s as i64)),
                    None => py_find(data, b"=", i, Some(len)),
                };
                if let Some(eq) = equals {
                    if i != eq as i64 {
                        emit
                            .input("field_name", i as usize, eq)
                            .map_err(WriteErr::Callback)?;
                    }
                    i = eq as i64;
                    state = FIELD_DATA;
                } else if !strict {
                    if let Some(s) = sep {
                        if i != s as i64 {
                            emit
                                .input("field_name", i as usize, s)
                                .map_err(WriteErr::Callback)?;
                        }
                        emit.event("field_end").map_err(WriteErr::Callback)?;
                        i = s as i64 - 1;
                        state = BEFORE_FIELD;
                    } else {
                        if i != len {
                            emit
                                .input("field_name", i as usize, length)
                                .map_err(WriteErr::Callback)?;
                        }
                        i = length as i64;
                    }
                } else {
                    if sep.is_some() {
                        let message = "When strict_parsing is True, we require an equals sign in all field chunks. Did not find one in the chunk that starts at ".to_string()
                            + &i.to_string();
                        emit.warn(&message).map_err(WriteErr::Callback)?;
                        return Err(ParseFail {
                            multipart: false,
                            message,
                            offset: i,
                        }
                        .into());
                    }
                    if i != len {
                        emit
                            .input("field_name", i as usize, length)
                            .map_err(WriteErr::Callback)?;
                    }
                    i = length as i64;
                }
            } else if state == FIELD_DATA {
                match py_find(data, b"&", i, Some(len)) {
                    Some(s) => {
                        if i != s as i64 {
                            emit
                                .input("field_data", i as usize, s)
                                .map_err(WriteErr::Callback)?;
                        }
                        emit.event("field_end").map_err(WriteErr::Callback)?;
                        i = s as i64 - 1;
                        state = BEFORE_FIELD;
                    }
                    None => {
                        if i != len {
                            emit
                                .input("field_data", i as usize, length)
                                .map_err(WriteErr::Callback)?;
                        }
                        i = length as i64;
                    }
                }
            } else {
                let message = format!("Reached an unknown state {state} at {i}");
                emit.warn(&message).map_err(WriteErr::Callback)?;
                return Err(ParseFail {
                    multipart: false,
                    message,
                    offset: i,
                }
                .into());
            }

            i += 1;
        }

        self.state = state;
        self.found_sep = found_sep;
        Ok(length)
    }
}

impl Default for QuerystringCore {
    fn default() -> Self {
        Self::new()
    }
}
