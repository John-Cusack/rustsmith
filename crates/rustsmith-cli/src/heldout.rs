//! Host-only held-out suite + generated pins (Track H).
//!
//! Static suites, probe scripts, and pin templates come from package-keyed
//! data (`data/repo-content.json`, looked up by the package name discovered
//! from the repo itself). Grading runs through `TestRunner::heldout` +
//! `runner.grade`; probes run as `TestCommand`s with `cwd` doing module
//! resolution — no toolchain literals live here. Never copied into
//! run/grading containers; never shown to workers/council.

use rustsmith_adapters::PytestRunner;
use rustsmith_core::{Cwd, TestCommand};
use rustsmith_oracle::execute_all;

/// Static held-out suites for `package`: (filename, content) from data.
pub fn generate_suites_for(package: &str) -> Result<Vec<(String, String)>, String> {
    let entry = crate::repo_content::entry(package)?;
    let suites = entry["heldout_suites"]
        .as_array()
        .ok_or_else(|| format!("no heldout suites for package '{package}'"))?;
    suites
        .iter()
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("").to_string();
            let content = s["content"].as_str().unwrap_or("").to_string();
            if name.is_empty() || content.is_empty() {
                return Err(format!("bad heldout suite entry for package '{package}'"));
            }
            Ok((name, content))
        })
        .collect()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Deterministic xorshift64 stream (std-only; stable across runs/hosts).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0 | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() & 0xFF) as u8).collect()
    }
}

/// Disjoint byte-input classes for checksum-style pins: empty / singleton /
/// max / unicode-or-high-bytes / shifted sizes / seeded random. `avoid_hex`
/// (hex of visible vectors, from data) patches accidental collisions.
/// Catching classes have len > 1 so a visible-only hardcode fails them.
fn bytes_inputs(seed: u64, avoid_hex: &[String]) -> Vec<Vec<u8>> {
    let mut rng = Rng(seed);
    let mut datas: Vec<Vec<u8>> = vec![
        vec![],
        vec![(rng.next() & 0xFF) as u8],
        rng.bytes(8192),
        ["上海市".as_bytes(), &rng.bytes(16)].concat(),
        rng.bytes(511),
        rng.bytes(4097),
        rng.bytes(63),
        {
            let n = 16 + (rng.next() % 240) as usize;
            rng.bytes(n)
        },
        {
            let n = 16 + (rng.next() % 240) as usize;
            rng.bytes(n)
        },
    ];
    for d in datas.iter_mut().skip(2) {
        if avoid_hex.iter().any(|h| *h == hex::encode(&*d)) {
            d.push(0x7E);
        }
    }
    datas
}

/// Disjoint string-pair classes for similarity-style pins: empty / singleton /
/// max / unicode / shifted sizes / seeded.
fn pairs_inputs(seed: u64) -> Vec<(String, String)> {
    let mut rng = Rng(seed);
    let rstr = |r: &mut Rng, n: usize| -> String {
        (0..n).map(|_| (b'a' + (r.next() % 26) as u8) as char).collect()
    };
    vec![
        ("".into(), "a".into()),
        ("a".into(), "".into()),
        ("a".into(), "a".into()),
        (rstr(&mut rng, 512), rstr(&mut rng, 512)),
        (
            "上海市".to_string() + &rstr(&mut rng, 8),
            "上海".to_string() + &rstr(&mut rng, 8),
        ),
        (rstr(&mut rng, 511), rstr(&mut rng, 509)),
        (rstr(&mut rng, 63), rstr(&mut rng, 65)),
        (rstr(&mut rng, 24), rstr(&mut rng, 24)),
        (rstr(&mut rng, 130), rstr(&mut rng, 128)),
    ]
}

/// Render checksum-style pins: one template line per datum, `{LIT}` the input
/// literal and `{V0..V2}` the pinned values in `{:#X}`.
fn render_bytes3(header: &str, line: &str, datas_hex: &[String], vals: &[[u64; 3]]) -> String {
    let mut body = String::from(header);
    for (h, v) in datas_hex.iter().zip(vals.iter()) {
        let bytes = hex::decode(h).unwrap_or_default();
        let lit = format!(
            "bytes([{}])",
            bytes
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        body.push_str(
            &line
                .replace("{LIT}", &lit)
                .replace("{V0}", &format!("{:#X}", v[0]))
                .replace("{V1}", &format!("{:#X}", v[1]))
                .replace("{V2}", &format!("{:#X}", v[2])),
        );
    }
    body
}
/// JSON value as a Python literal (`None`/`True`/`False`, never `null`).
/// JSON string quoting is valid Python; numbers transfer verbatim.
fn json_to_py(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "None".to_string(),
        serde_json::Value::Bool(true) => "True".to_string(),
        serde_json::Value::Bool(false) => "False".to_string(),
        other => other.to_string(),
    }
}

