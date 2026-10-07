// SPDX-License-Identifier: Apache-2.0
//! `parser._parse` + `_parse_numeric_token` and helpers: the dispatch loop
//! over lexer tokens producing an unvalidated [`ParseOk`].

use std::cmp::Ordering;

use super::dec::Dec;
use super::lex::{fold_digits, is_num, probe_float};
use super::ymd::{cmp_norm, Ymd};
use super::{Fail, Info, ParseOk, ParseOptions, TzOff};

fn token_is_digit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_num)
}

fn chars_of(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn slice_chars(s: &[char], from: usize, to: Option<usize>) -> String {
    let end = to.unwrap_or(s.len());
    s[from.min(s.len())..end.min(s.len())].iter().collect()
}

fn all_upper(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| c.is_ascii_uppercase())
}

fn could_be_tzname(
    hour: &Option<String>,
    tzname: &Option<String>,
    tzoffset: &Option<TzOff>,
    token: &str,
    info: &dyn Info,
) -> bool {
    hour.is_some()
        && tzname.is_none()
        && tzoffset.is_none()
        && token.chars().count() <= 5
        && (all_upper(token) || info.is_utc_abbr(token))
}

fn ampm_valid(
    hour: &Option<String>,
    ampm: &Option<i64>,
    fuzzy: bool,
) -> Result<bool, Fail> {
    let mut val_is_ampm = true;
    if fuzzy && ampm.is_some() {
        val_is_ampm = false;
    }
    match hour {
        None => {
            if fuzzy {
                val_is_ampm = false;
            } else {
                return Err(Fail::NoResult); // 'No hour specified ...'
            }
        }
        Some(h) => {
            if cmp_norm(h, 0) == Ordering::Less || cmp_norm(h, 12) == Ordering::Greater {
                if fuzzy {
                    val_is_ampm = false;
                } else {
                    return Err(Fail::NoResult); // 'Invalid hour ...'
                }
            }
        }
    }
    Ok(val_is_ampm)
}

fn adjust_ampm(hour: &str, ampm: i64) -> String {
    let h: i64 = hour.parse().unwrap_or(-1);
    if h < 12 && ampm == 1 {
        (h + 12).to_string()
    } else if h == 12 && ampm == 0 {
        "0".to_string()
    } else {
        hour.to_string()
    }
}

fn parse_min_sec(v: &Dec) -> (String, Option<u32>) {
    let minute = v.int_part().to_string();
    let second = if v.frac_nonzero {
        Some(v.frac60())
    } else {
        None
    };
    (minute, second)
}

fn parsems(value: &str) -> Result<(String, u32), Fail> {
    if !value.contains('.') {
        let i = super::dec::norm_int_str(value).ok_or(Fail::NoResult)?;
        return Ok((i, 0));
    }
    let mut it = value.split('.');
    let i = it.next().ok_or(Fail::NoResult)?;
    let f = it.next().ok_or(Fail::NoResult)?;
    if it.next().is_some() {
        return Err(Fail::NoResult);
    }
    let si = super::dec::norm_int_str(i).ok_or(Fail::NoResult)?;
    if f.is_empty() {
        return Ok((si, 0));
    }
    let folded = fold_digits(f).ok_or(Fail::NoResult)?;
    if !folded.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Fail::NoResult);
    }
    let mut digs = folded;
    while digs.len() < 6 {
        digs.push('0');
    }
    digs.truncate(6);
    let micro: u32 = digs.parse().map_err(|_| Fail::NoResult)?;
    Ok((si, micro))
}

/// `_find_hms_idx` + `_parse_hms` fused: label position and value, with a
/// single `hms()` probe per candidate (the original probes twice; the
/// tables are pure so this is unobservable).
fn find_hms(idx: usize, tokens: &[String], info: &dyn Info) -> Option<(usize, i64)> {
    let len_l = tokens.len();
    if idx + 1 < len_l {
        if let Some(h) = info.hms(&tokens[idx + 1]) {
            return Some((idx + 1, h));
        }
    }
    if idx + 2 < len_l && tokens[idx + 1] == " " {
        if let Some(h) = info.hms(&tokens[idx + 2]) {
            return Some((idx + 2, h));
        }
    }
    if idx > 0 {
        if let Some(h) = info.hms(&tokens[idx - 1]) {
            return Some((idx, h + 1));
        }
    }
    if 1 < idx && idx == len_l - 1 && tokens[idx - 1] == " " {
        if let Some(h) = info.hms(&tokens[idx - 2]) {
            return Some((idx, h + 1));
        }
    }
    None
}

