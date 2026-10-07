// SPDX-License-Identifier: Apache-2.0
//! VTIMEZONE parser, mirroring `dateutil.tz.tzical._parse_rfc`.
//! Component recurrence lines are returned raw; the binding feeds them
//! to `rrulestr` (which needs the Python parser for `UNTIL`).

/// One STANDARD/DAYLIGHT component.
#[derive(Clone, Debug, PartialEq)]
pub struct IcalComp {
    pub isdst: bool,
    pub offsetfrom: i64,
    pub offsetto: i64,
    pub tzname: Option<String>,
    /// Joined RRULE/RDATE/EXRULE/EXDATE/DTSTART lines, or `None`.
    pub rrule_lines: Option<String>,
}

/// One VTIMEZONE with its components.
#[derive(Clone, Debug, PartialEq)]
pub struct IcalZone {
    pub tzid: String,
    pub comps: Vec<IcalComp>,
}

/// Parse failure (the binding maps to `ValueError` with the same message).
#[derive(Clone, Debug, PartialEq)]
pub struct IcalError(pub String);

fn err(msg: &str) -> IcalError {
    IcalError(msg.to_string())
}

/// Unfold folded content lines (shared with `rrulestr` unfolding).
pub fn unfold_lines(s: &str) -> Result<Vec<String>, IcalError> {
    let mut lines: Vec<String> = s.lines().map(|l: &str| l.trim_end().to_string()).collect();
    if lines.iter().all(|l| l.is_empty()) {
        return Err(err("empty string"));
    }
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].clone();
        if line.is_empty() {
            lines.remove(i);
        } else if i > 0 && line.starts_with(' ') {
            let cont = line[1..].to_string();
            lines[i - 1].push_str(&cont);
            lines.remove(i);
        } else {
            i += 1;
        }
    }
    Ok(lines)
}
fn parse_int_part(part: &str) -> Result<i64, IcalError> {
    // Mirrors `int(part)`: surrounding whitespace and signs accepted.
    let t = part.trim();
    t.parse::<i64>()
        .map_err(|_| err(&format!("invalid literal for int() with base 10: '{}'", part)))
}

fn parse_offset(s: &str) -> Result<i64, IcalError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(err("empty offset"));
    }
    let (signal, s) = match s.chars().next().unwrap() {
        '+' => (1, &s[1..]),
        '-' => (-1, &s[1..]),
        _ => (1, s),
    };
    if s.len() == 4 {
        Ok(signal * (parse_int_part(&s[..2])? * 3600 + parse_int_part(&s[2..])? * 60))
    } else if s.len() == 6 {
        Ok(signal
            * (parse_int_part(&s[..2])? * 3600
                + parse_int_part(&s[2..4])? * 60
                + parse_int_part(&s[4..])?))
    } else {
        Err(err(&format!("invalid offset: {}", s)))
    }
}

