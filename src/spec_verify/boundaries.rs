use std::path::{Path, PathBuf};

use crate::spec_core::{
    BoundaryCategory, Evidence, ScenarioResult, SpecResult, StepVerdict, Verdict,
};

use super::{VerificationContext, Verifier};

/// Mechanical verifier for file-level task boundaries.
///
/// This verifier only runs when callers provide an explicit `change_paths` set.
/// It does not infer diffs from VCS state. `exempt` paths (workspace-relative)
/// skip the `Allowed changes` check but are still held to `Forbidden`: guard
/// uses them for changes that belong to another contract or are the
/// contract's own file.
#[derive(Debug, Clone, Default)]
pub struct BoundariesVerifier {
    exempt: Vec<String>,
}

impl BoundariesVerifier {
    pub fn with_exempt(exempt: Vec<String>) -> Self {
        Self { exempt }
    }
}

/// One active contract's territory as guard sees it: its file and the paths
/// its `Allowed changes` list claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractTerritory {
    /// Workspace-relative path of the contract file.
    pub spec_path: String,
    /// Normalized `Allowed changes` path patterns.
    pub allowed: Vec<String>,
}

impl ContractTerritory {
    pub fn new(spec_path: impl Into<String>, sections: &[crate::spec_core::Section]) -> Self {
        Self {
            spec_path: normalize_pattern(&spec_path.into()),
            allowed: collect_path_boundaries(sections).0,
        }
    }

    /// The contract file or where it was promoted from
    /// (`<spec dir>/roadmap/<file name>`).
    fn owns_file(&self, change: &str) -> bool {
        if change == self.spec_path {
            return true;
        }
        let (dir, name) = match self.spec_path.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name),
            None => (String::new(), self.spec_path.as_str()),
        };
        change == format!("{dir}roadmap/{name}")
    }

    fn allows(&self, change: &str) -> bool {
        self.allowed
            .iter()
            .any(|pattern| path_matches_pattern(pattern, change))
    }
}

/// How guard checks one contract's boundaries against a commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractBoundaryPlan {
    /// The commit touches the contract's territory, so its boundaries apply;
    /// otherwise guard checks no boundary for it.
    pub touched: bool,
    /// Changes exempt from its `Allowed changes` check.
    pub exempt: Vec<String>,
}

/// Plan guard's boundary check for every active contract (LEP-007 D6). A
/// contract's `Forbidden` and `Allowed changes` apply only to a commit that
/// touches its territory: a change matches its allow-list or is its contract
/// file (or the file's `roadmap/` move source). Within such a commit, a
/// change that belongs to another active contract (and not to this one) and
/// the contract's own file are exempt from the allow-list; a change no
/// contract claims is not, so it is out of bounds for every touched contract.
/// `strict` keeps the pre-D6 behavior: every contract checks every change.
/// `changes` are workspace-relative.
pub fn plan_contract_boundaries(
    contracts: &[ContractTerritory],
    changes: &[String],
    strict: bool,
) -> Vec<ContractBoundaryPlan> {
    contracts
        .iter()
        .enumerate()
        .map(|(index, contract)| {
            if strict {
                return ContractBoundaryPlan {
                    touched: true,
                    exempt: Vec::new(),
                };
            }
            let touched = changes
                .iter()
                .any(|change| contract.owns_file(change) || contract.allows(change));
            let exempt = changes
                .iter()
                .filter(|change| {
                    contract.owns_file(change)
                        || (!contract.allows(change)
                            && contracts.iter().enumerate().any(|(other, territory)| {
                                other != index && territory.allows(change)
                            }))
                })
                .cloned()
                .collect();
            ContractBoundaryPlan { touched, exempt }
        })
        .collect()
}

/// Normalize changed paths the way [`BoundariesVerifier`] does, relative to
/// the workspace root found from `code_paths` (or the changes themselves).
pub fn normalize_boundary_paths(code_paths: &[PathBuf], paths: &[PathBuf]) -> Vec<String> {
    let workspace_root = find_workspace_root(code_paths).or_else(|| find_workspace_root(paths));
    paths
        .iter()
        .map(|path| {
            // Relative paths (for example a spec file under `specs/`) are
            // resolved first, so they strip the canonical workspace root.
            let absolute = if path.is_relative() {
                path.canonicalize().unwrap_or_else(|_| path.clone())
            } else {
                path.clone()
            };
            let normalized = normalize_path(&absolute, workspace_root.as_deref());
            if normalized.starts_with('/') {
                normalize_path(path, workspace_root.as_deref())
            } else {
                normalized
            }
        })
        .collect()
}

