use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use crate::spec_core::{
    Evidence, ReviewMode, Scenario, ScenarioMode, ScenarioResult, SpecError, SpecResult,
    StepVerdict, TestOutputDetail, TestSelector, Verdict,
};

use super::test_batch::{
    BatchOutcomes, GroupKey, GroupOutcome, TestBatchPlan, attribute, execute_plan,
};
use super::{VerificationContext, Verifier};

/// Runs each bound scenario's `cargo test` and judges the verdict from what
/// libtest reports. The runner is injectable so the judgment can be tested
/// without spawning cargo; production uses [`CargoRunner`].
pub struct TestVerifier {
    runner: Arc<dyn TestCommandRunner>,
    options: TestRunOptions,
    /// Group runs already executed for several specs at once (guard); when
    /// absent, this spec's own groups are run.
    prefetched: Option<Arc<BatchOutcomes>>,
}

/// How bound tests are run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TestRunOptions {
    /// Run `Mode: soak` scenarios (with `--include-ignored`) instead of
    /// reporting them `deferred`.
    pub include_soak: bool,
    /// One `cargo test -q` per scenario instead of one batched run per
    /// (package, features, soak) group.
    pub no_batch: bool,
    /// libtest `--test-threads` for batched runs (default 1, so tests
    /// validated one at a time are not suddenly run concurrently).
    pub test_threads: Option<usize>,
}

impl Default for TestVerifier {
    fn default() -> Self {
        Self::with_runner(Arc::new(CargoRunner))
    }
}

impl TestVerifier {
    pub fn with_runner(runner: Arc<dyn TestCommandRunner>) -> Self {
        Self {
            runner,
            options: TestRunOptions::default(),
            prefetched: None,
        }
    }

    pub fn with_prefetched(mut self, outcomes: Option<Arc<BatchOutcomes>>) -> Self {
        self.prefetched = outcomes;
        self
    }

    /// Run the batch plan for several specs' scenarios at once, so guard
    /// builds and runs each group once rather than once per spec.
    pub fn prefetch(
        specs: &[&crate::spec_core::ResolvedSpec],
        code_paths: &[PathBuf],
        options: &TestRunOptions,
    ) -> SpecResult<Option<Arc<BatchOutcomes>>> {
        if options.no_batch {
            return Ok(None);
        }
        let Some(workspace_root) = find_workspace_root(code_paths) else {
            return Ok(None);
        };
        let legacy_bindings = collect_legacy_comment_bindings(code_paths)?;
        let bindings = specs
            .iter()
            .flat_map(|spec| &spec.all_scenarios)
            .filter_map(|scenario| {
                resolve_test_binding(scenario, &legacy_bindings)
                    .map(|binding| (binding.selector, scenario.mode == ScenarioMode::Soak))
            })
            .collect::<Vec<_>>();
        let plan = TestBatchPlan::build(
            bindings.iter().map(|(selector, soak)| (selector, *soak)),
            options,
        );
        Ok(Some(Arc::new(execute_plan(
            &plan,
            &CargoRunner,
            &workspace_root,
            options,
        )?)))
    }

    pub fn with_options(mut self, options: TestRunOptions) -> Self {
        self.options = options;
        self
    }
}

/// How a test command ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestExit {
    pub success: bool,
    /// Exit code when the process exited normally.
    pub code: Option<i32>,
    /// Signal number when the process was terminated by a signal.
    pub signal: Option<i32>,
}

/// Captured result of one test command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRun {
    pub exit: TestExit,
    pub stdout: String,
    pub stderr: String,
}

/// Spawns a test command. agent-spec never retries a run: a flaky runner is
/// reported (as `runner-exited`), and retrying is the caller's decision.
pub trait TestCommandRunner: Send + Sync {
    fn run(&self, args: &[String], cwd: &Path) -> SpecResult<TestRun>;
}

/// The production runner: `cargo <args>` in the workspace root.
pub struct CargoRunner;

