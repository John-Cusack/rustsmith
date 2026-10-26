//! Write-time scrubber for training-record export (report §2.3).
//!
//! Policy: deny-list scan over free-text fields (`proposal_text`,
//! `patch_text`, council reasoning, bench stderr). A hit redacts the span
//! (`[REDACTED:secret]` / `[REDACTED:pii]`), sets `scrubbed`, and bumps
//! `scrub_count`. Home-dir paths rewrite to `$WORK` (normalization, not a
//! scrub hit). Held-out magnitudes are NOT handled here: gate-summary
//! builders in `export.rs` never copy them in the first place.
//!
//! Deliberately dependency-free (no regex crate): hand-rolled ASCII-anchored
//! scanners. Notably there is NO entropy/long-blob rule: 40-char hex SHAs,
//! hashes, and test vectors are normal patch content and must survive.

/// Placeholder for redacted secret spans.
pub const REDACTED_SECRET: &str = "[REDACTED:secret]";
/// Placeholder for redacted PII spans (email, phone, card/SSN shapes).
pub const REDACTED_PII: &str = "[REDACTED:pii]";

/// Outcome of scrubbing one free-text field.
#[derive(Debug, Clone)]
pub struct ScrubOutcome {
    /// Scrubbed text (`$WORK`-rewritten, spans redacted).
    pub text: String,
    /// True when at least one span was redacted.
    pub scrubbed: bool,
    /// Number of redacted spans.
    pub scrub_count: usize,
    /// Original characters covered by redactions (gutted-patch rule input).
    pub redacted_chars: usize,
    /// Length of the scanned text in characters.
    pub orig_chars: usize,
}

impl ScrubOutcome {
    /// Fraction of the field covered by redactions. `export.rs` drops a
    /// record whose patch exceeds 50% redaction (a gutted example misleads).
    pub fn redacted_fraction(&self) -> f64 {
        if self.orig_chars == 0 {
            return 0.0;
        }
        self.redacted_chars as f64 / self.orig_chars as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Secret,
    Pii,
}

impl Kind {
    fn placeholder(self) -> &'static str {
        match self {
            Kind::Secret => REDACTED_SECRET,
            Kind::Pii => REDACTED_PII,
        }
    }
}

/// Scrub one free-text field. `home` is the exporter host's `$HOME`
/// (absolute home-dir occurrences rewrite to `$WORK`).
pub fn scrub_text(input: &str, home: &str) -> ScrubOutcome {
    // 1. Home-dir normalization (not a scrub hit).
    let mut text = input.to_string();
    if !home.is_empty() {
        text = text.replace(home, "$WORK");
    }
    text = rewrite_user_homes(&text);
    let orig_chars = text.chars().count();

    // 2. Collect redaction spans.
    let mut spans: Vec<(usize, usize, Kind)> = Vec::new();
    find_secret_spans(&text, &mut spans);
    find_pii_spans(&text, &mut spans);
    find_host_spans(&text, &mut spans);

    // 3. Sort (start asc, secrets first, longer first), drop overlaps, apply.
    // A secret span replaces an overlapping PII span (e.g. a URL password
    // inside an email-shaped `user:pass@host`): secrets outrank PII.
    spans.sort_by(|a, b| a.0.cmp(&b.0).then((a.2 as u8).cmp(&(b.2 as u8))).then(b.1.cmp(&a.1)));
    let mut kept: Vec<(usize, usize, Kind)> = Vec::new();
    for s in spans {
        if kept.last().is_none_or(|last| s.0 >= last.1) {
            kept.push(s);
            continue;
        }
        // Overlap: a secret span evicts every PII span it touches.
        if matches!(s.2, Kind::Secret) {
            while kept.last().is_some_and(|last| s.0 < last.1) {
                kept.pop();
            }
            kept.push(s);
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut redacted_chars = 0;
    for (s, e, k) in &kept {
        out.push_str(&text[cursor..*s]);
        out.push_str(k.placeholder());
        redacted_chars += text[*s..*e].chars().count();
        cursor = *e;
    }
    out.push_str(&text[cursor..]);
    ScrubOutcome {
        text: out,
        scrubbed: !kept.is_empty(),
        scrub_count: kept.len(),
        redacted_chars,
        orig_chars,
    }
}

/// Rewrite `/home/<user>...` and `/Users/<name>...` prefixes to `$WORK/...`.
/// All markers are ASCII so byte indices stay on char edges.
fn rewrite_user_homes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let rest = &text[i..];
        // Advance by one full character (byte pushes corrupt multi-byte UTF-8).
        let prefix_len = if rest.starts_with("/home/") {
            6
        } else if rest.starts_with("/Users/") {
            7
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
            continue;
        };
        // Consume the username; rewrite only when followed by end or `/`.
        let mut j = i + prefix_len;
        while j < b.len()
            && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'-' || b[j] == b'.')
        {
            j += 1;
        }
        if j == i + prefix_len || (j < b.len() && b[j] != b'/') {
            let c = rest.chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        out.push_str("$WORK");
        i = j;
    }
    out
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~' | '+' | '/' | '=')
}

