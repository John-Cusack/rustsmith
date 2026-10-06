//! `create-github-repo`: human-triggered publishing-repo creation.
//!
//! A run's `--fork` dir is a local git repo with no remote; the publishing
//! repo named in `mirror/<pkg>/release.toml` (`github_repo`) is created by
//! hand. This subcommand replaces that manual step. It is NEVER called
//! during a run: it only runs when a human invokes it explicitly.
//!
//! Flow (all refusals exit non-zero before any side effect):
//! 1. Load `release.toml`, derive the expected repo name `<project>-rust`.
//! 2. Read the upstream license from the recon license record
//!    (`--recon-out/recon.json` + `facts.json`), never guessing: missing or
//!    ambiguous records are refused rather than defaulted to MIT.
//! 3. Verify the fork (branch `main` exists, tree clean, LICENSE + NOTICE
//!    with upstream attribution) and refuse any upstream package remote.
//! 4. Ask `gh` who is authenticated and whether the target repo exists:
//!    foreign owners and existing non-empty repos are refused.
//! 5. Print the plan (repo, visibility, license, commit). Without `--yes`
//!    this is a dry run: nothing is written or created.
//! 6. With `--yes`: fix `release.toml` when the name differs, `gh repo
//!    create` (public by default, `--private` opts out), add the remote,
//!    record the remote in the run store + events, push `main`.
//!
//! No credentials travel in argv or committed files: `gh` owns
//! authentication (env/keyring), and the push reuses it through an ephemeral
//! `-c credential.helper` override that is never written anywhere.

use std::path::{Path, PathBuf};

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}
fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name || a.starts_with(&format!("{name}=")))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Run `prog args` in `dir`, returning trimmed stdout. Errors carry the
/// program, args, and a tail of combined output (never credentials: callers
/// must not pass any).
fn run_prog(dir: &Path, prog: &str, args: &[&str], extra_env: &[(&str, &str)]) -> Result<String, String> {
    let mut cmd = std::process::Command::new(prog);
    cmd.current_dir(dir).args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    match cmd.output() {
        Err(e) => Err(format!("spawn {prog}: {e}")),
        Ok(o) => {
            let mut log = String::from_utf8_lossy(&o.stdout).into_owned();
            log.push_str(&String::from_utf8_lossy(&o.stderr));
            if o.status.success() {
                Ok(log.trim().to_string())
            } else {
                let tail: Vec<&str> = log.trim().lines().collect();
                let tail = tail[tail.len().saturating_sub(8)..].join("\n");
                Err(format!("{} {} failed:\n{tail}", prog, args.join(" ")))
            }
        }
    }
}

/// `owner/repo` split with shape validation (GitHub login/repo rules,
/// ASCII only). Anything else is refused, never normalized into shape.
pub(crate) fn split_github_repo(s: &str) -> Result<(String, String), String> {
    let (owner, repo) = s.split_once('/').ok_or_else(|| format!("github_repo {s:?} must be `owner/repo`"))?;
    if owner.is_empty() || repo.is_empty() || s.contains(' ') || !s.is_ascii() {
        return Err(format!("github_repo {s:?} must be `owner/repo`"));
    }
    if s.matches('/').count() != 1 {
        return Err(format!("github_repo {s:?} must be `owner/repo`"));
    }
    for part in [owner, repo] {
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(format!("github_repo {s:?}: {part:?} has illegal characters"));
        }
        if part.starts_with(['-', '.']) || part.ends_with('.') {
            return Err(format!("github_repo {s:?}: {part:?} has illegal edges"));
        }
    }
    Ok((owner.to_string(), repo.to_string()))
}

/// `https://host/owner/repo(.git)` / `host:owner/repo(.git)` -> `owner/repo`.
pub(crate) fn shorten_remote(url: &str) -> Option<String> {
    let u = url.strip_suffix(".git").unwrap_or(url);
    for sep in ["github.com/", "github.com:"] {
        if let Some(i) = u.find(sep) {
            let tail = &u[i + sep.len()..];
            let mut it = tail.split('/');
            let (o, r) = (it.next()?, it.next()?);
            if it.next().is_none() && !o.is_empty() && !r.is_empty() {
                return Some(format!("{o}/{r}"));
            }
        }
    }
    None
}