impl TestCommandRunner for CargoRunner {
    fn run(&self, args: &[String], cwd: &Path) -> SpecResult<TestRun> {
        let output = Command::new("cargo")
            .args(args)
            .current_dir(cwd)
            .output()
            .map_err(|err| SpecError::Verification(format!("failed to run cargo test: {err}")))?;
        Ok(TestRun {
            exit: TestExit {
                success: output.status.success(),
                code: output.status.code(),
                signal: exit_signal(&output.status),
            },
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingSource {
    ExplicitScenarioSelector,
    LegacyComment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestBinding {
    selector: TestSelector,
    source: BindingSource,
}

impl Verifier for TestVerifier {
    fn name(&self) -> &str {
        "test"
    }

    fn verify(&self, ctx: &VerificationContext) -> SpecResult<Vec<ScenarioResult>> {
        let Some(workspace_root) = find_workspace_root(&ctx.code_paths) else {
            return Ok(Vec::new());
        };

        let legacy_bindings = collect_legacy_comment_bindings(&ctx.code_paths)?;
        let bound = ctx
            .resolved_spec
            .all_scenarios
            .iter()
            .filter_map(|scenario| {
                resolve_test_binding(scenario, &legacy_bindings).map(|binding| (scenario, binding))
            })
            .collect::<Vec<_>>();

        let outcomes = if self.options.no_batch {
            None
        } else if let Some(prefetched) = &self.prefetched {
            Some(Arc::clone(prefetched))
        } else {
            let plan = TestBatchPlan::build(
                bound.iter().map(|(scenario, binding)| {
                    (&binding.selector, scenario.mode == ScenarioMode::Soak)
                }),
                &self.options,
            );
            Some(Arc::new(execute_plan(
                &plan,
                self.runner.as_ref(),
                &workspace_root,
                &self.options,
            )?))
        };

        let mut results = Vec::new();
        for (scenario, binding) in &bound {
            let soak = scenario.mode == ScenarioMode::Soak;
            if soak && !self.options.include_soak {
                results.push(deferred_result(scenario));
                continue;
            }
            let group = outcomes.as_ref().and_then(|outcomes| {
                outcomes
                    .groups
                    .get(&GroupKey::for_selector(&binding.selector, soak))
            });
            let result = match group {
                Some(group) if !group.needs_fallback() => batched_result(scenario, binding, group),
                // --no-batch, an unparseable group, or a group a prefetch
                // did not cover: run this scenario on its own.
                _ => self.run_one(scenario, binding, &workspace_root)?,
            };
            results.push(result);
        }

        Ok(results)
    }
}

impl TestVerifier {
    /// One `cargo test -q` for one scenario (the pre-batching invocation).
    fn run_one(
        &self,
        scenario: &Scenario,
        binding: &TestBinding,
        workspace_root: &Path,
    ) -> SpecResult<ScenarioResult> {
        let mut args = build_cargo_test_args(&binding.selector);
        if scenario.mode == ScenarioMode::Soak {
            // A soak test may stay #[ignore]d for plain `cargo test`.
            args.extend(["--".to_string(), "--include-ignored".to_string()]);
        }
        let started = Instant::now();
        let run = self.runner.run(&args, workspace_root)?;
        let duration_ms = started.elapsed().as_millis() as u64;

        let combined = combine_output(&run.stdout, &run.stderr);
        let judgment = judge_test_run(&run.exit, &combined, scenario.review);
        let evidence = test_output_evidence(&binding.selector, combined, &run.exit, &judgment);
        Ok(scenario_result(
            scenario,
            binding,
            &judgment,
            evidence,
            duration_ms,
        ))
    }
}

fn scenario_result(
    scenario: &Scenario,
    binding: &TestBinding,
    judgment: &TestJudgment,
    evidence: Evidence,
    duration_ms: u64,
) -> ScenarioResult {
    let reason = judgment_reason(judgment, &binding.selector.label(), binding.source);
    ScenarioResult {
        scenario_name: scenario.name.clone(),
        verdict: judgment.verdict,
        step_results: scenario
            .steps
            .iter()
            .map(|step| StepVerdict {
                step_text: step.text.clone(),
                verdict: judgment.verdict,
                reason: reason.clone(),
            })
            .collect(),
        evidence: vec![evidence],
        duration_ms,
        provenance: None,
    }
}

/// A scenario's result read from its group's batched run.
fn batched_result(
    scenario: &Scenario,
    binding: &TestBinding,
    group: &GroupOutcome,
) -> ScenarioResult {
    let batched = attribute(&binding.selector.filter, scenario.review, group);
    // The group's exit status describes the whole group; it belongs in the
    // evidence of a scenario that did not pass, not of one that did.
    let exit = if matches!(
        batched.judgment.verdict,
        Verdict::Pass | Verdict::PendingReview
    ) {
        TestExit {
            success: true,
            code: None,
            signal: None,
        }
    } else {
        group.run.exit
    };
    let mut evidence =
        test_output_evidence(&binding.selector, batched.output, &exit, &batched.judgment);
    if let Evidence::TestOutput { detail, .. } = &mut evidence {
        detail.duration_estimated = batched.duration_estimated;
    }
    scenario_result(
        scenario,
        binding,
        &batched.judgment,
        evidence,
        batched.duration_ms,
    )
}

/// A soak scenario the run does not execute: reported, never run.
fn deferred_result(scenario: &Scenario) -> ScenarioResult {
    let reason = "soak scenario deferred; run with --include-soak".to_string();
    ScenarioResult {
        scenario_name: scenario.name.clone(),
        verdict: Verdict::Deferred,
        step_results: scenario
            .steps
            .iter()
            .map(|step| StepVerdict {
                step_text: step.text.clone(),
                verdict: Verdict::Deferred,
                reason: reason.clone(),
            })
            .collect(),
        evidence: Vec::new(),
        duration_ms: 0,
        provenance: None,
    }
}

pub(crate) fn combine_output(stdout: &str, stderr: &str) -> String {
    if stderr.trim().is_empty() {
        stdout.to_string()
    } else if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    }
}

fn find_workspace_root(code_paths: &[PathBuf]) -> Option<PathBuf> {
    for path in code_paths {
        let mut current = if path.is_file() {
            path.parent()?.to_path_buf()
        } else {
            path.clone()
        };

        loop {
            if current.join("Cargo.toml").is_file() {
                return Some(current);
            }
            if !current.pop() {
                break;
            }
        }
    }

    None
}

fn collect_legacy_comment_bindings(code_paths: &[PathBuf]) -> SpecResult<HashMap<String, String>> {
    let mut bindings = HashMap::new();
    let mut files = Vec::new();

    for path in code_paths {
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            collect_rust_files(path, &mut files);
        }
    }

    for file in files {
        let content = fs::read_to_string(&file)?;
        for (scenario, test_name) in extract_bindings(&content) {
            bindings.entry(scenario).or_insert(test_name);
        }
    }

    Ok(bindings)
}

fn resolve_test_binding(
    scenario: &Scenario,
    legacy_bindings: &HashMap<String, String>,
) -> Option<TestBinding> {
    if let Some(selector) = scenario.test_selector.as_ref() {
        return Some(TestBinding {
            selector: selector.clone(),
            source: BindingSource::ExplicitScenarioSelector,
        });
    }

    legacy_bindings
        .get(&scenario.name)
        .map(|selector| TestBinding {
            selector: TestSelector::filter_only(selector.clone()),
            source: BindingSource::LegacyComment,
        })
}

fn build_cargo_test_args(selector: &TestSelector) -> Vec<String> {
    let mut args = vec!["test".to_string(), "-q".to_string()];
    if let Some(package) = &selector.package {
        args.push("-p".to_string());
        args.push(package.clone());
    }
    if let Some(features) = selector_features(selector) {
        args.push("--features".to_string());
        args.push(features.to_string());
    }
    args.push(selector.filter.clone());
    args
}

/// The selector's `Features:` value, or `None` when absent or blank (a blank
/// value is a lint warning, never an empty `--features` argument).
pub(crate) fn selector_features(selector: &TestSelector) -> Option<&str> {
    selector
        .features
        .as_deref()
        .map(str::trim)
        .filter(|features| !features.is_empty())
}

fn test_output_evidence(
    selector: &TestSelector,
    stdout: String,
    exit: &TestExit,
    judgment: &TestJudgment,
) -> Evidence {
    Evidence::TestOutput {
        test_name: selector.label(),
        stdout,
        passed: matches!(judgment.verdict, Verdict::Pass | Verdict::PendingReview),
        package: selector.package.clone(),
        level: selector.level.clone(),
        test_double: selector.test_double.clone(),
        targets: selector.targets.clone(),
        detail: TestOutputDetail {
            features: selector_features(selector).map(str::to_string),
            exit_code: exit.code,
            signal: exit.signal,
            ignored: judgment
                .summary
                .map(|summary| summary.ignored)
                .filter(|ignored| *ignored > 0),
            failure_class: judgment.failure_class.map(str::to_string),
            duration_estimated: false,
        },
    }
}

/// Totals of every libtest `test result:` line in a cargo run (one line per
/// test binary).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LibtestSummary {
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) ignored: usize,
}

impl LibtestSummary {
    /// Tests that actually ran. An ignored test is matched but never run.
    pub(crate) fn executed(&self) -> usize {
        self.passed + self.failed
    }
}

/// Sum the `test result:` summary lines; `None` when there is none. libtest
/// prints `running 1 test` for a filter that matches only an `#[ignore]`d
/// test, so the `running` lines cannot tell executed from ignored.
pub(crate) fn parse_libtest_summary(output: &str) -> Option<LibtestSummary> {
    let mut total: Option<LibtestSummary> = None;
    for line in output.lines() {
        let Some(rest) = line.trim().strip_prefix("test result:") else {
            continue;
        };
        let entry = total.get_or_insert_with(LibtestSummary::default);
        for part in rest.split([';', '.']) {
            let mut words = part.split_whitespace();
            let (Some(count), Some(label)) = (words.next(), words.next()) else {
                continue;
            };
            let Ok(count) = count.parse::<usize>() else {
                continue;
            };
            match label {
                "passed" => entry.passed += count,
                "failed" => entry.failed += count,
                "ignored" => entry.ignored += count,
                _ => {}
            }
        }
    }
    total
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JudgmentKind {
    /// Matched tests ran and all passed.
    Covered,
    /// At least one matched test ran and failed.
    Failed,
    /// Every matched test is `#[ignore]`d, so none ran.
    AllIgnored,
    /// The selector matched no test.
    ZeroMatched,
    /// cargo succeeded but printed no `test result:` line.
    NoSummary,
    /// cargo exited non-zero before any test ran.
    NotRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TestJudgment {
    pub(crate) verdict: Verdict,
    pub(crate) kind: JudgmentKind,
    pub(crate) summary: Option<LibtestSummary>,
    /// `compile-error` or `runner-exited` for a run that never reached a test.
    pub(crate) failure_class: Option<&'static str>,
}

/// Judge a cargo test run from its exit and libtest's summary. A scenario
/// passes only when at least one matched test executed; a missing or
/// malformed summary is `uncertain`, never `pass`.
pub(crate) fn judge_test_run(exit: &TestExit, output: &str, review: ReviewMode) -> TestJudgment {
    let summary = parse_libtest_summary(output);
    let executed = summary.map_or(0, |summary| summary.executed());
    let ignored = summary.map_or(0, |summary| summary.ignored);
    let judgment = |verdict, kind, failure_class| TestJudgment {
        verdict,
        kind,
        summary,
        failure_class,
    };
    if !exit.success {
        return if executed > 0 {
            // A test built, ran, and failed → a genuinely disproven claim.
            judgment(Verdict::Fail, JudgmentKind::Failed, None)
        } else {
            // cargo never reached a test: a build, toolchain, or runner
            // failure, not a disproven claim.
            judgment(
                Verdict::Uncertain,
                JudgmentKind::NotRun,
                Some(not_run_class(output)),
            )
        };
    }
    match (summary, executed, ignored) {
        (None, _, _) => judgment(Verdict::Uncertain, JudgmentKind::NoSummary, None),
        (Some(_), 0, 0) => judgment(Verdict::Skip, JudgmentKind::ZeroMatched, None),
        (Some(_), 0, _) => judgment(Verdict::Skip, JudgmentKind::AllIgnored, None),
        // Human/AI-reviewed scenarios are judged out of band.
        _ if review == ReviewMode::Human => {
            judgment(Verdict::PendingReview, JudgmentKind::Covered, None)
        }
        _ => judgment(Verdict::Pass, JudgmentKind::Covered, None),
    }
}

/// A compile error never deserves a retry; a runner that exited without
/// cargo output usually is an environment problem worth one.
fn not_run_class(output: &str) -> &'static str {
    let compile_error = output.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("error[E") || line.contains("could not compile")
    });
    if compile_error {
        "compile-error"
    } else {
        "runner-exited"
    }
}

