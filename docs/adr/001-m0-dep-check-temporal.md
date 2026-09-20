# ADR 001: M0 dep-check is temporal, permanent invariant is gates/oracle isolation
- Context: M0 acceptance forbade any `rustsmith-agent` crate; M1 adds it by plan.
- Decision: M0 script enforces permanent SPEC §5 rule (gates/oracle clean) always;
  the "no agent crate in tree" check applied at M0 time only.
- Consequences: `m0..mN` chain stays green after M1; CI dep-check is the gate.
- Alternatives: freeze M0 script verbatim (chain red forever) — rejected.
- Spec: literal §5 hard rule wins over plan-doc temporal check.