fn assign_hms(res: &mut ParseOk, value_repr: &str, hms: i64) -> Result<(), Fail> {
    let value = Dec::parse(value_repr).ok_or(Fail::NoResult)?;
    if hms == 0 {
        res.hour = Some(value.int_part().to_string());
        if value.frac_nonzero {
            res.minute = Some(value.frac60().to_string());
        }
    } else if hms == 1 {
        let (minute, second) = parse_min_sec(&value);
        res.minute = Some(minute);
        res.second = second.map(|s| s.to_string());
    } else if hms == 2 {
        let (second, micro) = parsems(value_repr)?;
        res.second = Some(second);
        res.micro = Some(micro);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn parse_numeric_token(
    tokens: &[String],
    idx: usize,
    info: &dyn Info,
    ymd: &mut Ymd,
    res: &mut ParseOk,
    fuzzy: bool,
) -> Result<usize, Fail> {
    let value_repr = tokens[idx].clone();
    let folded = fold_digits(&value_repr).ok_or(Fail::NoResult)?;
    let value = Dec::parse(&folded).ok_or(Fail::NoResult)?;
    let len_li = value_repr.chars().count();
    let len_l = tokens.len();
    let fc: Vec<char> = folded.chars().collect();

    if ymd.len() == 3
        && (len_li == 2 || len_li == 4)
        && res.hour.is_none()
        && (idx + 1 >= len_l
            || (tokens[idx + 1] != ":" && info.hms(&tokens[idx + 1]).is_none()))
    {
        res.hour = Some(slice_chars(&fc, 0, Some(2)));
        if len_li == 4 {
            res.minute = Some(slice_chars(&fc, 2, None));
        }
        return Ok(idx);
    }
    if len_li == 6 || (len_li > 6 && fc.iter().position(|&c| c == '.') == Some(6)) {
        if ymd.is_empty() && !value_repr.contains('.') {
            ymd.append_str(&slice_chars(&fc, 0, Some(2)), None)?;
            ymd.append_str(&slice_chars(&fc, 2, Some(4)), None)?;
            ymd.append_str(&slice_chars(&fc, 4, None), None)?;
        } else {
            res.hour = Some(slice_chars(&fc, 0, Some(2)));
            res.minute = Some(slice_chars(&fc, 2, Some(4)));
            let (sec, micro) = parsems(&slice_chars(&fc, 4, None))?;
            res.second = Some(sec);
            res.micro = Some(micro);
        }
        return Ok(idx);
    }
    if len_li == 8 || len_li == 12 || len_li == 14 {
        ymd.append_str(&slice_chars(&fc, 0, Some(4)), Some('Y'))?;
        ymd.append_str(&slice_chars(&fc, 4, Some(6)), None)?;
        ymd.append_str(&slice_chars(&fc, 6, Some(8)), None)?;
        if len_li > 8 {
            res.hour = Some(slice_chars(&fc, 8, Some(10)));
            res.minute = Some(slice_chars(&fc, 10, Some(12)));
            if len_li > 12 {
                let s =
                    super::dec::norm_int_str(&slice_chars(&fc, 12, None)).ok_or(Fail::NoResult)?;
                res.second = Some(s);
            }
        }
        return Ok(idx);
    }
    if let Some((new_idx, hms)) = find_hms(idx, tokens, info) {
        if hms == 0 || hms == 1 || hms == 2 {
            assign_hms(res, &folded, hms)?;
        }
        return Ok(new_idx);
    }
    if idx + 2 < len_l && tokens[idx + 1] == ":" {
        res.hour = Some(value.int_part().to_string());
        let folded2 = fold_digits(&tokens[idx + 2]).ok_or(Fail::NoResult)?;
        let v2 = Dec::parse(&folded2).ok_or(Fail::NoResult)?;
        let (minute, second) = parse_min_sec(&v2);
        res.minute = Some(minute);
        res.second = second.map(|s| s.to_string());
        let mut nidx = idx + 2;
        if idx + 4 < len_l && tokens[idx + 3] == ":" {
            let (sec, micro) = parsems(&tokens[idx + 4])?;
            res.second = Some(sec);
            res.micro = Some(micro);
            nidx += 2;
        }
        return Ok(nidx);
    }
    if idx + 1 < len_l
        && (tokens[idx + 1] == "-" || tokens[idx + 1] == "/" || tokens[idx + 1] == ".")
    {
        let sep = tokens[idx + 1].clone();
        ymd.append_str(&value_repr, None)?;
        if idx + 2 < len_l && !info.jump(&tokens[idx + 2]) {
            if token_is_digit(&tokens[idx + 2]) {
                ymd.append_str(&tokens[idx + 2], None)?;
            } else {
                let m = info.month(&tokens[idx + 2]).ok_or(Fail::NoResult)?;
                ymd.append_small(m, 'M')?;
            }
            if idx + 3 < len_l && tokens[idx + 3] == sep {
                let nxt = tokens.get(idx + 4).ok_or(Fail::NoResult)?;
                match info.month(nxt) {
                    Some(m) => ymd.append_small(m, 'M')?,
                    None => ymd.append_str(nxt, None)?,
                }
                return Ok(idx + 4);
            }
            return Ok(idx + 2);
        }
        return Ok(idx + 1);
    }
    if idx + 1 >= len_l || info.jump(&tokens[idx + 1]) {
        if idx + 2 < len_l && info.ampm(&tokens[idx + 2]).is_some() {
            let a = info.ampm(&tokens[idx + 2]).unwrap_or(0);
            res.hour = Some(adjust_ampm(value.int_part(), a));
            return Ok(idx + 2);
        }
        ymd.append_dec(&value, None)?;
        return Ok(idx + 1);
    }
    if idx + 1 < len_l
        && info.ampm(&tokens[idx + 1]).is_some()
        && value.cmp_u32(0) != Ordering::Less
        && value.cmp_u32(24) == Ordering::Less
    {
        let a = info.ampm(&tokens[idx + 1]).unwrap_or(0);
        res.hour = Some(adjust_ampm(value.int_part(), a));
        return Ok(idx + 1);
    }
    if ymd.could_be_day(&value, info)? {
        ymd.append_dec(&value, None)?;
        return Ok(idx);
    }
    if !fuzzy {
        return Err(Fail::NoResult);
    }
    Ok(idx)
}

fn pint(s: &str) -> Result<i128, Fail> {
    let folded = fold_digits(s).ok_or(Fail::NoResult)?;
    folded.parse::<i128>().map_err(|_| Fail::NoResult)
}

fn tzoff_secs(signal: i64, h: i128, m: i128) -> Result<TzOff, Fail> {
    let total = h
        .checked_mul(3600)
        .and_then(|x| x.checked_add(m.checked_mul(60)?))
        .ok_or(Fail::NoResult)?;
    let v = total.checked_mul(signal as i128).ok_or(Fail::NoResult)?;
    if v >= i64::MIN as i128 && v <= i64::MAX as i128 {
        Ok(TzOff::Secs(v as i64))
    } else {
        Ok(TzOff::Big(v.to_string()))
    }
}

/// Mirrors `parser._parse` (unvalidated; the binding validates, so custom
/// `parserinfo.validate` overrides can replace the default).
pub fn parse_tokens(
    tokens: &mut [String],
    info: &dyn Info,
    opt: &ParseOptions,
) -> Result<ParseOk, Fail> {
    let mut res = ParseOk::default();
    let mut ymd = Ymd::new();
    let mut skipped_idxs: Vec<usize> = Vec::new();
    let len_l = tokens.len();
    let mut i = 0;
    while i < len_l {
        let value_repr = tokens[i].clone();
        if probe_float(&value_repr) {
            i = parse_numeric_token(tokens, i, info, &mut ymd, &mut res, opt.fuzzy)?;
        } else if let Some(w) = info.weekday(&tokens[i]) {
            res.weekday = Some(w);
        } else if let Some(m) = info.month(&tokens[i]) {
            ymd.append_small(m, 'M')?;
            if i + 1 < len_l {
                if tokens[i + 1] == "-" || tokens[i + 1] == "/" {
                    let sep = tokens[i + 1].clone();
                    let nxt = tokens.get(i + 2).ok_or(Fail::NoResult)?.clone();
                    ymd.append_str(&nxt, None)?;
                    if i + 3 < len_l && tokens[i + 3] == sep {
                        let nxt2 = tokens.get(i + 4).ok_or(Fail::NoResult)?.clone();
                        ymd.append_str(&nxt2, None)?;
                        i += 2;
                    }
                    i += 2;
                } else if i + 4 < len_l
                    && tokens[i + 1] == " "
                    && tokens[i + 3] == " "
                    && info.pertain(&tokens[i + 2])
                {
                    if token_is_digit(&tokens[i + 4]) {
                        let raw = tokens[i + 4].clone();
                        let ival = pint(&raw).map_err(|_| Fail::NoResult)?;
                        let year = info.convertyear(
                            &ival.to_string(),
                            false,
                            opt.cur_year,
                            opt.century,
                        );
                        ymd.append_str(&year, Some('Y'))?;
                    }
                    i += 4;
                }
            }
        } else if let Some(a) = info.ampm(&tokens[i]) {
            if ampm_valid(&res.hour, &res.ampm, opt.fuzzy)? {
                let h = res.hour.clone().unwrap_or_else(|| "0".to_string());
                res.hour = Some(adjust_ampm(&h, a));
                res.ampm = Some(a);
            } else if opt.fuzzy {
                skipped_idxs.push(i);
            }
        } else if could_be_tzname(&res.hour, &res.tzname, &res.tzoffset, &tokens[i], info) {
            res.tzname = Some(tokens[i].clone());
            res.tzoffset = info.tzoffset(&tokens[i]);
            if i + 1 < len_l && (tokens[i + 1] == "+" || tokens[i + 1] == "-") {
                tokens[i + 1] = if tokens[i + 1] == "+" {
                    "-".to_string()
                } else {
                    "+".to_string()
                };
                res.tzoffset = None;
                if info.utczone(res.tzname.as_deref().unwrap_or("")) {
                    res.tzname = None;
                }
            }
        } else if res.hour.is_some() && (tokens[i] == "+" || tokens[i] == "-") {
            let signal = if tokens[i] == "+" { 1 } else { -1 };
            let nxt = tokens.get(i + 1).ok_or(Fail::NoResult)?.clone();
            let len_li = nxt.chars().count();
            if len_li == 4 {
                let nc = chars_of(&nxt);
                let h = pint(&slice_chars(&nc, 0, Some(2)))?;
                let m = pint(&slice_chars(&nc, 2, None))?;
                res.tzoffset = Some(tzoff_secs(signal, h, m)?);
            } else if i + 2 < len_l && tokens[i + 2] == ":" {
                let h = pint(&nxt)?;
                let m = pint(tokens.get(i + 3).ok_or(Fail::NoResult)?)?;
                res.tzoffset = Some(tzoff_secs(signal, h, m)?);
                i += 2;
            } else if len_li <= 2 {
                let nc = chars_of(&nxt);
                let h = pint(&slice_chars(&nc, 0, Some(2)))?;
                res.tzoffset = Some(tzoff_secs(signal, h, 0)?);
            } else {
                return Err(Fail::NoResult);
            }
            let tz4 = tokens.get(i + 4).cloned().unwrap_or_default();
            if i + 5 < len_l
                && info.jump(tokens.get(i + 2).map(|s| s.as_str()).unwrap_or(""))
                && tokens.get(i + 3).map(|s| s.as_str()).unwrap_or("") == "("
                && tokens.get(i + 5).map(|s| s.as_str()).unwrap_or("") == ")"
                && tz4.chars().count() >= 3
                && could_be_tzname(&res.hour, &res.tzname, &None, &tz4, info)
            {
                res.tzname = Some(tz4);
                i += 4;
            }
            i += 1;
        } else if !(info.jump(&tokens[i]) || opt.fuzzy) {
            return Err(Fail::NoResult);
        } else {
            skipped_idxs.push(i);
        }
        i += 1;
    }
    let (year, month, day) = ymd.resolve_ymd(opt.yearfirst, opt.dayfirst)?;
    res.year = year;
    res.month = month;
    res.day = day;
    res.century_specified = ymd.century_specified;
    res.skipped = super::recombine_skipped(tokens, &skipped_idxs);
    Ok(res)
}