pub(crate) fn judgment_reason(
    judgment: &TestJudgment,
    selector_label: &str,
    source: BindingSource,
) -> String {
    let binding = match source {
        BindingSource::ExplicitScenarioSelector => "explicit test",
        BindingSource::LegacyComment => "legacy @spec test",
    };
    match judgment.kind {
        JudgmentKind::Covered => format!("covered by {binding} `{selector_label}`"),
        JudgmentKind::Failed => format!("{binding} `{selector_label}` failed"),
        JudgmentKind::AllIgnored => format!(
            "test selector `{selector_label}` matched only ignored tests: bound test is #[ignore]d"
        ),
        JudgmentKind::ZeroMatched => {
            format!("test selector `{selector_label}` matched zero tests")
        }
        JudgmentKind::NoSummary => format!(
            "could not judge `{selector_label}`: cargo printed no libtest `test result:` summary"
        ),
        JudgmentKind::NotRun => format!(
            "could not run `{selector_label}`: cargo exited before any test ran ({})",
            judgment.failure_class.unwrap_or("runner-exited")
        ),
    }
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && (name.starts_with('.') || name == "target")
            {
                continue;
            }
            collect_rust_files(&path, files);
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn extract_bindings(source: &str) -> Vec<(String, String)> {
    let mut bindings = Vec::new();
    let mut pending_specs = Vec::new();
    let mut saw_test_attr = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(spec_name) = trimmed
            .strip_prefix("// @spec:")
            .or_else(|| trimmed.strip_prefix("/// @spec:"))
        {
            pending_specs.push(spec_name.trim().to_string());
            continue;
        }

        if trimmed.starts_with("#[test]") || trimmed.starts_with("#[tokio::test") {
            saw_test_attr = true;
            continue;
        }

        if saw_test_attr && trimmed.starts_with("fn ") {
            if let Some(test_name) = extract_fn_name(trimmed) {
                for spec_name in pending_specs.drain(..) {
                    bindings.push((spec_name, test_name.clone()));
                }
            }
            saw_test_attr = false;
            continue;
        }

        if !trimmed.starts_with("#[") && !trimmed.is_empty() {
            pending_specs.clear();
            saw_test_attr = false;
        }
    }

    bindings
}