/// Parse the full VTIMEZONE text (mirrors `_parse_rfc`).
pub fn parse_ical(s: &str) -> Result<Vec<IcalZone>, IcalError> {
    let lines = unfold_lines(s)?;
    let mut zones: Vec<IcalZone> = Vec::new();
    let mut tzid: Option<String> = None;
    let mut comps: Vec<IcalComp> = Vec::new();
    let mut invtz = false;
    let mut comptype: Option<String> = None;
    let mut founddtstart = false;
    let mut tzoffsetfrom: Option<i64> = None;
    let mut tzoffsetto: Option<i64> = None;
    let mut rrulelines: Vec<String> = Vec::new();
    let mut tzname: Option<String> = None;

    for line in &lines {
        if line.is_empty() {
            continue;
        }
        let (name0, value) = match line.split_once(':') {
            Some((n, v)) => (n.to_string(), v.to_string()),
            None => return Err(err(&format!("invalid line: {}", line))),
        };
        let parms: Vec<&str> = name0.split(';').collect();
        if parms.is_empty() {
            return Err(err("empty property name"));
        }
        let name = parms[0].to_uppercase();
        let rest = &parms[1..];
        let _ = rest;
        if invtz {
            if name == "BEGIN" {
                if value != "STANDARD" && value != "DAYLIGHT" {
                    return Err(err(&format!("unknown component: {}", value)));
                }
                comptype = Some(value);
                founddtstart = false;
                tzoffsetfrom = None;
                tzoffsetto = None;
                rrulelines = Vec::new();
                tzname = None;
            } else if name == "END" {
                if value == "VTIMEZONE" {
                    match &comptype {
                        Some(c) => return Err(err(&format!("component not closed: {}", c))),
                        None => {}
                    }
                    let tzid = tzid.take().ok_or_else(|| err("mandatory TZID not found"))?;
                    if comps.is_empty() {
                        return Err(err("at least one component is needed"));
                    }
                    zones.push(IcalZone { tzid, comps: std::mem::take(&mut comps) });
                    invtz = false;
                } else if Some(&value) == comptype.as_ref() {
                    if !founddtstart {
                        return Err(err("mandatory DTSTART not found"));
                    }
                    let from = tzoffsetfrom.ok_or_else(|| err("mandatory TZOFFSETFROM not found"))?;
                    let to = tzoffsetto.ok_or_else(|| err("mandatory TZOFFSETFROM not found"))?;
                    let rr = if rrulelines.is_empty() {
                        None
                    } else {
                        Some(rrulelines.join("\n"))
                    };
                    comps.push(IcalComp {
                        isdst: comptype.as_deref() == Some("DAYLIGHT"),
                        offsetfrom: from,
                        offsetto: to,
                        tzname: tzname.take(),
                        rrule_lines: rr,
                    });
                    comptype = None;
                } else {
                    return Err(err(&format!("invalid component end: {}", value)));
                }
            } else if comptype.is_some() {
                match name.as_str() {
                    "DTSTART" => {
                        for parm in parms.iter().skip(1) {
                            if *parm != "VALUE=DATE-TIME" {
                                return Err(err(&format!(
                                    "Unsupported DTSTART param in VTIMEZONE: {}",
                                    parm
                                )));
                            }
                        }
                        rrulelines.push(line.clone());
                        founddtstart = true;
                    }
                    "RRULE" | "RDATE" | "EXRULE" | "EXDATE" => {
                        rrulelines.push(line.clone());
                    }
                    "TZOFFSETFROM" => {
                        if parms.len() > 1 {
                            return Err(err(&format!("unsupported {} parm: {} ", name, parms[1])));
                        }
                        tzoffsetfrom = Some(parse_offset(&value)?);
                    }
                    "TZOFFSETTO" => {
                        if parms.len() > 1 {
                            return Err(err(&format!("unsupported TZOFFSETTO parm: {}", parms[1])));
                        }
                        tzoffsetto = Some(parse_offset(&value)?);
                    }
                    "TZNAME" => {
                        if parms.len() > 1 {
                            return Err(err(&format!("unsupported TZNAME parm: {}", parms[1])));
                        }
                        tzname = Some(value);
                    }
                    "COMMENT" => {}
                    _ => return Err(err(&format!("unsupported property: {}", name))),
                }
            } else if name == "TZID" {
                if parms.len() > 1 {
                    return Err(err(&format!("unsupported TZID parm: {}", parms[1])));
                }
                tzid = Some(value);
            } else if name == "TZURL" || name == "LAST-MODIFIED" || name == "COMMENT" {
            } else {
                return Err(err(&format!("unsupported property: {}", name)));
            }
        } else if name == "BEGIN" && value == "VTIMEZONE" {
            tzid = None;
            comps = Vec::new();
            invtz = true;
        }
    }
    Ok(zones)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VTIMEZONE: &str = "BEGIN:VTIMEZONE\nTZID:CustomZone\nBEGIN:STANDARD\nDTSTART:19671029T020000\nRRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU;UNTIL=20061029T020000\nTZOFFSETFROM:-0400\nTZOFFSETTO:-0500\nEND:STANDARD\nBEGIN:DAYLIGHT\nDTSTART:19870405T020000\nRRULE:FREQ=YEARLY;BYMONTH=4;BYDAY=1SU;UNTIL=20060402T020000\nTZOFFSETFROM:-0500\nTZOFFSETTO:-0400\nEND:DAYLIGHT\nEND:VTIMEZONE";

    #[test]
    fn parses_custom_zone() {
        let zones = parse_ical(VTIMEZONE).unwrap();
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].tzid, "CustomZone");
        assert_eq!(zones[0].comps.len(), 2);
        let std = &zones[0].comps[0];
        assert!(!std.isdst);
        assert_eq!((std.offsetfrom, std.offsetto), (-14400, -18000));
        assert!(std.rrule_lines.as_ref().unwrap().contains("BYDAY=-1SU"));
        let dst = &zones[0].comps[1];
        assert!(dst.isdst);
        assert_eq!((dst.offsetfrom, dst.offsetto), (-18000, -14400));
    }
}