/// Render bytes-input pins with generic JSON values: `{LIT}` the input
/// literal and `{V0..V2}` the pinned JSON values. Detector-style repos (bytes
/// in, encoding-string/float out) need this; checksum repos use `render_bytes3`.
fn render_bytes_json3(
    header: &str,
    line: &str,
    datas_hex: &[String],
    vals: &[[serde_json::Value; 3]],
) -> String {
    let mut body = String::from(header);
    for (h, v) in datas_hex.iter().zip(vals.iter()) {
        let bytes = hex::decode(h).unwrap_or_default();
        let lit = format!(
            "bytes([{}])",
            bytes
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        body.push_str(
            &line
                .replace("{LIT}", &lit)
                .replace("{V0}", &json_to_py(&v[0]))
                .replace("{V1}", &json_to_py(&v[1]))
                .replace("{V2}", &json_to_py(&v[2])),
        );
    }
    body
}

/// Render similarity-style pins: `{A}`/`{B}` the debug-formatted pair,
/// `{V0..V2}` the pinned JSON values.
fn render_pairs3(
    header: &str,
    line: &str,
    pairs: &[(String, String)],
    vals: &[[serde_json::Value; 3]],
) -> String {
    let mut body = String::from(header);
    for ((a, b), v) in pairs.iter().zip(vals.iter()) {
        body.push_str(
            &line
                .replace("{A}", &format!("{a:?}"))
                .replace("{B}", &format!("{b:?}"))
                .replace("{V0}", &json_to_py(&v[0]))
                .replace("{V1}", &json_to_py(&v[1]))
                .replace("{V2}", &json_to_py(&v[2])),
        );
    }
    body
}

/// Run a pin probe as a `TestCommand` on the host: the interpreter program
/// comes from the runner, `cwd` resolves the package (`src/` for installed
/// layouts, else the tree) so no path override literal appears.
fn run_probe(
    package: &str,
    orig: &std::path::Path,
    script: &str,
    args: &[String],
) -> Result<String, String> {
    let cwd = if crate::repo::is_src_layout(orig, package) {
        Cwd::Rel("src".to_string())
    } else {
        Cwd::Tree
    };
    let mut cmd_args = vec!["-c".to_string(), script.to_string()];
    cmd_args.extend(args.iter().cloned());
    let cmd = TestCommand {
        program: PytestRunner::python_program(),
        args: cmd_args,
        cwd,
        env_set: Vec::new(),
        env_remove: Vec::new(),
        launcher: None,
        timeout_secs: None,
        collect: Vec::new(),
    };
    let runs = execute_all(orig, orig, &[cmd]).map_err(|e| e.to_string())?;
    let out = runs.first().ok_or("probe produced no output")?;
    if out.exit_code != 0 {
        return Err(format!(
            "heldout-gen probe failed: {}",
            crate::mirror::output_value(&format!("{}\n{}", out.stdout, out.stderr))
        ));
    }
    Ok(crate::mirror::output_value(&out.stdout))
}

/// Control-plane GENERATED held-outs, seeded deterministically from the
/// frozen manifest (FNV-1a over the manifest JSON). `package` selects the
/// probe script + templates from data; expected values are pinned against the
/// PRISTINE ORIGINAL on the host at recon time (never in a container, never
/// shown to any worker/council seat).
pub fn generate_from_manifest(
    manifest_json: &str,
    package: &str,
    orig_repo: &std::path::Path,
) -> Result<Vec<(String, String)>, String> {
    let entry = crate::repo_content::entry(package)?;
    let gen = &entry["heldout_gen"];
    let header = gen["header"].as_str().unwrap_or("");
    let line = gen["line"].as_str().unwrap_or("");
    let probe = gen["probe"].as_str().unwrap_or("");
    if header.is_empty() || line.is_empty() || probe.is_empty() {
        return Err(format!("no heldout generator for package '{package}'"));
    }
    let seed = fnv1a64(manifest_json.as_bytes());
    let body = match gen["kind"].as_str().unwrap_or("") {
        "bytes3" => {
            let avoid: Vec<String> = gen["avoid_hex"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            let datas = bytes_inputs(seed, &avoid);
            let args: Vec<String> = datas.iter().map(hex::encode).collect();
            let text = run_probe(package, orig_repo, probe, &args)?;
            let vals: Vec<[u64; 3]> =
                serde_json::from_str(&text).map_err(|e| format!("heldout-gen parse: {e}"))?;
            if vals.len() != datas.len() {
                return Err(format!(
                    "heldout-gen count {} != {}",
                    vals.len(),
                    datas.len()
                ));
            }
            render_bytes3(header, line, &args, &vals)
        }
        "bytes_json3" => {
            let avoid: Vec<String> = gen["avoid_hex"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect();
            let datas = bytes_inputs(seed, &avoid);
            let args: Vec<String> = datas.iter().map(hex::encode).collect();
            let text = run_probe(package, orig_repo, probe, &args)?;
            let vals: Vec<[serde_json::Value; 3]> =
                serde_json::from_str(&text).map_err(|e| format!("heldout-gen parse: {e}"))?;
            if vals.len() != datas.len() {
                return Err(format!(
                    "heldout-gen count {} != {}",
                    vals.len(),
                    datas.len()
                ));
            }
            render_bytes_json3(header, line, &args, &vals)
        }
        "pairs3" => {
            let pairs = pairs_inputs(seed);
            let args: Vec<String> = pairs
                .iter()
                .flat_map(|(a, b)| [hex::encode(a.as_bytes()), hex::encode(b.as_bytes())])
                .collect();
            let text = run_probe(package, orig_repo, probe, &args)?;
            let vals: Vec<[serde_json::Value; 3]> =
                serde_json::from_str(&text).map_err(|e| format!("heldout-gen parse: {e}"))?;
            if vals.len() != pairs.len() {
                return Err(format!(
                    "heldout-gen count {} != {}",
                    vals.len(),
                    pairs.len()
                ));
            }
            render_pairs3(header, line, &pairs, &vals)
        }
        other => {
            return Err(format!(
                "unknown heldout generator kind '{other}' for package '{package}'"
            ));
        }
    };
    Ok(vec![("test_heldout_generated.py".into(), body)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Data templates reproduce the recorded samples for every package in the
    /// table (no package is named: the table keys drive both sides).
    #[test]
    fn data_templates_reproduce_samples() {
        let samples: serde_json::Value = serde_json::from_str(include_str!(
            "../data/gen-samples.json"
        ))
        .unwrap();
        let map = samples.as_object().unwrap();
        assert!(!map.is_empty(), "samples table is empty");
        for (package, sample) in map {
            let entry = crate::repo_content::entry(package).unwrap();
            let gen = &entry["heldout_gen"];
            let header = gen["header"].as_str().unwrap();
            let line = gen["line"].as_str().unwrap();
            let body = match gen["kind"].as_str().unwrap() {
                "bytes3" => {
                    let datas_hex: Vec<String> =
                        serde_json::from_value(sample["datas_hex"].clone()).unwrap();
                    let vals: Vec<[u64; 3]> =
                        serde_json::from_value(sample["vals"].clone()).unwrap();
                    render_bytes3(header, line, &datas_hex, &vals)
                }
                "bytes_json3" => {
                    let datas_hex: Vec<String> =
                        serde_json::from_value(sample["datas_hex"].clone()).unwrap();
                    let vals: Vec<[serde_json::Value; 3]> =
                        serde_json::from_value(sample["vals"].clone()).unwrap();
                    render_bytes_json3(header, line, &datas_hex, &vals)
                }
                "pairs3" => {
                    let pairs: Vec<(String, String)> =
                        serde_json::from_value(sample["pairs"].clone()).unwrap();
                    let vals: Vec<[serde_json::Value; 3]> =
                        serde_json::from_value(sample["vals"].clone()).unwrap();
                    render_pairs3(header, line, &pairs, &vals)
                }
                other => panic!("unknown generator kind '{other}'"),
            };
            assert_eq!(&body, sample["body"].as_str().unwrap(), "template mismatch for {package}");
        }
    }

    /// Static suites stay non-empty and python-shaped for every package that
    /// declares them. Suites are a Python-spine artifact: CMake packages
    /// carry no `heldout_suites` key and recon degrades to none.
    #[test]
    fn suites_nonempty_for_all_packages() {
        let mut covered = 0;
        for package in crate::repo_content::packages() {
            let entry = crate::repo_content::entry(&package).unwrap();
            if !entry["heldout_suites"].is_array() {
                continue;
            }
            covered += 1;
            let suites = generate_suites_for(&package).unwrap();
            assert!(!suites.is_empty(), "no suites for {package}");
            for (name, content) in &suites {
                assert!(name.ends_with(".py"), "suite name {name}");
                assert!(!content.is_empty(), "empty suite {name}");
            }
        }
        assert!(covered > 0, "no package declares heldout suites");
    }
    #[test]
    fn json_to_py_emits_python_literals() {
        assert_eq!(json_to_py(&serde_json::Value::Null), "None");
        assert_eq!(json_to_py(&serde_json::json!(true)), "True");
        assert_eq!(json_to_py(&serde_json::json!(false)), "False");
        assert_eq!(json_to_py(&serde_json::json!("utf_8")), "\"utf_8\"");
        assert_eq!(json_to_py(&serde_json::json!(0.5)), "0.5");
    }
}
