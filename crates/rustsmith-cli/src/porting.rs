//! Deterministic PORTING.md (Track H).
//!
//! Repo API rules live in RepoFacts (`recon/facts.json`, `rules.porting_rules`),
//! seeded at recon from package-keyed data; ABI/layout rules live on frontends
//! (`Frontend::language_rules`). This module seeds facts from data, renders
//! facts to PORTING.md, and parses facts back — it holds no rule content and
//! names no repo.

/// One porting rule: original-pattern + rust-pattern + example triple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortingRule {
    pub id: String,
    pub title: String,
    pub original: String,
    pub rust: String,
    pub example: String,
}

impl PortingRule {
    fn from_json(id: &str, v: &serde_json::Value) -> Result<Self, String> {
        Ok(Self {
            id: id.to_string(),
            title: v["title"].as_str().unwrap_or("").to_string(),
            original: v["original"].as_str().unwrap_or("").to_string(),
            rust: v["rust"].as_str().unwrap_or("").to_string(),
            example: v["example"].as_str().unwrap_or("").to_string(),
        })
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "title": self.title,
            "original": self.original,
            "rust": self.rust,
            "example": self.example,
        })
    }
}

/// Seed rules for `package` from package-keyed data (recon time). Ids are
/// stable (`R001`…); the frozen facts carry them verbatim.
pub fn porting_rules_for(package: &str) -> Result<Vec<PortingRule>, String> {
    let entry = crate::repo_content::entry(package)?;
    let rules = entry["porting_rules"]
        .as_array()
        .ok_or_else(|| format!("no porting rules for package '{package}'"))?;
    rules
        .iter()
        .enumerate()
        .map(|(i, r)| PortingRule::from_json(&format!("R{:03}", i + 1), r))
        .collect()
}

/// Render rules to the pinned PORTING.md shape (count + triple per rule).
/// The header names the package and the composite languages; no repo or
/// language is hardcoded here.
pub fn render_porting_md(package: &str, languages: &[String], rules: &[PortingRule]) -> String {
    let mut langs: Vec<String> = languages
        .iter()
        .map(|l| {
            let mut c = l.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    langs.sort();
    langs.dedup();
    let mut md = format!(
        "# PORTING.md — {package} {}→Rust rulebook (architect-v1)\n\n\
         Concrete translation rules. Each rule has original-pattern, rust-pattern, and example.\n\n",
        langs.join("+"),
    );
    for r in rules {
        md.push_str(&format!(
            "## {}: {}\n- {}\n- Rust: {}\n- Example: {}\n\n",
            r.id,
            r.title,
            r.original,
            r.rust.replace("Rust: ", ""),
            r.example.replace("Example: ", "")
        ));
    }
    md
}

/// Serialize rules for the `facts.json` rules section.
pub fn rules_to_facts(rules: &[PortingRule]) -> serde_json::Value {
    serde_json::Value::Array(rules.iter().map(PortingRule::to_json).collect())
}

/// Parse rules back out of a `facts.json` value (stages read facts, never
/// generators).
pub fn rules_from_facts(facts: &serde_json::Value) -> Result<Vec<PortingRule>, String> {
    let rules = facts["rules"]["porting_rules"]
        .as_array()
        .ok_or_else(|| "facts.json lacks rules.porting_rules".to_string())?;
    rules
        .iter()
        .map(|r| {
            let id = r["id"].as_str().unwrap_or("");
            if id.is_empty() {
                return Err("facts rule lacks id".to_string());
            }
            PortingRule::from_json(id, r)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every packaged rulebook renders with stable ids and the triple shape.
    #[test]
    fn rulebooks_render_with_triple() {
        for package in crate::repo_content::packages() {
            let rules = porting_rules_for(&package).unwrap();
            assert!(!rules.is_empty(), "no rules for {package}");
            let md = render_porting_md(&package, &["python".to_string()], &rules);
            let count = md.lines().filter(|l| l.starts_with("## R")).count();
            assert_eq!(count, rules.len(), "rule count for {package}");
            for block in md.split("## R").skip(1) {
                assert!(block.contains("Original"), "block lacks Original: {block:.200}");
                assert!(block.contains("Rust:"), "block lacks Rust: {block:.200}");
                assert!(block.contains("Example:"), "block lacks Example: {block:.200}");
            }
            // Facts round-trip preserves every rule.
            let facts = serde_json::json!({"rules": {"porting_rules": rules_to_facts(&rules)}});
            assert_eq!(rules_from_facts(&facts).unwrap(), rules);
        }
    }
}