fn find_secret_spans(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    secret_token_prefixes(text, spans);
    private_key_blocks(text, spans);
    bearer_tokens(text, spans);
    key_value_secrets(text, spans);
    url_passwords(text, spans);
}

/// Fixed secret-token prefixes + trailing token run.
fn secret_token_prefixes(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    // (prefix, trailing run required, min trailing length)
    let prefixes = [
        ("AKIA", false, 16), // AWS access key: exactly 16 [0-9A-Z] follow
        ("ghp_", true, 8),
        ("github_pat_", true, 8),
        ("gho_", true, 8),
        ("ghu_", true, 8),
        ("sk-ant-", true, 8),
        ("xoxb-", true, 8),
        ("xoxp-", true, 8),
        ("xoxa-", true, 8),
        ("xoxs-", true, 8),
        ("tskey-", true, 8),
        ("sk-", true, 8),
    ];
    for (prefix, run, min_len) in prefixes {
        let mut search = 0;
        while let Some(pos) = text[search..].find(prefix).map(|p| search + p) {
            let token_start = pos + prefix.len();
            let mut end = token_start;
            if prefix == "AKIA" {
                let tail = &text[token_start..];
                let run_len: usize = tail
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    .map(|c| c.len_utf8())
                    .sum();
                if run_len >= 16 {
                    end = token_start + 16;
                }
            } else if run {
                let tail = &text[token_start..];
                let run_len: usize = tail
                    .chars()
                    .take_while(|c| is_token_char(*c))
                    .map(|c| c.len_utf8())
                    .sum();
                if run_len >= min_len {
                    end = token_start + run_len;
                }
            }
            if end > pos + prefix.len() {
                spans.push((pos, end, Kind::Secret));
                search = end;
            } else {
                search = pos + prefix.len();
            }
        }
    }
}

/// `-----BEGIN ... PRIVATE KEY-----` through the matching END line (or the
/// single line when no END marker follows).
fn private_key_blocks(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let mut search = 0;
    while let Some(pos) = text[search..].find("-----BEGIN ").map(|p| search + p) {
        let head_end = text[pos..].find('\n').map(|n| pos + n).unwrap_or(text.len());
        if !text[pos..head_end].contains("PRIVATE KEY-----") {
            search = pos + 11;
            continue;
        }
        let mut end = head_end;
        if let Some(epos) = text[head_end..].find("-----END ") {
            let line_end = text[head_end + epos..]
                .find('\n')
                .map(|n| head_end + epos + n)
                .unwrap_or(text.len());
            end = line_end;
        }
        spans.push((pos, end, Kind::Secret));
        search = end.max(pos + 1);
    }
}

/// `Bearer <token>` (HTTP auth headers pasted into logs).
fn bearer_tokens(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let mut search = 0;
    while let Some(pos) = text[search..].find("Bearer ").map(|p| search + p) {
        let start = pos + 7;
        let len: usize = text[start..]
            .chars()
            .take_while(|c| is_token_char(*c))
            .map(|c| c.len_utf8())
            .sum();
        if len >= 8 {
            // Keep the scheme word; redact the token (same rule as key=value).
            spans.push((start, start + len, Kind::Secret));
            search = start + len;
        } else {
            search = start;
        }
    }
}