impl Verifier for BoundariesVerifier {
    fn name(&self) -> &str {
        "boundaries"
    }

    fn verify(&self, ctx: &VerificationContext) -> SpecResult<Vec<ScenarioResult>> {
        let (allowed, forbidden) = collect_path_boundaries(&ctx.resolved_spec.task.sections);
        if ctx.change_paths.is_empty() || (allowed.is_empty() && forbidden.is_empty()) {
            return Ok(Vec::new());
        }

        let workspace_root =
            find_workspace_root(&ctx.code_paths).or_else(|| find_workspace_root(&ctx.change_paths));
        let changes = normalize_change_paths(&ctx.change_paths, workspace_root.as_deref());
        if changes.is_empty() {
            return Ok(Vec::new());
        }

        let mut step_results = Vec::new();
        let mut evidence = Vec::new();
        let mut has_failure = false;

        for change in changes {
            if let Some(pattern) = forbidden
                .iter()
                .find(|pattern| path_matches_pattern(pattern, &change))
            {
                has_failure = true;
                step_results.push(StepVerdict {
                    step_text: change.clone(),
                    verdict: Verdict::Fail,
                    reason: format!("matches forbidden boundary `{pattern}`"),
                });
                evidence.push(Evidence::PatternMatch {
                    pattern: pattern.clone(),
                    matched: true,
                    locations: vec![change],
                });
                continue;
            }

            if self.exempt.iter().any(|exempt| exempt == &change) {
                step_results.push(StepVerdict {
                    step_text: change.clone(),
                    verdict: Verdict::Pass,
                    reason: "belongs to another active contract or is this contract's own file"
                        .into(),
                });
                continue;
            }

            if let Some(pattern) = allowed
                .iter()
                .find(|pattern| path_matches_pattern(pattern, &change))
            {
                step_results.push(StepVerdict {
                    step_text: change.clone(),
                    verdict: Verdict::Pass,
                    reason: format!("matches allowed boundary `{pattern}`"),
                });
                evidence.push(Evidence::PatternMatch {
                    pattern: pattern.clone(),
                    matched: true,
                    locations: vec![change],
                });
                continue;
            }

            if !allowed.is_empty() {
                has_failure = true;
                step_results.push(StepVerdict {
                    step_text: change.clone(),
                    verdict: Verdict::Fail,
                    reason: "not covered by any allowed boundary".into(),
                });
                evidence.push(Evidence::PatternMatch {
                    pattern: "<allowed-boundaries>".into(),
                    matched: false,
                    locations: vec![change],
                });
            } else {
                step_results.push(StepVerdict {
                    step_text: change.clone(),
                    verdict: Verdict::Pass,
                    reason: "no allow-list declared; change accepted because it is not forbidden"
                        .into(),
                });
            }
        }

        Ok(vec![ScenarioResult {
            scenario_name: "[boundaries] explicit change set respects declared paths".into(),
            verdict: if has_failure {
                Verdict::Fail
            } else {
                Verdict::Pass
            },
            step_results,
            evidence,
            duration_ms: 0,
            provenance: None,
        }])
    }
}

fn collect_path_boundaries(sections: &[crate::spec_core::Section]) -> (Vec<String>, Vec<String>) {
    let mut allowed = Vec::new();
    let mut forbidden = Vec::new();

    for section in sections {
        if let crate::spec_core::Section::Boundaries { items, .. } = section {
            for item in items {
                if !looks_like_path_boundary(&item.text) {
                    continue;
                }

                match item.category {
                    BoundaryCategory::Allow => allowed.push(normalize_pattern(&item.text)),
                    // Symbols are code-graph references, never path globs.
                    BoundaryCategory::Symbols => {}
                    BoundaryCategory::Deny | BoundaryCategory::General => {
                        forbidden.push(normalize_pattern(&item.text))
                    }
                }
            }
        }
    }

    (allowed, forbidden)
}

fn looks_like_path_boundary(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('*')
        || trimmed.ends_with(".rs")
        || trimmed.ends_with(".ts")
        || trimmed.ends_with(".js")
        || trimmed.ends_with(".py")
        || trimmed.ends_with(".md")
        || trimmed.ends_with(".spec")
        || trimmed.ends_with(".spec.md")
}

