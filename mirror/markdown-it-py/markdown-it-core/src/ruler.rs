//! `Ruler` (mirror of `markdown_it/ruler.py`).
//!
//! Generic over the rule payload `K` (builtin discriminant or Python
//! registry id); table mechanics (ordering, enable/disable, per-chain
//! caching with invalidation on every mutation) are shared by all four
//! rule chains.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RuleEntry<K: Clone> {
    pub name: String,
    pub enabled: bool,
    pub kind: K,
    pub alt: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Ruler<K: Clone> {
    rules: Vec<RuleEntry<K>>,
    cache: Option<HashMap<String, Vec<K>>>,
}

impl<K: Clone> Ruler<K> {
    pub fn new() -> Self {
        Self { rules: Vec::new(), cache: None }
    }

    pub fn push(&mut self, name: &str, kind: K, alt: &[&str]) {
        self.rules.push(RuleEntry {
            name: name.to_string(),
            enabled: true,
            kind,
            alt: alt.iter().map(|s| s.to_string()).collect(),
        });
        self.cache = None;
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.rules.iter().position(|r| r.name == name)
    }

    pub fn at(&mut self, name: &str, kind: K, alt: &[String]) -> Result<(), String> {
        match self.find(name) {
            Some(i) => {
                self.rules[i].kind = kind;
                self.rules[i].alt = alt.to_vec();
                self.cache = None;
                Ok(())
            }
            None => Err(format!("Parser rule not found: {name}")),
        }
    }

    pub fn before(&mut self, before: &str, name: &str, kind: K, alt: &[String]) -> Result<(), String> {
        match self.find(before) {
            Some(i) => {
                self.rules.insert(
                    i,
                    RuleEntry { name: name.to_string(), enabled: true, kind, alt: alt.to_vec() },
                );
                self.cache = None;
                Ok(())
            }
            None => Err(format!("Parser rule not found: {before}")),
        }
    }

    pub fn after(&mut self, after: &str, name: &str, kind: K, alt: &[String]) -> Result<(), String> {
        match self.find(after) {
            Some(i) => {
                self.rules.insert(
                    i + 1,
                    RuleEntry { name: name.to_string(), enabled: true, kind, alt: alt.to_vec() },
                );
                self.cache = None;
                Ok(())
            }
            None => Err(format!("Parser rule not found: {after}")),
        }
    }

    pub fn enable(&mut self, names: &[String], ignore_invalid: bool) -> Result<Vec<String>, String> {
        let mut result = Vec::new();
        for name in names {
            match self.find(name) {
                Some(i) => {
                    self.rules[i].enabled = true;
                    result.push(name.clone());
                }
                None if ignore_invalid => {}
                None => return Err(format!("Rules manager: invalid rule name {name}")),
            }
        }
        self.cache = None;
        Ok(result)
    }

    pub fn enable_only(&mut self, names: &[String], ignore_invalid: bool) -> Result<Vec<String>, String> {
        for rule in &mut self.rules {
            rule.enabled = false;
        }
        self.enable(names, ignore_invalid)
    }

    pub fn disable(&mut self, names: &[String], ignore_invalid: bool) -> Result<Vec<String>, String> {
        let mut result = Vec::new();
        for name in names {
            match self.find(name) {
                Some(i) => {
                    self.rules[i].enabled = false;
                    result.push(name.clone());
                }
                None if ignore_invalid => {}
                None => return Err(format!("Rules manager: invalid rule name {name}")),
            }
        }
        self.cache = None;
        Ok(result)
    }

    fn compile(&mut self) {
        let mut chains: Vec<String> = vec![String::new()];
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            for alt in &rule.alt {
                if !chains.contains(alt) {
                    chains.push(alt.clone());
                }
            }
        }
        let mut cache: HashMap<String, Vec<K>> = HashMap::new();
        for chain in &chains {
            let mut list = Vec::new();
            for rule in &self.rules {
                if !rule.enabled {
                    continue;
                }
                if !chain.is_empty() && !rule.alt.iter().any(|a| a == chain) {
                    continue;
                }
                list.push(rule.kind.clone());
            }
            cache.insert(chain.clone(), list);
        }
        self.cache = Some(cache);
    }

    /// Active rule payloads for a chain (default `""`). Compiles the cache
    /// on first use after any mutation.
    pub fn get_rules(&mut self, chain: &str) -> Vec<K> {
        if self.cache.is_none() {
            self.compile();
        }
        self.cache.as_ref().and_then(|c| c.get(chain)).cloned().unwrap_or_default()
    }

    /// Active rule payloads WITHOUT compiling (binding helper for snapshots).
    pub fn active_kinds(&self) -> Vec<K> {
        self.rules.iter().filter(|r| r.enabled).map(|r| r.kind.clone()).collect()
    }

    pub fn rule_kind(&self, name: &str) -> Option<K> {
        self.find(name).map(|i| self.rules[i].kind.clone())
    }

    pub fn get_all_rules(&self) -> Vec<String> {
        self.rules.iter().map(|r| r.name.clone()).collect()
    }

    pub fn get_active_rules(&self) -> Vec<String> {
        self.rules.iter().filter(|r| r.enabled).map(|r| r.name.clone()).collect()
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.find(name).map(|i| self.rules[i].enabled).unwrap_or(false)
    }
}