/// `key = value` / `key: value` secrets: redact the value span only so the
/// key name stays as context.
fn key_value_secrets(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let keys = [
        "password",
        "passwd",
        "api_key",
        "apikey",
        "api-key",
        "secret",
        "token",
        "aws_secret_access_key",
    ];
    let lower = text.to_lowercase();
    for key in keys {
        let mut search = 0;
        while let Some(pos) = lower[search..].find(key).map(|p| search + p) {
            let before_ok = pos == 0
                || (!lower.as_bytes()[pos - 1].is_ascii_alphanumeric()
                    && lower.as_bytes()[pos - 1] != b'_');
            let mut after = pos + key.len();
            let bytes = lower.as_bytes();
            while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                after += 1;
            }
            let mut end = None;
            if after < bytes.len() && (bytes[after] == b':' || bytes[after] == b'=') {
                after += 1;
                while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                    after += 1;
                }
                // Optional quote; value runs to whitespace/quote/comma/semicolon.
                if after < bytes.len() && (bytes[after] == b'"' || bytes[after] == b'\'') {
                    after += 1;
                }
                let mut v = after;
                while v < text.len()
                    && !matches!(
                        text.as_bytes()[v],
                        b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b',' | b';'
                    )
                {
                    v += 1;
                }
                if v > after {
                    end = Some((after, v));
                }
            }
            if before_ok {
                if let Some((s, e)) = end {
                    spans.push((s, e, Kind::Secret));
                    search = e;
                    continue;
                }
            }
            search = pos + key.len();
        }
    }
}

/// Passwords embedded in URLs: `scheme://user:pass@host` → redact `pass`.
fn url_passwords(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let mut search = 0;
    while let Some(pos) = text[search..].find("://").map(|p| search + p) {
        let mut seg_end = pos + 3;
        while seg_end < text.len()
            && !matches!(
                text.as_bytes()[seg_end],
                b' ' | b'\t' | b'\n' | b'\r' | b'\'' | b'"' | b'/'
            )
        {
            seg_end += 1;
        }
        let seg = &text[pos + 3..seg_end];
        if let Some(at) = seg.find('@') {
            if let Some(colon) = seg[..at].find(':') {
                let s = pos + 3 + colon + 1;
                let e = pos + 3 + at;
                if e > s {
                    spans.push((s, e, Kind::Secret));
                }
            }
        }
        search = seg_end.max(pos + 3);
        if search <= pos {
            break;
        }
    }
}

fn find_pii_spans(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    email_addresses(text, spans);
    phone_numbers(text, spans);
    card_and_ssn_shapes(text, spans);
    sensitive_markers(text, spans);
}

/// `local@domain.tld` scanner (ASCII; IDNA/punycode still matches).
fn email_addresses(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'@' {
            i += 1;
            continue;
        }
        // Expand left over local-part chars.
        let mut s = i;
        while s > 0
            && matches!(
                b[s - 1],
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'%' | b'+' | b'-'
            )
        {
            s -= 1;
        }
        // Expand right over domain chars, then trim trailing dots/dashes.
        let mut e = i + 1;
        while e < b.len()
            && matches!(b[e], b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-')
        {
            e += 1;
        }
        while e > i + 1 && matches!(b[e - 1], b'.' | b'-') {
            e -= 1;
        }
        // Domain must contain a dot with a 2+ letter TLD.
        let domain = &text[i + 1..e];
        let valid = s < i
            && e > i + 1
            && domain.contains('.')
            && text[s..].starts_with(|c: char| c.is_ascii_alphanumeric())
            && domain
                .rsplit('.')
                .next()
                .is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()));
        if valid {
            spans.push((s, e, Kind::Pii));
            i = e;
        } else {
            i += 1;
        }
    }
}