fn normalize_change_paths(paths: &[PathBuf], workspace_root: Option<&Path>) -> Vec<String> {
    let mut changes = Vec::new();

    for path in paths {
        let normalized = normalize_path(path, workspace_root);
        if !normalized.is_empty() && !changes.iter().any(|item| item == &normalized) {
            changes.push(normalized);
        }
    }

    changes
}

fn normalize_path(path: &Path, workspace_root: Option<&Path>) -> String {
    let candidate = workspace_root
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path);

    candidate
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string()
}

fn normalize_pattern(pattern: &str) -> String {
    pattern
        .trim()
        .trim_matches('`')
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_matches('/')
        .to_string()
}

fn path_matches_pattern(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let path_segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    match_segments(&pattern_segments, &path_segments)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }

    if pattern[0] == "**" {
        return (0..=path.len()).any(|index| match_segments(&pattern[1..], &path[index..]));
    }

    if path.is_empty() {
        return false;
    }

    segment_matches(pattern[0], path[0]) && match_segments(&pattern[1..], &path[1..])
}

fn segment_matches(pattern: &str, segment: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') {
        return pattern == segment;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let anchored_start = !pattern.starts_with('*');
    let anchored_end = !pattern.ends_with('*');
    let mut cursor = 0usize;

    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if index == 0 && anchored_start {
            if !segment[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
            continue;
        }

        if let Some(found) = segment[cursor..].find(part) {
            cursor += found + part.len();
        } else {
            return false;
        }
    }

    if anchored_end && let Some(last_part) = parts.iter().rev().find(|part| !part.is_empty()) {
        return segment.ends_with(last_part);
    }

    true
}