/// Upstream license from the recon license record: `recon.json`
/// `license.license` plus every `facts.json` `probe.attribution[].license`.
/// Missing records, `unknown`/empty ids, and disagreements are all refused
/// (never guessed, never defaulted to MIT).
pub(crate) fn read_recon_license(recon_out: &Path) -> Result<String, String> {
    let mut ids: Vec<String> = Vec::new();
    let recon_path = recon_out.join("recon.json");
    if recon_path.is_file() {
        let text = std::fs::read_to_string(&recon_path).map_err(|e| format!("{}: {e}", recon_path.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: bad json: {e}", recon_path.display()))?;
        if let Some(id) = v.pointer("/license/license").and_then(|x| x.as_str()) {
            if !id.trim().is_empty() {
                ids.push(id.trim().to_string());
            }
        }
    }
    let facts_path = recon_out.join("facts.json");
    if facts_path.is_file() {
        let text = std::fs::read_to_string(&facts_path).map_err(|e| format!("{}: {e}", facts_path.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: bad json: {e}", facts_path.display()))?;
        if let Some(arr) = v.pointer("/probe/attribution").and_then(|x| x.as_array()) {
            for a in arr {
                if let Some(id) = a.get("license").and_then(|x| x.as_str()) {
                    if !id.trim().is_empty() {
                        ids.push(id.trim().to_string());
                    }
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    match ids.len() {
        0 => Err(format!(
            "{}: no recon license record (recon.json `license.license` + facts.json `probe.attribution[].license` both missing); refusing to guess",
            recon_out.display()
        )),
        1 => {
            let id = &ids[0];
            if id.eq_ignore_ascii_case("unknown") {
                return Err(format!(
                    "{}: recon license record is `unknown`; determine the upstream license and re-run recon instead of defaulting to MIT",
                    recon_out.display()
                ));
            }
            Ok(id.clone())
        }
        _ => Err(format!(
            "{}: ambiguous recon license record ({}) — refusing to pick one",
            recon_out.display(),
            ids.join(", ")
        )),
    }
}

/// Copyleft family whose terms survive into the port (GPL/LGPL/AGPL).
pub(crate) fn is_copyleft(license: &str) -> bool {
    let u = license.to_uppercase();
    u.contains("GPL") || u.contains("AFFERO")
}

/// Effective publish license: copyleft upstream terms stay under those terms
/// (relicensing GPL as MIT is refused); otherwise `release.toml`
/// `license_spdx` must be a known identifier.
pub(crate) fn effective_license(recon_license: &str, license_spdx: &str) -> Result<String, String> {
    if is_copyleft(recon_license) {
        if license_spdx == recon_license {
            return Ok(recon_license.to_string());
        }
        return Err(format!(
            "upstream license is copyleft ({recon_license}): release.toml `license_spdx` {license_spdx:?} would relicense it — keep upstream terms ({recon_license:?}) with the upstream LICENSE text"
        ));
    }
    if rustsmith_release::KNOWN_SPDX.contains(&license_spdx) {
        return Ok(license_spdx.to_string());
    }
    Err(format!(
        "release.toml `license_spdx` {license_spdx:?} unknown (known: {})",
        rustsmith_release::KNOWN_SPDX.join(", ")
    ))
}

/// Author display names from `upstream_authors` (`"Name <mail>, Name <mail>"`).
pub(crate) fn author_names(authors: &str) -> Vec<String> {
    authors
        .split(',')
        .filter_map(|p| {
            let name = p.split('<').next().unwrap_or("").trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

const LICENSE_NAMES: &[&str] =
    &["LICENSE", "LICENSE.txt", "LICENSE.md", "LICENCE", "LICENCE.txt", "COPYING", "COPYING.txt"];

pub(crate) fn fork_license_text(fork: &Path) -> Result<(String, String), String> {
    for name in LICENSE_NAMES {
        let p = fork.join(name);
        if p.is_file() {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            if text.trim().is_empty() {
                return Err(format!("{}: license file is empty", p.display()));
            }
            return Ok((name.to_string(), text));
        }
    }
    Err(format!(
        "{}: no license file ({}); the pushed repo must carry LICENSE",
        fork.display(),
        LICENSE_NAMES.join("/")
    ))
}

/// Copyleft ports must retain the upstream LICENSE text, not just the tag.
pub(crate) fn check_copyleft_text(recon_license: &str, text: &str) -> Result<(), String> {
    let upper = text.to_uppercase();
    let want = recon_license.to_uppercase();
    if upper.contains(&want) {
        return Ok(());
    }
    let u = want.as_str();
    if u.contains("AFFERO") && upper.contains("AFFERO") {
        return Ok(());
    }
    if (u.contains("GPL")) && upper.contains("GENERAL PUBLIC LICENSE") {
        return Ok(());
    }
    Err(format!(
        "fork LICENSE lacks the upstream {recon_license} terms (retaining the upstream LICENSE text is required)"
    ))
}

pub(crate) fn check_notice(
    fork: &Path,
    upstream: &str,
    upstream_url: &str,
    upstream_authors: &str,
    license: &str,
    upstream_license: &str,
) -> Result<(), String> {
    let p = fork.join("NOTICE");
    let notice = std::fs::read_to_string(&p)
        .map_err(|_| format!("{}: missing NOTICE (must name {upstream} {upstream_url})", fork.display()))?;
    if !notice.contains(upstream) {
        return Err(format!("{}: NOTICE lacks upstream attribution {upstream:?}", p.display()));
    }
    if !notice.contains(upstream_url) {
        return Err(format!("{}: NOTICE lacks upstream URL {upstream_url:?}", p.display()));
    }
    if !notice.contains(license) && !notice.contains(upstream_license) {
        return Err(format!("{}: NOTICE lacks the license tag ({license} / {upstream_license})", p.display()));
    }
    let names = author_names(upstream_authors);
    if names.is_empty() {
        return Err(format!(
            "{}: release.toml `upstream_authors` is empty (NOTICE must name the upstream authors)",
            p.display()
        ));
    }
    for n in &names {
        if !notice.contains(n) {
            return Err(format!("{}: NOTICE lacks upstream author {n:?}", p.display()));
        }
    }
    Ok(())
}

pub(crate) fn fork_remotes(fork: &Path) -> Result<Vec<(String, String)>, String> {
    let out = run_prog(fork, "git", &["remote", "-v"], &[]).map_err(|e| format!("{}: {e}", fork.display()))?;
    let mut remotes = Vec::new();
    for line in out.lines() {
        let mut it = line.split_whitespace();
        if let (Some(name), Some(url)) = (it.next(), it.next()) {
            if it.next() == Some("(fetch)") && !remotes.iter().any(|(n, u)| n == name && u == url) {
                remotes.push((name.to_string(), url.to_string()));
            }
        }
    }
    Ok(remotes)
}

/// Refuse a fork remote pointing at the upstream package (the target repo
/// itself is exempt: pushing there is the point).
pub(crate) fn check_no_upstream_remote(
    remotes: &[(String, String)],
    upstream: &str,
    upstream_url: &str,
    target: &str,
) -> Result<(), String> {
    let upstream_slug = upstream.to_lowercase();
    let target_slug = target.to_lowercase();
    for (name, url) in remotes {
        let slug = shorten_remote(url).map(|s| s.to_lowercase());
        if slug.as_deref() == Some(upstream_slug.as_str()) && slug.as_deref() != Some(target_slug.as_str()) {
            return Err(format!(
                "fork remote {name:?} points at the upstream package ({url}); pushing there is refused"
            ));
        }
        if url.trim_end_matches(".git") == upstream_url.trim_end_matches(".git") {
            return Err(format!(
                "fork remote {name:?} points at the upstream URL ({url}); pushing there is refused"
            ));
        }
    }
    Ok(())
}

/// Remote name already pointing at the target repo, if any.
pub(crate) fn remote_for_target(remotes: &[(String, String)], target: &str) -> Option<String> {
    let want = target.to_lowercase();
    remotes.iter().find_map(|(name, url)| {
        if shorten_remote(url).map(|s| s.to_lowercase()).as_deref() == Some(want.as_str()) {
            Some(name.clone())
        } else {
            None
        }
    })
}

/// Hosting operations (real: `gh` + `git`; fake in tests). No method takes
/// credentials: `gh` authenticates from its own config/environment.
pub(crate) trait Host {
    fn current_user(&self) -> Result<String, String>;
    /// Existing repo size in KiB, or `None` when the repo does not exist.
    fn repo_size(&self, owner: &str, repo: &str) -> Result<Option<i64>, String>;
    fn create_repo(&self, owner: &str, repo: &str, private: bool) -> Result<(), String>;
    fn push_main(&self, fork: &Path, remote: &str) -> Result<(), String>;
}

pub(crate) struct GhHost;

fn parse_json_string(out: &str) -> Result<String, String> {
    if let Ok(s) = serde_json::from_str::<String>(out.trim()) {
        return Ok(s);
    }
    let t = out.trim().trim_matches('"').trim().to_string();
    if t.is_empty() {
        return Err("empty response".to_string());
    }
    Ok(t)
}

impl Host for GhHost {
    fn current_user(&self) -> Result<String, String> {
        let out = run_prog(Path::new("."), "gh", &["api", "user", "--jq", ".login"], &[])
            .map_err(|e| format!("gh auth failed (is `gh auth login` done?): {e}"))?;
        parse_json_string(&out).map_err(|e| format!("gh api user: {e}"))
    }
    fn repo_size(&self, owner: &str, repo: &str) -> Result<Option<i64>, String> {
        let path = format!("repos/{owner}/{repo}");
        match run_prog(Path::new("."), "gh", &["api", &path, "--jq", ".size"], &[]) {
            Ok(out) => out
                .trim()
                .parse::<i64>()
                .map(Some)
                .map_err(|e| format!("gh api {path}: bad .size: {e}")),
            Err(e) if e.contains("404") => Ok(None),
            Err(e) => Err(format!("gh api {path}: {e}")),
        }
    }
    fn create_repo(&self, owner: &str, repo: &str, private: bool) -> Result<(), String> {
        let vis = if private { "--private" } else { "--public" };
        run_prog(Path::new("."), "gh", &["repo", "create", &format!("{owner}/{repo}"), vis], &[])
            .map(|_| ())
            .map_err(|e| format!("gh repo create {owner}/{repo}: {e}"))
    }
    fn push_main(&self, fork: &Path, remote: &str) -> Result<(), String> {
        // Ephemeral credential helper: gh supplies the token per-invocation,
        // nothing is written to git config or passed on the command line.
        // GIT_TERMINAL_PROMPT=0 fails fast instead of hanging on a prompt.
        run_prog(
            fork,
            "git",
            &["-c", "credential.helper=!gh auth git-credential", "push", remote, "main"],
            &[("GIT_TERMINAL_PROMPT", "0")],
        )
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", fork.display()))
    }
}

pub(crate) struct Plan {
    pub owner: String,
    pub repo: String,
    pub full: String,
    pub remote: String,
    pub visibility: String,
    pub license: String,
    pub recon_license: String,
    pub sha: String,
    pub subject: String,
    pub github_repo_was: String,
    pub needs_release_toml_fix: bool,
    pub repo_existed: bool,
}

pub(crate) fn plan(
    host: &dyn Host,
    project: &Path,
    fork: &Path,
    recon_out: &Path,
    private: bool,
) -> Result<Plan, String> {
    if !project.is_dir() {
        return Err(format!("{}: no such project dir", project.display()));
    }
    if !fork.is_dir() {
        return Err(format!("{}: no such --fork dir", fork.display()));
    }
    if !recon_out.is_dir() {
        return Err(format!("{}: no such --recon-out dir", recon_out.display()));
    }
    let cfg = rustsmith_release::load_release_config(project).map_err(|e| e.to_string())?;
    let (owner, repo) = split_github_repo(&cfg.github_repo)?;
    // Canonical name `<project>-rust` (captain-confirmed); the file is fixed
    // on the `--yes` path, never silently during planning.
    let want = format!("{}-rust", cfg.project);
    let needs_fix = repo != want;
    let repo = if needs_fix { want } else { repo };
    let full = format!("{owner}/{repo}");
    // The captain's account owns the target: `gh` says who is authenticated.
    let login = host.current_user()?;
    if owner.to_lowercase() != login.to_lowercase() {
        return Err(format!(
            "github_repo owner {owner:?} != authenticated gh user {login:?} (refusing an owner the captain does not control)"
        ));
    }
    // License from the recon record, never guessed.
    let recon_license = read_recon_license(recon_out)?;
    let license = effective_license(&recon_license, &cfg.license_spdx)?;
    // Fork: branch `main`, clean tree, license + attribution.
    let sha = run_prog(fork, "git", &["rev-parse", "main"], &[])
        .map(|s| s.trim().to_string())
        .map_err(|_| format!("{}: no `main` branch (pushing the fork's main needs one)", fork.display()))?;
    if sha.is_empty() {
        return Err(format!("{}: no `main` branch", fork.display()));
    }
    let status = run_prog(fork, "git", &["status", "--porcelain"], &[]).map_err(|e| format!("{}: {e}", fork.display()))?;
    if !status.trim().is_empty() {
        return Err(format!("{}: dirty tree (commit or stash first; only the pushed commit is previewed)", fork.display()));
    }
    let subject = run_prog(fork, "git", &["log", "-1", "--format=%s", "main"], &[])
        .map_err(|e| format!("{}: {e}", fork.display()))?;
    let (_, license_text) = fork_license_text(fork)?;
    if is_copyleft(&recon_license) {
        check_copyleft_text(&recon_license, &license_text)?;
    }
    check_notice(fork, &cfg.upstream, &cfg.upstream_url, &cfg.upstream_authors, &license, &cfg.upstream_license)?;
    let remotes = fork_remotes(fork)?;
    check_no_upstream_remote(&remotes, &cfg.upstream, &cfg.upstream_url, &full)?;
    // Existing repos: empty ones are ours to fill (or retry); non-empty is
    // refused — pushing onto someone's history is never automatic.
    let repo_existed = match host.repo_size(&owner, &repo)? {
        None => false,
        Some(size) if size <= 0 => true,
        Some(size) => {
            return Err(format!(
                "github.com/{full} already exists and is non-empty (size {size} KiB); refusing to push onto it"
            ));
        }
    };
    Ok(Plan {
        owner,
        repo,
        full: full.clone(),
        remote: format!("https://github.com/{full}.git"),
        visibility: if private { "private".into() } else { "public".into() },
        license,
        recon_license,
        sha,
        subject,
        github_repo_was: cfg.github_repo.clone(),
        needs_release_toml_fix: needs_fix,
        repo_existed,
    })
}

pub(crate) fn preview(p: &Plan) -> String {
    let mut s = String::from("github repo plan (dry run: re-run with --yes to create and push)\n");
    s.push_str(&format!("  repo:       {}\n", p.full));
    s.push_str(&format!("  visibility: {}\n", p.visibility));
    s.push_str(&format!("  license:    {} (recon: {})\n", p.license, p.recon_license));
    s.push_str(&format!("  commit:     {} {}\n", p.sha, p.subject));
    s.push_str(&format!("  remote:     {}\n", p.remote));
    if p.needs_release_toml_fix {
        s.push_str(&format!(
            "  release.toml: will update github_repo {:?} -> {:?}\n",
            p.github_repo_was, p.full
        ));
    } else {
        s.push_str("  release.toml: already matches\n");
    }
    if p.repo_existed {
        s.push_str("  existing repo: exists and is empty (will push)\n");
    } else {
        s.push_str("  existing repo: not found (will create)\n");
    }
    s
}

/// Line-scoped `github_repo` fix preserving comments and formatting.
pub(crate) fn fix_release_toml(path: &Path, old: &str, new: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut done = false;
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if !done && t.starts_with("github_repo") {
            let quote = t.contains('"').then_some('"').or_else(|| t.contains('\'').then_some('\''));
            if let Some(q) = quote {
                let cur = t.split(q).nth(1).unwrap_or("");
                if cur == old {
                    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                    out.push_str(&format!("{indent}github_repo = {q}{new}{q}\n"));
                    done = true;
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    if !done {
        return Err(format!("{}: github_repo line for {old:?} not found (not rewriting the file)", path.display()));
    }
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    std::fs::write(path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

pub(crate) fn cmd_with_host(host: &dyn Host, args: &[String]) -> Result<(), String> {
    let fork = PathBuf::from(flag(args, "--fork").ok_or("missing --fork")?);
    let project = PathBuf::from(flag(args, "--project").ok_or("missing --project")?);
    let recon_out = PathBuf::from(flag(args, "--recon-out").ok_or("missing --recon-out")?);
    let run_id = flag(args, "--run-id").ok_or("missing --run-id")?;
    let store_path = PathBuf::from(flag(args, "--store").unwrap_or_else(|| "store.db".into()));
    let private = has_flag(args, "--private");
    let yes = has_flag(args, "--yes");
    // The run must exist before anything else: the remote is recorded
    // against it, and a typo'd id must fail in preview, not after a push.
    if !store_path.is_file() {
        return Err(format!("{}: no run store (record needs --store pointing at the run's store.db)", store_path.display()));
    }
    let store = rustsmith_store::Store::open(&store_path).map_err(|e| e.to_string())?;
    if store.get_run(&run_id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("unknown --run-id {run_id:?} in {}", store_path.display()));
    }
    let p = plan(host, &project, &fork, &recon_out, private)?;
    if !yes {
        print!("{}", preview(&p));
        return Ok(());
    }
    if p.needs_release_toml_fix {
        fix_release_toml(&project.join("release.toml"), &p.github_repo_was, &p.full)?;
        let fork_toml = fork.join("release.toml");
        if fork_toml.is_file() {
            if let Ok(t) = std::fs::read_to_string(&fork_toml) {
                if t.contains(&p.github_repo_was) {
                    eprintln!(
                        "warning: {} still names {:?} (project release.toml fixed; re-run mirror so the pushed tree agrees)",
                        fork_toml.display(),
                        p.github_repo_was
                    );
                }
            }
        }
    }
    if !p.repo_existed {
        host.create_repo(&p.owner, &p.repo, private)?;
    }
    let remotes = fork_remotes(&fork)?;
    let remote_name = match remote_for_target(&remotes, &p.full) {
        Some(n) => n,
        None => {
            if remotes.iter().any(|(n, _)| n == "origin") {
                return Err(format!(
                    "{}: remote `origin` already points elsewhere (point it at {} or remove it)",
                    fork.display(),
                    p.remote
                ));
            }
            run_prog(&fork, "git", &["remote", "add", "origin", &p.remote], &[])
                .map_err(|e| format!("{}: {e}", fork.display()))?;
            "origin".to_string()
        }
    };
    store
        .record_github_repo(&run_id, &p.full, &p.remote, &p.visibility, &p.license, &p.sha, now())
        .map_err(|e| e.to_string())?;
    let _ = store.append_event(&rustsmith_core::Event {
        ts: now(),
        run_id: run_id.clone(),
        kind: "github_repo_created".into(),
        detail: serde_json::json!({
            "repo": p.full, "remote": p.remote, "visibility": p.visibility,
            "license": p.license, "sha": p.sha,
        }),
    });
    host.push_main(&fork, &remote_name)?;
    println!(
        "{}",
        serde_json::json!({
            "repo": p.full, "remote": p.remote, "visibility": p.visibility,
            "license": p.license, "sha": p.sha, "run_id": run_id,
        })
    );
    Ok(())
}

pub fn cmd_create_github_repo(args: &[String]) -> Result<(), String> {
    cmd_with_host(&GhHost, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeHost {
        user: String,
        /// owner/repo -> size; absent = 404.
        repos: std::cell::RefCell<std::collections::BTreeMap<String, i64>>,
        created: std::cell::RefCell<Vec<(String, String, bool)>>,
        pushed: std::cell::RefCell<Vec<(PathBuf, String)>>,
    }
    impl FakeHost {
        fn new(user: &str) -> Self {
            Self {
                user: user.into(),
                repos: Default::default(),
                created: Default::default(),
                pushed: Default::default(),
            }
        }
    }
    impl Host for FakeHost {
        fn current_user(&self) -> Result<String, String> {
            Ok(self.user.clone())
        }
        fn repo_size(&self, owner: &str, repo: &str) -> Result<Option<i64>, String> {
            Ok(self.repos.borrow().get(&format!("{owner}/{repo}")).copied())
        }
        fn create_repo(&self, owner: &str, repo: &str, private: bool) -> Result<(), String> {
            self.created.borrow_mut().push((owner.into(), repo.into(), private));
            self.repos.borrow_mut().insert(format!("{owner}/{repo}"), 0);
            Ok(())
        }
        fn push_main(&self, fork: &Path, remote: &str) -> Result<(), String> {
            self.pushed.borrow_mut().push((fork.to_path_buf(), remote.into()));
            Ok(())
        }
    }

    fn sh(dir: &Path, args: &[&str]) {
        let st = std::process::Command::new(args[0])
            .args(&args[1..])
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(st.status.success(), "{args:?}: {}", String::from_utf8_lossy(&st.stderr));
    }

    /// Scratch project dir: release.toml (+ optional fork + recon fixtures).
    /// Callers add fork dirs / recon records as the case needs.
    struct Fixture {
        dir: PathBuf,
    }
    impl Fixture {
        fn new(github_repo: &str, license_spdx: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "rs-gh-{pid}-{nanos}",
                pid = std::process::id(),
                nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("release.toml"),
                format!(
                    "[release]\n# keep me: fixture comment\nschema = 1\nproject = \"crc\"\nrust_crate = \"crc-rust-core\"\n\
                     pypi_dist = \"crc-rust\"\npython_import = \"crc\"\nversion = \"0.1.0\"\n\
                     requires_python = \">=3.9\"\npython_versions = [\"3.9\"]\nplatforms = [\"ubuntu-22.04\"]\n\
                     github_repo = \"{github_repo}\"\nrelease_workflow = \".github/workflows/release.yml\"\n\
                     pypi_environment = \"pypi\"\ntestpypi_environment = \"testpypi\"\n\
                     license_spdx = \"{license_spdx}\"\nupstream = \"Nicoretti/crc\"\n\
                     upstream_url = \"https://github.com/Nicoretti/crc\"\n\
                     upstream_license = \"BSD-2-Clause\"\n\
                     upstream_authors = \"Nicola Coretti <nico.coretti@gmail.com>, Gert van Dijk <github@gertvandijk.nl>\"\n\
                     smoke_exprs = []\n"
                ),
            )
            .unwrap();
            Self { dir }
        }
        fn fork(&self) -> PathBuf {
            let f = self.dir.join("fork");
            std::fs::create_dir_all(&f).unwrap();
            sh(&f, &["git", "init", "-b", "main", "-q"]);
            sh(&f, &["git", "config", "user.email", "t@t"]);
            sh(&f, &["git", "config", "user.name", "t"]);
            std::fs::write(f.join("LICENSE"), "BSD 2-Clause license text\n").unwrap();
            std::fs::write(
                f.join("NOTICE"),
                "crc-rust: Rust mirror of Nicoretti/crc\nhttps://github.com/Nicoretti/crc\n\
                 Upstream authors: Nicola Coretti <nico.coretti@gmail.com>, Gert van Dijk <github@gertvandijk.nl>\n\
                 Upstream license: BSD-2-Clause\n",
            )
            .unwrap();
            std::fs::write(f.join("lib.rs"), "// SPDX-License-Identifier: BSD-2-Clause\n").unwrap();
            sh(&f, &["git", "add", "-A"]);
            sh(&f, &["git", "commit", "-qm", "mirror baseline"]);
            f
        }
        fn recon(&self, license: Option<&str>, extra_attribution: &[&str]) {
            let r = self.dir.join("recon");
            std::fs::create_dir_all(&r).unwrap();
            let mut attributions: Vec<serde_json::Value> = extra_attribution
                .iter()
                .map(|l| serde_json::json!({"glob": "x", "license": l}))
                .collect();
            if let Some(l) = license {
                attributions.insert(0, serde_json::json!({"glob": "LICENSE", "license": l}));
            }
            std::fs::write(
                r.join("recon.json"),
                serde_json::json!({"license": license.map(|l| serde_json::json!({"license": l})).unwrap_or(serde_json::Value::Null)}).to_string(),
            )
            .unwrap();
            std::fs::write(r.join("facts.json"), serde_json::json!({"probe": {"attribution": attributions}}).to_string())
                .unwrap();
        }
        fn store(&self, run_id: &str) -> PathBuf {
            let db = self.dir.join("store.db");
            let s = rustsmith_store::Store::open(&db).unwrap();
            s.create_run(run_id, "https://github.com/Nicoretti/crc", "python", "run").unwrap();
            db
        }
        fn args(&self, fork: &Path, run_id: &str, extra: &[&str]) -> Vec<String> {
            let mut a = vec![
                "create-github-repo".to_string(),
                "--fork".to_string(),
                fork.display().to_string(),
                "--project".to_string(),
                self.dir.display().to_string(),
                "--recon-out".to_string(),
                self.dir.join("recon").display().to_string(),
                "--run-id".to_string(),
                run_id.to_string(),
                "--store".to_string(),
                self.dir.join("store.db").display().to_string(),
            ];
            a.extend(extra.iter().map(|s| s.to_string()));
            a
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn split_validates_shape() {
        assert_eq!(split_github_repo("a/b").unwrap(), ("a".into(), "b".into()));
        assert!(split_github_repo("ab").is_err());
        assert!(split_github_repo("a/b/c").is_err());
        assert!(split_github_repo("/b").is_err());
        assert!(split_github_repo("a b/c").is_err());
    }

    #[test]
    fn shorten_remote_shapes() {
        assert_eq!(shorten_remote("https://github.com/a/b.git").as_deref(), Some("a/b"));
        assert_eq!(shorten_remote("git@github.com:a/b").as_deref(), Some("a/b"));
        assert_eq!(shorten_remote("https://example.com/a/b"), None);
    }

    #[test]
    fn recon_license_refuses_missing_and_ambiguous() {
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        // No recon dir files at all.
        let r = f.dir.join("recon");
        std::fs::create_dir_all(&r).unwrap();
        assert!(read_recon_license(&r).is_err());
        // "unknown" refused.
        f.recon(Some("unknown"), &[]);
        assert!(read_recon_license(&f.dir.join("recon")).unwrap_err().contains("unknown"));
        // Disagreement refused.
        f.recon(Some("MIT"), &["GPL-3.0-only"]);
        assert!(read_recon_license(&f.dir.join("recon")).unwrap_err().contains("ambiguous"));
        // Agreement accepted.
        f.recon(Some("MIT"), &["MIT"]);
        assert_eq!(read_recon_license(&f.dir.join("recon")).unwrap(), "MIT");
    }

    #[test]
    fn copyleft_keeps_upstream_terms() {
        assert!(is_copyleft("GPL-3.0-only"));
        assert!(is_copyleft("LGPL-2.1-only"));
        assert!(!is_copyleft("MIT"));
        assert!(!is_copyleft("BSD-2-Clause"));
        assert_eq!(effective_license("GPL-3.0-only", "GPL-3.0-only").unwrap(), "GPL-3.0-only");
        assert!(effective_license("GPL-3.0-only", "MIT").is_err());
        assert_eq!(effective_license("BSD-2-Clause", "BSD-2-Clause").unwrap(), "BSD-2-Clause");
        assert!(effective_license("MIT", "GPL-2.0-only").is_err());
    }

    #[test]
    fn dry_run_creates_nothing() {
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        let db = f.store("r1");
        let host = FakeHost::new("Cap");
        cmd_with_host(&host, &f.args(&fork, "r1", &[])).unwrap();
        assert!(host.created.borrow().is_empty());
        assert!(host.pushed.borrow().is_empty());
        // No remote added, no store row.
        assert!(run_prog(&fork, "git", &["remote"], &[]).unwrap().is_empty());
        let s = rustsmith_store::Store::open(&db).unwrap();
        assert!(s.get_github_repo("r1").unwrap().is_none());
    }

    #[test]
    fn yes_path_creates_pushes_records() {
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("cap"); // owner match is case-insensitive
        cmd_with_host(&host, &f.args(&fork, "r1", &["--yes"])).unwrap();
        assert_eq!(host.created.borrow().as_slice(), &[("Cap".into(), "crc-rust".into(), false)]);
        assert_eq!(host.pushed.borrow().as_slice(), &[(fork.clone(), "origin".into())]);
        assert_eq!(
            run_prog(&fork, "git", &["remote", "get-url", "origin"], &[]).unwrap(),
            "https://github.com/Cap/crc-rust.git"
        );
        let s = rustsmith_store::Store::open(&f.dir.join("store.db")).unwrap();
        let row = s.get_github_repo("r1").unwrap().expect("store row");
        assert_eq!(row.repo, "Cap/crc-rust");
        assert_eq!(row.remote, "https://github.com/Cap/crc-rust.git");
        assert_eq!(row.visibility, "public");
        assert_eq!(row.license, "BSD-2-Clause");
        assert!(!row.sha.is_empty());
    }

    #[test]
    fn mismatched_name_fixes_release_toml() {
        let f = Fixture::new("Cap/wrong-name", "MIT");
        let fork = f.fork();
        std::fs::write(
            fork.join("NOTICE"),
            "crc-rust: Rust mirror of Nicoretti/crc\nhttps://github.com/Nicoretti/crc\n\
             Upstream authors: Nicola Coretti <nico.coretti@gmail.com>, Gert van Dijk <github@gertvandijk.nl>\n\
             Upstream license: MIT\n",
        )
        .unwrap();
        sh(&fork, &["git", "add", "-A"]);
        sh(&fork, &["git", "commit", "-qm", "notice"]);
        f.recon(Some("MIT"), &["MIT"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        cmd_with_host(&host, &f.args(&fork, "r1", &["--yes"])).unwrap();
        assert_eq!(host.created.borrow()[0].1, "crc-rust");
        let text = std::fs::read_to_string(f.dir.join("release.toml")).unwrap();
        assert!(text.contains("github_repo = \"Cap/crc-rust\""), "{text}");
        // Comments/formatting elsewhere preserved (line-scoped edit).
        assert!(text.contains("#"), "{text}");
    }

    #[test]
    fn refusals_cover_owner_upstream_nonempty_dirty() {
        // Foreign owner.
        let f = Fixture::new("Evil/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        assert!(cmd_with_host(&host, &f.args(&fork, "r1", &["--yes"])).unwrap_err().contains("does not control"));
        // Existing non-empty repo.
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        host.repos.borrow_mut().insert("Cap/crc-rust".into(), 42);
        assert!(cmd_with_host(&host, &f.args(&fork, "r1", &["--yes"])).unwrap_err().contains("non-empty"));
        assert!(host.created.borrow().is_empty());
        // Upstream remote.
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        sh(&fork, &["git", "remote", "add", "upstream", "https://github.com/Nicoretti/crc.git"]);
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        assert!(cmd_with_host(&host, &f.args(&fork, "r1", &["--yes"])).unwrap_err().contains("upstream"));
        // Dirty tree.
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        std::fs::write(fork.join("draft.txt"), "x").unwrap();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        assert!(cmd_with_host(&host, &f.args(&fork, "r1", &["--yes"])).unwrap_err().contains("dirty"));
        // Unknown run id.
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        assert!(cmd_with_host(&host, &f.args(&fork, "nope", &["--yes"])).unwrap_err().contains("unknown --run-id"));
    }

    #[test]
    fn empty_existing_repo_pushes_without_create() {
        let f = Fixture::new("Cap/crc-rust", "BSD-2-Clause");
        let fork = f.fork();
        f.recon(Some("BSD-2-Clause"), &["BSD-2-Clause"]);
        f.store("r1");
        let host = FakeHost::new("Cap");
        host.repos.borrow_mut().insert("Cap/crc-rust".into(), 0);
        cmd_with_host(&host, &f.args(&fork, "r1", &["--yes", "--private"])).unwrap();
        assert!(host.created.borrow().is_empty());
        assert_eq!(host.pushed.borrow().len(), 1);
        let s = rustsmith_store::Store::open(&f.dir.join("store.db")).unwrap();
        assert_eq!(s.get_github_repo("r1").unwrap().unwrap().visibility, "private");
    }
}