/// Phone-number shapes: 7-15 digits with phone separators. Conservative
/// guards: `/`-separated (dates) never match; exact `YYYY-MM-DD` never
/// matches; no digit group longer than 5 (floats like `1.75922973` and
/// long hashes are not phones; real NANP/E.164 groups are short).
fn phone_numbers(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if !(c == '+' || c.is_ascii_digit() || c == '(') {
            i += 1;
            continue;
        }
        let mut k = i;
        if b[k] == b'+' {
            k += 1;
        }
        let mut groups: Vec<usize> = vec![0];
        let mut end = k;
        while k < b.len() {
            let d = b[k] as char;
            if d.is_ascii_digit() {
                *groups.last_mut().unwrap() += 1;
                end = k + 1;
                k += 1;
            } else if matches!(d, ' ' | '(' | ')' | '-' | '.') {
                // Collapse separator runs: paren-wrapped groups ("(555)") and
                // spaced/dashed groups stay one number; empty groups never form.
                if groups.last() != Some(&0) {
                    groups.push(0);
                }
                k += 1;
            } else {
                break;
            }
        }
        // Trim trailing separators from the span.
        while end > i && matches!(b[end - 1] as char, ' ' | '(' | ')' | '-' | '.') {
            end -= 1;
        }
        let digits: usize = groups.iter().sum();
        let span = &text[i..end];
        let strong = span.starts_with('+') || span.contains('(');
        let dashed = span.chars().filter(|c| *c == '-').count() >= 2;
        let spaced = span.contains(' ');
        let groups_ok = !groups.is_empty() && groups.iter().all(|g| *g <= 5);
        // A dashed/dotted span starting mid-number is a sub-slice of a longer
        // run (e.g. `6-10-26` inside `2026-10-26`): the full run is judged
        // from its own start, never from inside.
        let mid_number = (dashed || span.contains('.'))
            && i > 0
            && (b[i - 1] as char).is_ascii_digit();
        let looks_date = span.len() == 10
            && span.as_bytes()[4] == b'-'
            && span.as_bytes()[7] == b'-'
            && span[..4].chars().all(|d| d.is_ascii_digit());
        if (7..=15).contains(&digits) && groups_ok && (strong || dashed || spaced) && !looks_date && !mid_number
        {
            spans.push((i, end, Kind::Pii));
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
}

/// Card (`dddd-dddd-dddd-dddd`) and SSN (`ddd-dd-dddd`) digit runs.
fn card_and_ssn_shapes(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let digits: Vec<(usize, char)> = text
        .char_indices()
        .filter(|(_, c)| c.is_ascii_digit() || *c == '-' || *c == ' ')
        .collect();
    let mut i = 0;
    while i < digits.len() {
        if !digits[i].1.is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut run = String::new();
        let mut k = i;
        while k < digits.len() && run.len() < 19 {
            run.push(digits[k].1);
            k += 1;
        }
        let start = digits[i].0;
        for pat in ["dddd-dddd-dddd-dddd", "dddd dddd dddd dddd", "ddd-dd-dddd"] {
            if run.len() >= pat.len() {
                let candidate: String = run.chars().take(pat.len()).collect();
                if shape_matches(&candidate, pat) {
                    // ASCII-only patterns: char length == byte length.
                    let end = start + pat.len();
                    // Word boundaries: not part of a longer digit run.
                    let before_ok =
                        start == 0 || !text[..start].ends_with(|c: char| c.is_ascii_digit());
                    let after_ok = end >= text.len()
                        || !text[end..].starts_with(|c: char| c.is_ascii_digit());
                    if before_ok && after_ok {
                        spans.push((start, end, Kind::Pii));
                        break;
                    }
                }
            }
        }
        i += 1;
    }
}

fn shape_matches(candidate: &str, pat: &str) -> bool {
    candidate.len() == pat.len()
        && candidate.chars().zip(pat.chars()).all(|(c, p)| match p {
            'd' => c.is_ascii_digit(),
            sep => c == sep,
        })
}

/// Medical/financial content markers: redact the matched phrase. This is a
/// tripwire, not a classifier: surrounding prose still ships (scrubbed of
/// PII), so genuinely sensitive documents must not be exported at all.
fn sensitive_markers(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let markers = [
        "social security",
        "credit card",
        "bank account",
        "routing number",
        "medical record",
        "diagnosis",
        "patient",
        "iban",
    ];
    let lower = text.to_lowercase();
    for m in markers {
        let mut search = 0;
        while let Some(pos) = lower[search..].find(m).map(|p| search + p) {
            let before_ok = pos == 0 || !lower.as_bytes()[pos - 1].is_ascii_alphanumeric();
            let after = pos + m.len();
            let after_ok =
                after >= lower.len() || !lower.as_bytes()[after].is_ascii_alphanumeric();
            if before_ok && after_ok {
                spans.push((pos, after, Kind::Pii));
                search = after;
            } else {
                search = after;
            }
        }
    }
}

fn find_host_spans(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    hostname_values(text, spans);
    ssh_hosts(text, spans);
}

/// `HOSTNAME = value` / `hostname: value` (env dumps, logs).
fn hostname_values(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let lower = text.to_lowercase();
    let mut search = 0;
    while let Some(pos) = lower[search..].find("hostname").map(|p| search + p) {
        let before_ok = pos == 0
            || (!lower.as_bytes()[pos - 1].is_ascii_alphanumeric()
                && lower.as_bytes()[pos - 1] != b'_');
        let mut after = pos + 8;
        let bytes = lower.as_bytes();
        while after < bytes.len() && bytes[after].is_ascii_whitespace() {
            after += 1;
        }
        if before_ok && after < bytes.len() && (bytes[after] == b':' || bytes[after] == b'=') {
            after += 1;
            while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                after += 1;
            }
            let mut v = after;
            while v < text.len()
                && !matches!(
                    text.as_bytes()[v],
                    b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b',' | b';'
                )
            {
                v += 1;
            }
            if v > after {
                spans.push((after, v, Kind::Secret));
                search = v;
                continue;
            }
        }
        search = pos + 8;
    }
}