fn find_workspace_root(paths: &[PathBuf]) -> Option<PathBuf> {
    for path in paths {
        let mut current = if path.is_file() {
            path.parent()?.to_path_buf()
        } else {
            path.clone()
        };

        loop {
            if current.join("Cargo.toml").is_file() {
                return Some(current.canonicalize().unwrap_or(current));
            }
            if !current.pop() {
                break;
            }
        }
    }

    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use crate::spec_core::{ResolvedSpec, Scenario, SpecLevel, SpecMeta, SpecResult, Verdict};

    use super::{
        BoundariesVerifier, ContractTerritory, path_matches_pattern, plan_contract_boundaries,
    };
    use crate::spec_verify::{AiMode, VerificationContext, Verifier};

    fn make_resolved_spec(input: &str) -> SpecResult<ResolvedSpec> {
        let doc = crate::spec_parser::parse_spec_from_str(input)?;
        Ok(ResolvedSpec {
            task: doc,
            inherited_constraints: Vec::new(),
            inherited_decisions: Vec::new(),
            all_scenarios: Vec::<Scenario>::new(),
        })
    }

    #[test]
    fn matches_double_star_path_patterns() {
        assert!(path_matches_pattern(
            "crates/spec-parser/**",
            "crates/spec-parser/src/parser.rs"
        ));
        assert!(path_matches_pattern("specs/**", "specs/task.spec"));
        assert!(!path_matches_pattern(
            "crates/spec-parser/**",
            "crates/spec-gateway/src/lib.rs"
        ));
    }

    const CONTRACT_A: &str = "spec: task\nname: \"A\"\n---\n\n## Boundaries\n\n### Allowed Changes\n- crates/ingest/**\n\n### Forbidden\n- knowledge/**\n";
    const CONTRACT_B: &str =
        "spec: task\nname: \"B\"\n---\n\n## Boundaries\n\n### Allowed Changes\n- crates/query/**\n";
    const CONTRACT_C: &str = "spec: task\nname: \"C\"\n---\n\n## Boundaries\n\n### Allowed Changes\n- crates/server/**\n\n### Forbidden\n- knowledge/**\n";

    /// Run guard's per-contract boundary check: plan every contract's
    /// territory against `changes`, then verify each contract with the change
    /// set and exemptions its plan assigns. Returns (spec path, results).
    fn guard_boundaries(
        contracts: &[(&str, &str)],
        changes: &[&str],
        strict: bool,
    ) -> Vec<(String, Vec<crate::spec_core::ScenarioResult>)> {
        let resolved = contracts
            .iter()
            .map(|(path, input)| (path.to_string(), make_resolved_spec(input).unwrap()))
            .collect::<Vec<_>>();
        let territories = resolved
            .iter()
            .map(|(path, spec)| ContractTerritory::new(path.clone(), &spec.task.sections))
            .collect::<Vec<_>>();
        let change_strings = changes.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        let plans = plan_contract_boundaries(&territories, &change_strings, strict);
        resolved
            .into_iter()
            .zip(plans)
            .map(|((path, spec), plan)| {
                let change_paths = if plan.touched {
                    changes.iter().map(PathBuf::from).collect()
                } else {
                    Vec::new()
                };
                let results = BoundariesVerifier::with_exempt(plan.exempt)
                    .verify(&VerificationContext {
                        code_paths: vec![PathBuf::from(".")],
                        change_paths,
                        ai_mode: AiMode::Off,
                        resolved_spec: spec,
                    })
                    .unwrap();
                (path, results)
            })
            .collect()
    }

    fn failures(results: &[crate::spec_core::ScenarioResult]) -> Vec<(String, String)> {
        results
            .iter()
            .flat_map(|result| &result.step_results)
            .filter(|step| step.verdict == Verdict::Fail)
            .map(|step| (step.step_text.clone(), step.reason.clone()))
            .collect()
    }

    fn two_contracts() -> [(&'static str, &'static str); 2] {
        [
            ("specs/task-a.spec.md", CONTRACT_A),
            ("specs/task-b.spec.md", CONTRACT_B),
        ]
    }

    #[test]
    fn test_guard_boundaries_skip_contract_whose_territory_is_untouched() {
        let out = guard_boundaries(&two_contracts(), &["crates/query/src/lib.rs"], false);
        assert!(
            out[0].1.is_empty(),
            "contract A was not touched: {:?}",
            out[0].1
        );
        assert_eq!(out[1].1.len(), 1);
        assert_eq!(out[1].1[0].verdict, Verdict::Pass);
    }

    #[test]
    fn test_guard_boundaries_forbidden_applies_to_touching_commit() {
        let out = guard_boundaries(
            &two_contracts(),
            &["crates/ingest/src/lib.rs", "knowledge/x.md"],
            false,
        );
        let a = failures(&out[0].1);
        assert_eq!(a.len(), 1, "{a:?}");
        assert_eq!(a[0].0, "knowledge/x.md");
        assert!(a[0].1.contains("forbidden"), "{a:?}");
    }

    #[test]
    fn test_guard_boundaries_other_contract_paths_are_not_out_of_bounds() {
        let out = guard_boundaries(
            &two_contracts(),
            &["crates/ingest/src/lib.rs", "crates/query/src/lib.rs"],
            false,
        );
        for (path, results) in &out {
            assert!(
                failures(results).is_empty(),
                "{path}: {:?}",
                failures(results)
            );
            assert_eq!(results[0].verdict, Verdict::Pass, "{path}");
        }
    }

    #[test]
    fn test_guard_boundaries_contract_file_and_move_source_are_in_bounds() {
        let out = guard_boundaries(
            &[("specs/task-c.spec.md", CONTRACT_C)],
            &["specs/task-c.spec.md", "specs/roadmap/task-c.spec.md"],
            false,
        );
        assert_eq!(out[0].1.len(), 1, "contract C's territory is touched");
        assert!(failures(&out[0].1).is_empty(), "{:?}", failures(&out[0].1));
        assert_eq!(out[0].1[0].verdict, Verdict::Pass);

        // The contract file alone still counts as touching the territory, so
        // its Forbidden list applies to the rest of the commit.
        let out = guard_boundaries(
            &[("specs/task-c.spec.md", CONTRACT_C)],
            &["specs/task-c.spec.md", "knowledge/x.md"],
            false,
        );
        let c = failures(&out[0].1);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(c[0].0, "knowledge/x.md");
    }

    #[test]
    fn test_guard_boundaries_unowned_path_fails_every_touched_contract() {
        let out = guard_boundaries(
            &two_contracts(),
            &[
                "crates/ingest/src/lib.rs",
                "crates/query/src/lib.rs",
                "docs/guide.md",
            ],
            false,
        );
        for (path, results) in &out {
            let failed = failures(results);
            assert_eq!(failed.len(), 1, "{path}: {failed:?}");
            assert_eq!(failed[0].0, "docs/guide.md");
            assert!(failed[0].1.contains("not covered by any allowed boundary"));
        }
    }

    #[test]
    fn test_guard_boundaries_commit_touching_no_territory_passes() {
        for changes in [
            vec!["docs/guide.md", ".gitlab-ci.yml"],
            vec!["knowledge/requirements/req-x.md"],
        ] {
            let out = guard_boundaries(&two_contracts(), &changes, false);
            for (path, results) in &out {
                assert!(results.is_empty(), "{path} {changes:?}: {results:?}");
            }
        }
    }

    #[test]
    fn test_guard_boundaries_strict_mode_checks_every_contract() {
        let out = guard_boundaries(&two_contracts(), &["docs/guide.md"], true);
        for (path, results) in &out {
            let failed = failures(results);
            assert_eq!(failed.len(), 1, "{path}: {failed:?}");
            assert!(failed[0].1.contains("not covered by any allowed boundary"));
        }
    }

    #[test]
    fn test_boundaries_verifier_accepts_changes_within_allowed_paths() {
        let resolved = make_resolved_spec(
            r#"spec: task
name: "边界"
---

## 边界

### 允许修改
- crates/spec-parser/**

### 禁止做
- crates/spec-gateway/**
"#,
        )
        .unwrap();

        let verifier = BoundariesVerifier::default();
        let results = verifier
            .verify(&VerificationContext {
                code_paths: vec![PathBuf::from(".")],
                change_paths: vec![PathBuf::from("crates/spec-parser/src/parser.rs")],
                ai_mode: AiMode::Off,
                resolved_spec: resolved,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, Verdict::Pass);
    }

    #[test]
    fn test_boundaries_verifier_accepts_root_markdown_boundaries() {
        let resolved = make_resolved_spec(
            r#"spec: task
name: "Docs"
---

## Boundaries

### Allowed Changes
- README.md
"#,
        )
        .unwrap();

        let verifier = BoundariesVerifier::default();
        let results = verifier
            .verify(&VerificationContext {
                code_paths: vec![PathBuf::from(".")],
                change_paths: vec![PathBuf::from("README.md")],
                ai_mode: AiMode::Off,
                resolved_spec: resolved,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, Verdict::Pass);
    }

    #[test]
    fn test_boundaries_verifier_rejects_change_outside_allowed_paths() {
        let resolved = make_resolved_spec(
            r#"spec: task
name: "边界"
---

## Boundaries

### Allowed Changes
- crates/spec-parser/**
"#,
        )
        .unwrap();

        let verifier = BoundariesVerifier::default();
        let results = verifier
            .verify(&VerificationContext {
                code_paths: vec![PathBuf::from(".")],
                change_paths: vec![PathBuf::from("crates/spec-gateway/src/lifecycle.rs")],
                ai_mode: AiMode::Off,
                resolved_spec: resolved,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, Verdict::Fail);
        assert!(
            results[0].step_results[0]
                .reason
                .contains("not covered by any allowed boundary")
        );
    }

    #[test]
    fn test_boundaries_verifier_rejects_change_matching_forbidden_boundary() {
        let resolved = make_resolved_spec(
            r#"spec: task
name: "边界"
---

## 边界

### 允许修改
- crates/spec-gateway/**

### 禁止做
- crates/spec-gateway/src/lib.rs
"#,
        )
        .unwrap();

        let verifier = BoundariesVerifier::default();
        let results = verifier
            .verify(&VerificationContext {
                code_paths: vec![PathBuf::from(".")],
                change_paths: vec![PathBuf::from("crates/spec-gateway/src/lib.rs")],
                ai_mode: AiMode::Off,
                resolved_spec: resolved,
            })
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, Verdict::Fail);
        assert!(
            results[0].step_results[0]
                .reason
                .contains("matches forbidden boundary")
        );
    }

    #[test]
    fn verifier_skips_when_no_explicit_change_paths_are_provided() {
        let resolved = ResolvedSpec {
            task: crate::spec_core::SpecDocument {
                meta: SpecMeta {
                    level: SpecLevel::Task,
                    name: "边界".into(),
                    inherits: None,
                    lang: vec![],
                    tags: vec![],
                    depends: vec![],
                    estimate: None,
                    capability: None,
                    satisfies: vec![],
                    risk: None,
                },
                sections: vec![],
                lint_acks: vec![],
                source_path: PathBuf::new(),
            },
            inherited_constraints: Vec::new(),
            inherited_decisions: Vec::new(),
            all_scenarios: Vec::new(),
        };

        let verifier = BoundariesVerifier::default();
        let results = verifier
            .verify(&VerificationContext {
                code_paths: vec![PathBuf::from(".")],
                change_paths: Vec::new(),
                ai_mode: AiMode::Off,
                resolved_spec: resolved,
            })
            .unwrap();

        assert!(results.is_empty());
    }
}