fn extract_fn_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("fn ")?;
    let name = rest.split('(').next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use crate::spec_core::{ReviewMode, Scenario, Span, TestSelector, Verdict};

    use super::{
        BindingSource, TestCommandRunner, TestExit, TestRun, TestRunOptions, TestVerifier,
        build_cargo_test_args, extract_bindings, judge_test_run, judgment_reason,
        parse_libtest_summary, resolve_test_binding, test_output_evidence,
    };
    use crate::spec_verify::Verifier;

    fn selector(package: Option<&str>, features: Option<&str>) -> TestSelector {
        TestSelector {
            package: package.map(str::to_string),
            features: features.map(str::to_string),
            ..TestSelector::filter_only("crash_recovery")
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn test_build_cargo_test_args_appends_features() {
        assert_eq!(
            build_cargo_test_args(&selector(Some("amberlog"), Some("fault-injection"))),
            strings(&[
                "test",
                "-q",
                "-p",
                "amberlog",
                "--features",
                "fault-injection",
                "crash_recovery"
            ])
        );
    }

    #[test]
    fn test_build_cargo_test_args_without_features_is_unchanged() {
        assert_eq!(
            build_cargo_test_args(&selector(Some("amberlog"), None)),
            strings(&["test", "-q", "-p", "amberlog", "crash_recovery"])
        );
    }

    #[test]
    fn test_empty_selector_features_warns_and_passes_no_flag() {
        let args = build_cargo_test_args(&selector(None, Some("  ")));
        assert!(!args.iter().any(|arg| arg == "--features"), "{args:?}");

        let doc = crate::spec_parser::parse_spec_from_str(
            "spec: task\nname: \"Empty\"\n---\n\n## Completion Criteria\n\nScenario: empty features\n  Test:\n    Filter: crash_recovery\n    Features:\n  Given a selector\n  When it is linted\n  Then a warning is reported\n",
        )
        .unwrap();
        let report = crate::spec_lint::LintPipeline::with_defaults().run(&doc);
        let empty = report
            .diagnostics
            .iter()
            .filter(|diag| diag.rule == "selector-features-empty")
            .collect::<Vec<_>>();
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].severity, crate::spec_core::Severity::Warning);
    }

    #[test]
    fn test_test_output_evidence_records_selector_features() {
        let ok = exit(0);
        let judgment = judge_test_run(
            &ok,
            &format!("running 1 test\n.\n{}\n", summary_line(1, 0, 0)),
            ReviewMode::Auto,
        );
        let with = test_output_evidence(
            &selector(None, Some("probe")),
            String::new(),
            &ok,
            &judgment,
        );
        let json = serde_json::to_value(&with).unwrap();
        assert_eq!(json["features"], "probe");

        let without = test_output_evidence(&selector(None, None), String::new(), &ok, &judgment);
        let json = serde_json::to_value(&without).unwrap();
        assert!(json.get("features").is_none());
    }

    fn exit(code: i32) -> TestExit {
        TestExit {
            success: code == 0,
            code: Some(code),
            signal: None,
        }
    }

    fn summary_line(passed: usize, failed: usize, ignored: usize) -> String {
        let status = if failed > 0 { "FAILED" } else { "ok" };
        format!(
            "test result: {status}. {passed} passed; {failed} failed; {ignored} ignored; 0 measured; 3 filtered out; finished in 0.00s"
        )
    }

    #[test]
    fn test_cargo_test_executed_count_distinguishes_zero_match_from_mixed_targets() {
        // A filter that matches nothing in any binary executes zero tests,
        // and so does not pass.
        let zero = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out\n";
        assert_eq!(parse_libtest_summary(zero).unwrap().executed(), 0);
        assert_eq!(
            judge_test_run(&exit(0), zero, ReviewMode::Auto).verdict,
            Verdict::Skip
        );
        // Mixed targets: one binary ran the test, another matched nothing.
        let mixed = format!(
            "running 1 test\n.\n{}\n\nrunning 0 tests\n{}\n",
            summary_line(1, 0, 0),
            summary_line(0, 0, 0)
        );
        assert_eq!(parse_libtest_summary(&mixed).unwrap().executed(), 1);
        assert_eq!(
            judge_test_run(&exit(0), &mixed, ReviewMode::Auto).verdict,
            Verdict::Pass
        );
    }

    #[test]
    fn test_parse_libtest_summary_sums_binaries_and_separates_ignored() {
        let output = format!(
            "running 3 tests\n..i\n{}\n\nrunning 1 test\n.\n{}\n",
            summary_line(2, 0, 1),
            summary_line(1, 0, 0)
        );
        let summary = parse_libtest_summary(&output).unwrap();
        assert_eq!(summary.passed, 3);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.ignored, 1);
        assert_eq!(summary.executed(), 3);
        assert_eq!(parse_libtest_summary("running 1 test\n.\n"), None);
    }

    #[test]
    fn test_verdict_is_skip_when_every_matched_test_is_ignored() {
        let output = format!("running 1 test\ni\n{}\n", summary_line(0, 0, 1));
        let judgment = judge_test_run(&exit(0), &output, ReviewMode::Auto);
        assert_eq!(judgment.verdict, Verdict::Skip);
        let reason = judgment_reason(
            &judgment,
            "tests::probe",
            BindingSource::ExplicitScenarioSelector,
        );
        assert!(reason.contains("bound test is #[ignore]d"), "{reason}");
        // Zero matches without ignored tests keeps the existing reason.
        let none = judge_test_run(
            &exit(0),
            &format!("running 0 tests\n{}\n", summary_line(0, 0, 0)),
            ReviewMode::Auto,
        );
        assert_eq!(none.verdict, Verdict::Skip);
        assert!(
            judgment_reason(&none, "x", BindingSource::ExplicitScenarioSelector)
                .contains("matched zero tests")
        );
    }

    #[test]
    fn test_verdict_never_passes_or_pends_with_zero_executed() {
        let output = format!("running 1 test\ni\n{}\n", summary_line(0, 0, 1));
        let judgment = judge_test_run(&exit(0), &output, ReviewMode::Human);
        assert_eq!(judgment.verdict, Verdict::Skip);
        let passing = judge_test_run(
            &exit(0),
            &format!("running 1 test\n.\n{}\n", summary_line(1, 0, 0)),
            ReviewMode::Human,
        );
        assert_eq!(passing.verdict, Verdict::PendingReview);
    }

    #[test]
    fn test_verdict_uses_executed_tests_when_some_are_ignored() {
        let selector = TestSelector::filter_only("probe");
        let passed_output = format!("running 2 tests\n.i\n{}\n", summary_line(1, 0, 1));
        let passed = judge_test_run(&exit(0), &passed_output, ReviewMode::Auto);
        assert_eq!(passed.verdict, Verdict::Pass);
        let failed_output = format!("running 2 tests\nFi\n{}\n", summary_line(0, 1, 1));
        let failed = judge_test_run(&exit(101), &failed_output, ReviewMode::Auto);
        assert_eq!(failed.verdict, Verdict::Fail);
        for (judgment, run_exit, output) in [
            (&passed, exit(0), passed_output.clone()),
            (&failed, exit(101), failed_output.clone()),
        ] {
            let evidence = test_output_evidence(&selector, output, &run_exit, judgment);
            let json = serde_json::to_value(&evidence).unwrap();
            assert_eq!(json["ignored"], 1);
        }
    }

    #[test]
    fn test_verdict_is_uncertain_without_summary_line() {
        let judgment = judge_test_run(&exit(0), "running 1 test\n.\n", ReviewMode::Auto);
        assert_eq!(judgment.verdict, Verdict::Uncertain);
    }

    #[test]
    fn test_uncertain_distinguishes_compile_error_from_runner_exit() {
        let selector = TestSelector::filter_only("probe");
        let compile_output = "error[E0425]: cannot find value `x` in this scope\nerror: could not compile `probe` (lib test) due to 1 previous error\n";
        let compile = judge_test_run(&exit(101), compile_output, ReviewMode::Auto);
        assert_eq!(compile.verdict, Verdict::Uncertain);
        assert_eq!(compile.failure_class, Some("compile-error"));

        let runner = judge_test_run(&exit(255), "", ReviewMode::Auto);
        assert_eq!(runner.verdict, Verdict::Uncertain);
        assert_eq!(runner.failure_class, Some("runner-exited"));
        let evidence = test_output_evidence(&selector, String::new(), &exit(255), &runner);
        let json = serde_json::to_value(&evidence).unwrap();
        assert_eq!(json["exit_code"], 255);
        assert_eq!(json["failure_class"], "runner-exited");
        assert!(json.get("ignored").is_none());
    }

    struct CountingRunner {
        calls: std::sync::atomic::AtomicUsize,
        exit: TestExit,
        stdout: String,
    }

    impl TestCommandRunner for CountingRunner {
        fn run(
            &self,
            _args: &[String],
            _cwd: &std::path::Path,
        ) -> crate::spec_core::SpecResult<TestRun> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(TestRun {
                exit: self.exit,
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    fn verify_with_runner(
        spec: &str,
        runner: std::sync::Arc<CountingRunner>,
    ) -> Vec<crate::spec_core::ScenarioResult> {
        let dir = std::env::temp_dir().join(format!(
            "agent-spec-test-verifier-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"probe\"\n").unwrap();
        let doc = crate::spec_parser::parse_spec_from_str(spec).unwrap();
        let ctx = crate::spec_verify::VerificationContext {
            code_paths: vec![dir.clone()],
            change_paths: Vec::new(),
            ai_mode: crate::spec_verify::AiMode::Off,
            resolved_spec: crate::spec_parser::resolve_spec(doc, &[]).unwrap(),
        };
        let results = TestVerifier::with_runner(runner).verify(&ctx).unwrap();
        std::fs::remove_dir_all(dir).ok();
        results
    }

    const ONE_SCENARIO_SPEC: &str = "spec: task\nname: \"Probe\"\n---\n\n## Completion Criteria\n\nScenario: probe case\n  Test: probe_case\n  Given a bound test\n  When it is verified\n  Then a verdict is recorded\n";

    struct RecordingRunner {
        calls: std::sync::Mutex<Vec<Vec<String>>>,
        stdout: String,
    }

    impl TestCommandRunner for RecordingRunner {
        fn run(
            &self,
            args: &[String],
            _cwd: &std::path::Path,
        ) -> crate::spec_core::SpecResult<TestRun> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(TestRun {
                exit: exit(101),
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    const SOAK_SPEC: &str = "spec: task\nname: \"Soak\"\n---\n\n## Completion Criteria\n\nScenario: long run\n  Mode: soak\n  Test: soak_case\n  Given a long-running test\n  When it is verified\n  Then it is deferred by default\n";

    fn verify_soak(
        options: TestRunOptions,
    ) -> (Vec<Vec<String>>, Vec<crate::spec_core::ScenarioResult>) {
        let runner = std::sync::Arc::new(RecordingRunner {
            calls: Default::default(),
            stdout: format!(
                "running 1 test\ntest tests::soak_case ... FAILED\n{}\n",
                summary_line(0, 1, 0)
            ),
        });
        let dir = std::env::temp_dir().join(format!(
            "agent-spec-soak-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"probe\"\n").unwrap();
        let doc = crate::spec_parser::parse_spec_from_str(SOAK_SPEC).unwrap();
        let ctx = crate::spec_verify::VerificationContext {
            code_paths: vec![dir.clone()],
            change_paths: Vec::new(),
            ai_mode: crate::spec_verify::AiMode::Off,
            resolved_spec: crate::spec_parser::resolve_spec(doc, &[]).unwrap(),
        };
        let results = TestVerifier::with_runner(runner.clone())
            .with_options(options)
            .verify(&ctx)
            .unwrap();
        std::fs::remove_dir_all(dir).ok();
        let calls = runner.calls.lock().unwrap().clone();
        (calls, results)
    }

    #[test]
    fn test_soak_scenario_is_deferred_without_running_tests() {
        let (calls, results) = verify_soak(TestRunOptions::default());
        assert!(calls.is_empty(), "{calls:?}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, Verdict::Deferred);
        assert!(results[0].step_results[0].reason.contains("--include-soak"));
    }

    #[test]
    fn test_include_soak_runs_with_include_ignored() {
        let (calls, results) = verify_soak(TestRunOptions {
            include_soak: true,
            no_batch: true,
            ..TestRunOptions::default()
        });
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].ends_with(&strings(&["soak_case", "--", "--include-ignored"])),
            "{:?}",
            calls[0]
        );
        assert_eq!(results[0].verdict, Verdict::Fail);
    }

    struct ScriptRunner {
        calls: std::sync::Mutex<Vec<Vec<String>>>,
        exit_code: i32,
        stdout: String,
    }

    impl TestCommandRunner for ScriptRunner {
        fn run(
            &self,
            args: &[String],
            _cwd: &std::path::Path,
        ) -> crate::spec_core::SpecResult<TestRun> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(TestRun {
                exit: exit(self.exit_code),
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    const TWO_SCENARIO_SPEC: &str = "spec: task\nname: \"Two\"\n---\n\n## Completion Criteria\n\nScenario: first case\n  Test:\n    Package: probe\n    Filter: alpha\n  Given a\n  When b\n  Then c\n\nScenario: second case\n  Test:\n    Package: probe\n    Filter: beta\n  Given a\n  When b\n  Then c\n";

    fn verify_two(stdout: &str, options: TestRunOptions) -> Vec<Vec<String>> {
        let runner = std::sync::Arc::new(ScriptRunner {
            calls: Default::default(),
            exit_code: 0,
            stdout: stdout.to_string(),
        });
        let dir = std::env::temp_dir().join(format!(
            "agent-spec-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"probe\"\n").unwrap();
        let doc = crate::spec_parser::parse_spec_from_str(TWO_SCENARIO_SPEC).unwrap();
        let ctx = crate::spec_verify::VerificationContext {
            code_paths: vec![dir.clone()],
            change_paths: Vec::new(),
            ai_mode: crate::spec_verify::AiMode::Off,
            resolved_spec: crate::spec_parser::resolve_spec(doc, &[]).unwrap(),
        };
        TestVerifier::with_runner(runner.clone())
            .with_options(options)
            .verify(&ctx)
            .unwrap();
        std::fs::remove_dir_all(dir).ok();
        runner.calls.lock().unwrap().clone()
    }

    #[test]
    fn test_batch_unparseable_output_falls_back_to_per_scenario_runs() {
        // A custom harness: success, no per-test lines, no libtest summary.
        let calls = verify_two("custom harness: all good\n", TestRunOptions::default());
        assert_eq!(calls.len(), 3, "{calls:?}");
        assert!(!calls[0].iter().any(|arg| arg == "-q"), "{:?}", calls[0]);
        for call in &calls[1..] {
            assert!(call.iter().any(|arg| arg == "-q"), "{call:?}");
        }
    }

    #[test]
    fn test_no_batch_runs_each_scenario_separately() {
        let calls = verify_two(
            &format!("running 1 test\n.\n{}\n", summary_line(1, 0, 0)),
            TestRunOptions {
                no_batch: true,
                ..TestRunOptions::default()
            },
        );
        assert_eq!(calls.len(), 2, "{calls:?}");
        for call in &calls {
            assert!(call.iter().any(|arg| arg == "-q"), "{call:?}");
        }
    }

    #[test]
    fn test_runner_exit_is_not_retried() {
        let runner = std::sync::Arc::new(CountingRunner {
            calls: Default::default(),
            exit: exit(255),
            stdout: String::new(),
        });
        let results = verify_with_runner(ONE_SCENARIO_SPEC, runner.clone());
        assert_eq!(runner.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].verdict, Verdict::Uncertain);
    }

    #[test]
    fn extracts_spec_bindings_from_test_comments() {
        let source = r#"
// @spec: 场景一
// @spec: 场景二
#[test]
fn test_example() {}
"#;

        let bindings = extract_bindings(source);
        assert_eq!(bindings.len(), 2);
        assert_eq!(
            bindings[0],
            ("场景一".to_string(), "test_example".to_string())
        );
        assert_eq!(
            bindings[1],
            ("场景二".to_string(), "test_example".to_string())
        );
    }

    #[test]
    fn ignores_comments_not_followed_by_a_test() {
        let source = r#"
// @spec: 场景一
fn helper() {}
"#;

        assert!(extract_bindings(source).is_empty());
    }

    #[test]
    fn test_explicit_scenario_selector_takes_precedence_over_legacy_comment_binding() {
        let scenario = Scenario {
            name: "场景一".into(),
            steps: Vec::new(),
            test_selector: Some(TestSelector::filter_only(
                "test_explicit_scenario_selector_takes_precedence_over_legacy_comment_binding",
            )),
            tags: Vec::new(),
            review: Default::default(),
            mode: Default::default(),
            depends_on: vec![],
            rule: None,
            span: Span::default(),
            unrecognized: Vec::new(),
        };
        let legacy = HashMap::from([("场景一".to_string(), "legacy_test_name".to_string())]);

        let binding = resolve_test_binding(&scenario, &legacy).unwrap();
        assert_eq!(
            binding.selector,
            TestSelector::filter_only(
                "test_explicit_scenario_selector_takes_precedence_over_legacy_comment_binding"
            )
        );
        assert_eq!(binding.source, BindingSource::ExplicitScenarioSelector);
    }

    #[test]
    fn test_legacy_comment_binding_is_used_when_no_explicit_selector_exists() {
        let scenario = Scenario {
            name: "场景一".into(),
            steps: Vec::new(),
            test_selector: None,
            tags: Vec::new(),
            review: Default::default(),
            mode: Default::default(),
            depends_on: vec![],
            rule: None,
            span: Span::default(),
            unrecognized: Vec::new(),
        };
        let legacy = HashMap::from([(
            "场景一".to_string(),
            "test_legacy_comment_binding_is_used_when_no_explicit_selector_exists".to_string(),
        )]);

        let binding = resolve_test_binding(&scenario, &legacy).unwrap();
        assert_eq!(
            binding.selector,
            TestSelector::filter_only(
                "test_legacy_comment_binding_is_used_when_no_explicit_selector_exists"
            )
        );
        assert_eq!(binding.source, BindingSource::LegacyComment);
    }

    #[test]
    fn test_build_cargo_test_command_with_package_selector() {
        let selector = TestSelector {
            package: Some("spec-parser".into()),
            filter: "test_parse_structured_test_selector_block".into(),
            level: None,
            test_double: None,
            targets: None,
            features: None,
        };

        let args = build_cargo_test_args(&selector);
        assert_eq!(
            args,
            vec![
                "test".to_string(),
                "-q".to_string(),
                "-p".to_string(),
                "spec-parser".to_string(),
                "test_parse_structured_test_selector_block".to_string(),
            ]
        );
    }
}