/// `ssh [user@]host`: redact the host (often an internal name/IP).
fn ssh_hosts(text: &str, spans: &mut Vec<(usize, usize, Kind)>) {
    let mut search = 0;
    while let Some(pos) = text[search..].find("ssh ").map(|p| search + p) {
        let before_ok =
            pos == 0 || matches!(text.as_bytes()[pos - 1], b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'');
        let mut s = pos + 4;
        // Skip flags (`-p 22`): stop at the first non-flag token.
        loop {
            if s < text.len() && text[s..].starts_with('-') {
                let flag_start = s;
                let mut e = s;
                while e < text.len() && !matches!(text.as_bytes()[e], b' ' | b'\t') {
                    e += 1;
                }
                s = e;
                while s < text.len() && matches!(text.as_bytes()[s], b' ' | b'\t') {
                    s += 1;
                }
                // `-p 22` takes a value: skip one more token.
                if text[flag_start..e].contains('p')
                    && s < text.len()
                    && text.as_bytes()[s].is_ascii_digit()
                {
                    while s < text.len() && !matches!(text.as_bytes()[s], b' ' | b'\t') {
                        s += 1;
                    }
                    while s < text.len() && matches!(text.as_bytes()[s], b' ' | b'\t') {
                        s += 1;
                    }
                }
                continue;
            }
            break;
        }
        let mut e = s;
        while e < text.len()
            && !matches!(
                text.as_bytes()[e],
                b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\''
            )
        {
            e += 1;
        }
        if before_ok && e > s {
            let tok = &text[s..e];
            if let Some(at) = tok.find('@') {
                spans.push((s + at + 1, e, Kind::Secret));
            } else {
                spans.push((s, e, Kind::Secret));
            }
            search = e;
        } else {
            search = pos + 4;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/home/alice";

    #[test]
    fn aws_key_and_token_prefixes_redact() {
        let text = "key AKIAIOSFODNN7EXAMPLE leaked and ghp_abcdefghijklmnopqrstuvwx rest";
        let o = scrub_text(text, HOME);
        assert!(o.scrubbed);
        assert_eq!(o.scrub_count, 2);
        assert!(!o.text.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!o.text.contains("ghp_abcdefghijklmnopqrstuvwx"));
        assert_eq!(o.text.matches(REDACTED_SECRET).count(), 2);
    }

    #[test]
    fn github_pat_and_anthropic_keys_redact() {
        let text = "a github_pat_abcDEF1234567890 b sk-ant-api03-xyzABCDEF123456 c";
        let o = scrub_text(text, HOME);
        assert_eq!(o.scrub_count, 2);
        assert!(!o.text.contains("github_pat_"));
        assert!(!o.text.contains("sk-ant-"));
    }

    #[test]
    fn private_key_block_redacts_through_end_marker() {
        let text =
            "leak:\n-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAKxyz\n-----END RSA PRIVATE KEY-----\ndone";
        let o = scrub_text(text, HOME);
        assert!(o.scrubbed);
        assert!(!o.text.contains("MIIBOgIBAAJBAKxyz"));
        assert!(!o.text.contains("PRIVATE KEY"));
        assert!(o.text.contains("done"));
    }

    #[test]
    fn password_and_bearer_values_redact_keep_keys() {
        let text = "db password = hunter2-hunter2 then Authorization: Bearer abcDEF1234567890 end";
        let o = scrub_text(text, HOME);
        assert!(!o.text.contains("hunter2"));
        assert!(!o.text.contains("abcDEF1234567890"));
        assert!(o.text.contains("password = "));
        assert!(o.text.contains("Bearer "));
    }

    #[test]
    fn url_password_redacts_only_password() {
        let text = "clone https://deploy:s3cr3t-pw@git.example.com/org/repo.git now";
        let o = scrub_text(text, HOME);
        assert!(!o.text.contains("s3cr3t-pw"));
        assert!(o.text.contains("https://deploy:"));
        assert!(o.text.contains("@git.example.com"));
    }

    #[test]
    fn email_phone_card_ssn_redact_as_pii() {
        let text = "mail bob.smith+tag@example.co.uk call +1 (555) 013-4455 card 4111-1111-1111-1111 ssn 078-05-1120";
        let o = scrub_text(text, HOME);
        assert!(!o.text.contains("bob.smith"));
        assert!(!o.text.contains("555"));
        assert!(!o.text.contains("4111"));
        assert!(!o.text.contains("078-05-1120"));
        assert!(o.text.contains(REDACTED_PII));
        assert!(!o.text.contains(REDACTED_SECRET));
    }

    #[test]
    fn hostname_and_ssh_hosts_redact() {
        let text = "HOSTNAME = build-fleet-07\nrun ssh deploy@internal-build-03 -- up";
        let o = scrub_text(text, HOME);
        assert!(!o.text.contains("build-fleet-07"));
        assert!(!o.text.contains("internal-build-03"));
        assert!(o.text.contains("deploy@"));
    }

    #[test]
    fn home_paths_rewrite_to_work_without_scrub_flag() {
        let text = "wrote /home/alice/work/out.txt and /home/bob/x.py";
        let o = scrub_text(text, HOME);
        assert!(!o.scrubbed);
        assert_eq!(o.scrub_count, 0);
        assert_eq!(o.text, "wrote $WORK/work/out.txt and $WORK/x.py");
    }

    #[test]
    fn clean_code_fixture_is_byte_identical() {
        let text = "diff --git a/crc-core/src/lib.rs b/crc-core/src/lib.rs\n\
            --- a/crc-core/src/lib.rs\n+++ b/crc-core/src/lib.rs\n\
            @@ -1,3 +1,5 @@\n+fn slice_by_8(crc: u32) -> u32 { crc }\n \
            commit 586940e7bde7cb9049a473e6c9a2e50c7dc57638\n\
            gain 1.75922973672599 wall 2026-10-26 version 1.2.3 phone-ish 12345\n\
            upstream https://github.com/Nicoretti/crc#4e65ac4 token-budget 150";
        let o = scrub_text(text, HOME);
        assert_eq!(o.text, text, "clean fixture must survive byte-identical");
        assert!(!o.scrubbed);
    }

    #[test]
    fn medical_marker_redacts() {
        let text = "note: patient follow-up, diagnosis pending; contact jane@example.com";
        let o = scrub_text(text, HOME);
        assert!(!o.text.contains("patient"));
        assert!(!o.text.contains("diagnosis"));
        assert!(!o.text.contains("jane@example.com"));
    }

    #[test]
    fn gutted_fraction_reflects_coverage() {
        let o = scrub_text("ghp_abcdefghijklmnopqrstuvwx", HOME);
        assert!(o.redacted_fraction() > 0.5);
        let clean = scrub_text("fn main() {}", HOME);
        assert_eq!(clean.redacted_fraction(), 0.0);
    }

    #[test]
    fn empty_input_stays_empty() {
        let o = scrub_text("", HOME);
        assert_eq!(o.text, "");
        assert!(!o.scrubbed);
        assert_eq!(o.redacted_fraction(), 0.0);
    }

    #[test]
    fn multibyte_text_survives_home_scan() {
        // Regression: the home-dir scan once pushed raw bytes, panicking on 📦.
        let text = "bench 📦 /home/alice/x done";
        let o = scrub_text(text, HOME);
        assert_eq!(o.text, "bench 📦 $WORK/x done");
        assert!(!o.scrubbed);
    }
}
