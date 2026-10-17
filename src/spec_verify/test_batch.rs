//! Batched test execution (LEP-007 D7): one `cargo test` per
//! (package, features, soak) group instead of one per scenario, with each
//! scenario's verdict read back from libtest's per-test result lines.
//! With `--runner nextest` (LEP-008 D1) each group is one `cargo nextest run`
//! read back from its JUnit report; either way the same [`attribute`] judges
//! every scenario (LEP-008 D4).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::spec_core::{ReviewMode, SpecResult, TestSelector, Verdict};

use super::test_verifier::{
    JudgmentKind, LibtestSummary, TestCommandRunner, TestJudgment, TestRun, TestRunOptions,
    TestRunner, judge_test_run, not_run_class, parse_libtest_summary, selector_features,
};

/// Scenarios that share a key run in one cargo invocation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupKey {
    pub package: Option<String>,
    pub features: Option<String>,
    /// Soak groups run with `--include-ignored`; keeping them apart stops
    /// that flag from running other scenarios' ignored tests.
    pub soak: bool,
}

impl GroupKey {
    pub fn for_selector(selector: &TestSelector, soak: bool) -> Self {
        Self {
            package: selector.package.clone(),
            features: selector_features(selector).map(str::to_string),
            soak,
        }
    }
}

/// One cargo invocation's worth of filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchGroup {
    pub key: GroupKey,
    /// Distinct filters in first-seen order.
    pub filters: Vec<String>,
    /// Scenarios (bindings) the group serves; used to spread wall time.
    pub scenario_count: usize,
}

/// The groups a set of bound scenarios runs as.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestBatchPlan {
    pub groups: Vec<BatchGroup>,
}

impl TestBatchPlan {
    /// Group `(selector, soak)` bindings. Soak bindings are left out unless
    /// `options.include_soak`, since those scenarios are deferred, not run.
    pub fn build<'a>(
        bindings: impl IntoIterator<Item = (&'a TestSelector, bool)>,
        options: &TestRunOptions,
    ) -> Self {
        let mut groups: BTreeMap<GroupKey, BatchGroup> = BTreeMap::new();
        for (selector, soak) in bindings {
            if soak && !options.include_soak {
                continue;
            }
            let key = GroupKey::for_selector(selector, soak);
            let group = groups.entry(key.clone()).or_insert_with(|| BatchGroup {
                key,
                filters: Vec::new(),
                scenario_count: 0,
            });
            group.scenario_count += 1;
            if !group.filters.contains(&selector.filter) {
                group.filters.push(selector.filter.clone());
            }
        }
        Self {
            groups: groups.into_values().collect(),
        }
    }
}

/// `cargo test [-p P] [--features F] --no-fail-fast -- <filters…>
/// --test-threads=<N> [--include-ignored]`. No `-q`: the per-test result
/// lines are what the verdicts are read from. cargo takes one test name
/// before `--`, so the filters go to libtest after it.
pub fn group_args(group: &BatchGroup, options: &TestRunOptions) -> Vec<String> {
    debug_assert_eq!(options.runner, TestRunner::Cargo);
    let mut args = vec!["test".to_string()];
    if let Some(package) = &group.key.package {
        args.push("-p".to_string());
        args.push(package.clone());
    }
    if let Some(features) = &group.key.features {
        args.push("--features".to_string());
        args.push(features.clone());
    }
    args.push("--no-fail-fast".to_string());
    args.push("--".to_string());
    args.extend(group.filters.iter().cloned());
    args.push(format!(
        "--test-threads={}",
        options.test_threads.unwrap_or(1)
    ));
    if group.key.soak {
        args.push("--include-ignored".to_string());
    }
    args
}

/// The nextest profile agent-spec defines in its tool config file; a custom
/// profile inherits the project's `default` profile (test groups, overrides).
pub const NEXTEST_PROFILE: &str = "agent-spec";

/// `cargo nextest run [-p P] [--features F] --no-fail-fast --no-tests=pass
/// --test-threads <N> [--run-ignored all] --tool-config-file
/// agent-spec:<file> --profile agent-spec -E 'test(~f1) | …'`. The
/// expression's `~` is a substring match, as libtest's filters are; a group
/// whose filters match nothing succeeds with an empty report
/// (`--no-tests=pass`) and its scenarios are `zero-matched`.
pub fn nextest_group_args(
    group: &BatchGroup,
    options: &TestRunOptions,
    tool_config: &Path,
) -> Vec<String> {
    let mut args = vec!["nextest".to_string(), "run".to_string()];
    if let Some(package) = &group.key.package {
        args.push("-p".to_string());
        args.push(package.clone());
    }
    if let Some(features) = &group.key.features {
        args.push("--features".to_string());
        args.push(features.clone());
    }
    args.push("--no-fail-fast".to_string());
    args.push("--no-tests=pass".to_string());
    args.push("--test-threads".to_string());
    args.push(options.test_threads.unwrap_or(1).to_string());
    if group.key.soak {
        args.push("--run-ignored".to_string());
        args.push("all".to_string());
    }
    args.push("--tool-config-file".to_string());
    args.push(format!("{NEXTEST_PROFILE}:{}", tool_config.display()));
    args.push("--profile".to_string());
    args.push(NEXTEST_PROFILE.to_string());
    args.push("-E".to_string());
    args.push(
        group
            .filters
            .iter()
            .map(|filter| format!("test(~{filter})"))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    args
}

/// The tool config file for one nextest run: profile `agent-spec` writes its
/// JUnit report to `junit`.
pub fn nextest_tool_config(junit: &Path) -> String {
    let path = junit.display().to_string();
    let quoted = if path.contains('\'') {
        format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{path}'")
    };
    format!("[profile.{NEXTEST_PROFILE}.junit]\npath = {quoted}\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOutcome {
    Ok,
    Failed,
    Ignored,
}

/// One test's outcome: a libtest `test <path> ... <result>` line or a JUnit
/// `<testcase>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestLine {
    pub path: String,
    pub outcome: LineOutcome,
    /// `<1.500s>` suffix, when libtest reports per-test time; the JUnit
    /// `time` attribute.
    pub duration_ms: Option<u64>,
    /// The test binary (`<package>` or `<package>::<target>`), known for
    /// JUnit test cases; libtest lines do not carry it.
    pub binary_id: Option<String>,
    /// Failure output carried with the outcome (JUnit `<failure>` text);
    /// libtest failure output is read from the group output instead.
    pub detail: Option<String>,
    /// Index of the supplied report the outcome came from.
    pub report: Option<usize>,
}

pub fn parse_test_lines(output: &str) -> Vec<TestLine> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("test ")?;
            let (path, result) = rest.split_once(" ... ")?;
            let result = result.trim();
            let (status, time) = match result.split_once(" <") {
                Some((status, time)) => (status.trim(), Some(time)),
                None => (result, None),
            };
            let outcome = if status == "ok" {
                LineOutcome::Ok
            } else if status == "FAILED" {
                LineOutcome::Failed
            } else if status == "ignored" || status.starts_with("ignored,") {
                LineOutcome::Ignored
            } else {
                return None;
            };
            let duration_ms = time
                .and_then(|time| time.trim_end_matches('>').strip_suffix('s'))
                .and_then(|seconds| seconds.parse::<f64>().ok())
                .map(|seconds| (seconds * 1000.0).round() as u64);
            Some(TestLine {
                path: path.trim().to_string(),
                outcome,
                duration_ms,
                binary_id: None,
                detail: None,
                report: None,
            })
        })
        .collect()
}

/// Where a group's test outcomes came from; recorded in each scenario's
/// evidence as `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutcomeSource {
    Cargo,
    Nextest,
    /// Supplied JUnit reports (`--test-results`), by path.
    Recorded(Arc<Vec<String>>),
}

/// What one group's run produced.
#[derive(Debug, Clone)]
pub struct GroupOutcome {
    pub run: TestRun,
    pub output: String,
    pub lines: Vec<TestLine>,
    pub wall_ms: u64,
    pub scenario_count: usize,
    pub source: OutcomeSource,
    /// Set when the run never reached a test in a way the exit status alone
    /// does not show (nextest missing, no report written): every scenario of
    /// the group is `uncertain` with this class.
    pub not_run: Option<&'static str>,
}

impl GroupOutcome {
    /// Succeeded and reported tests, but printed no per-test line this parser
    /// recognizes (for example a `harness = false` target): read nothing
    /// from it and run the group's scenarios one by one instead. A summary
    /// that reports zero tests is an ordinary "matched nothing". Only cargo
    /// runs fall back; nextest and recorded outcomes are read as they are.
    pub fn needs_fallback(&self) -> bool {
        if self.source != OutcomeSource::Cargo || !self.run.exit.success || !self.lines.is_empty() {
            return false;
        }
        match parse_libtest_summary(&self.output) {
            Some(summary) => summary.executed() + summary.ignored > 0,
            None => true,
        }
    }
}

/// Every group's outcome, keyed by group.
#[derive(Debug, Clone, Default)]
pub struct BatchOutcomes {
    pub groups: BTreeMap<GroupKey, GroupOutcome>,
}

pub fn execute_plan(
    plan: &TestBatchPlan,
    runner: &dyn TestCommandRunner,
    cwd: &Path,
    options: &TestRunOptions,
) -> SpecResult<BatchOutcomes> {
    if options.runner == TestRunner::Nextest {
        return execute_plan_with_nextest(plan, runner, cwd, options);
    }
    let mut outcomes = BatchOutcomes::default();
    for group in &plan.groups {
        let started = Instant::now();
        let run = runner.run(&group_args(group, options), cwd)?;
        let wall_ms = started.elapsed().as_millis() as u64;
        let output = super::test_verifier::combine_output(&run.stdout, &run.stderr);
        outcomes.groups.insert(
            group.key.clone(),
            GroupOutcome {
                lines: parse_test_lines(&output),
                output,
                run,
                wall_ms,
                scenario_count: group.scenario_count,
                source: OutcomeSource::Cargo,
                not_run: None,
            },
        );
    }
    Ok(outcomes)
}

/// One `cargo nextest run` per group, each read back from the JUnit report
/// the tool config file points into a private temporary directory.
fn execute_plan_with_nextest(
    plan: &TestBatchPlan,
    runner: &dyn TestCommandRunner,
    cwd: &Path,
    options: &TestRunOptions,
) -> SpecResult<BatchOutcomes> {
    let mut outcomes = BatchOutcomes::default();
    if plan.groups.is_empty() {
        return Ok(outcomes);
    }
    let dir = NextestScratch::create()?;
    std::fs::write(&dir.tool_config, nextest_tool_config(&dir.junit))?;
    for group in &plan.groups {
        let _ = std::fs::remove_file(&dir.junit);
        let started = Instant::now();
        let run = runner.run(&nextest_group_args(group, options, &dir.tool_config), cwd)?;
        let wall_ms = started.elapsed().as_millis() as u64;
        let output = super::test_verifier::combine_output(&run.stdout, &run.stderr);
        let (lines, not_run) = if nextest_unavailable(&run, &output) {
            (Vec::new(), Some("runner-unavailable"))
        } else {
            match std::fs::read_to_string(&dir.junit)
                .ok()
                .and_then(|xml| super::junit::parse_junit(&xml).ok())
            {
                Some(lines) => (lines, None),
                // No usable report: the run never got to report on a test.
                None => (Vec::new(), Some(not_run_class(&output))),
            }
        };
        outcomes.groups.insert(
            group.key.clone(),
            GroupOutcome {
                lines,
                output,
                run,
                wall_ms,
                scenario_count: group.scenario_count,
                source: OutcomeSource::Nextest,
                not_run,
            },
        );
    }
    Ok(outcomes)
}

/// cargo could not find the `nextest` subcommand.
fn nextest_unavailable(run: &TestRun, output: &str) -> bool {
    !run.exit.success && output.contains("no such command: `nextest`")
}

/// A temporary directory for one nextest run, removed when dropped.
struct NextestScratch {
    root: PathBuf,
    tool_config: PathBuf,
    junit: PathBuf,
}

impl NextestScratch {
    fn create() -> SpecResult<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("agent-spec-nextest-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            tool_config: root.join("nextest.toml"),
            junit: root.join("junit.xml"),
            root,
        })
    }
}

impl Drop for NextestScratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A scenario's share of a group run.
#[derive(Debug, Clone)]
pub struct BatchedVerdict {
    pub judgment: TestJudgment,
    /// The scenario's own result lines (and the failure output of its
    /// failing tests), or the whole group output when no test ran.
    pub output: String,
    pub duration_ms: u64,
    pub duration_estimated: bool,
    /// Supplied reports (by index) the matched outcomes came from.
    pub reports: Vec<usize>,
}

/// Judge one scenario from its group's run, with the same rules as a
/// per-scenario run: the matched tests are those whose path contains the
/// filter (libtest's substring semantics).
pub fn attribute(filter: &str, review: ReviewMode, group: &GroupOutcome) -> BatchedVerdict {
    let estimated = || group.wall_ms / group.scenario_count.max(1) as u64;
    if let Some(class) = group.not_run {
        return BatchedVerdict {
            judgment: TestJudgment {
                verdict: Verdict::Uncertain,
                kind: JudgmentKind::NotRun,
                summary: None,
                failure_class: Some(class),
            },
            output: group.output.clone(),
            duration_ms: estimated(),
            duration_estimated: true,
            reports: Vec::new(),
        };
    }
    if group.lines.is_empty() && !group.run.exit.success {
        // Nothing ran (build, toolchain or runner failure): every scenario
        // in the group is uncertain, classed from the group output.
        return BatchedVerdict {
            judgment: judge_test_run(&group.run.exit, &group.output, review),
            output: group.output.clone(),
            duration_ms: estimated(),
            duration_estimated: true,
            reports: Vec::new(),
        };
    }
    let matched = group
        .lines
        .iter()
        .filter(|line| line.path.contains(filter))
        .collect::<Vec<_>>();
    let count = |outcome| {
        matched
            .iter()
            .filter(|line| line.outcome == outcome)
            .count()
    };
    let summary = LibtestSummary {
        passed: count(LineOutcome::Ok),
        failed: count(LineOutcome::Failed),
        ignored: count(LineOutcome::Ignored),
    };
    let (verdict, kind) = if matched.is_empty() {
        (Verdict::Skip, JudgmentKind::ZeroMatched)
    } else if summary.executed() == 0 {
        (Verdict::Skip, JudgmentKind::AllIgnored)
    } else if summary.failed > 0 {
        (Verdict::Fail, JudgmentKind::Failed)
    } else if review == ReviewMode::Human {
        (Verdict::PendingReview, JudgmentKind::Covered)
    } else {
        (Verdict::Pass, JudgmentKind::Covered)
    };
    let reported = matched
        .iter()
        .filter_map(|line| line.duration_ms)
        .collect::<Vec<_>>();
    let (duration_ms, duration_estimated) =
        if !matched.is_empty() && reported.len() == matched.len() {
            (reported.iter().sum(), false)
        } else {
            (estimated(), true)
        };
    let mut reports = matched
        .iter()
        .filter_map(|line| line.report)
        .collect::<Vec<_>>();
    reports.sort_unstable();
    reports.dedup();
    BatchedVerdict {
        judgment: TestJudgment {
            verdict,
            kind,
            summary: Some(summary),
            failure_class: None,
        },
        output: scenario_output(&matched, &group.output),
        duration_ms,
        duration_estimated,
        reports,
    }
}

/// The matched result lines, then libtest's captured output for each matched
/// failing test (`---- <path> stdout ----` up to the next section).
fn scenario_output(matched: &[&TestLine], group_output: &str) -> String {
    let mut out = matched
        .iter()
        .map(|line| {
            let status = match line.outcome {
                LineOutcome::Ok => "ok",
                LineOutcome::Failed => "FAILED",
                LineOutcome::Ignored => "ignored",
            };
            format!("test {} ... {status}", line.path)
        })
        .collect::<Vec<_>>();
    for line in matched
        .iter()
        .filter(|line| line.outcome == LineOutcome::Failed)
    {
        if let Some(detail) = &line.detail {
            out.push(format!("---- {} ----", line.path));
            out.extend(detail.lines().map(str::to_string));
            continue;
        }
        let header = format!("---- {} stdout ----", line.path);
        let mut in_section = false;
        for text in group_output.lines() {
            if text.trim() == header {
                in_section = true;
            } else if in_section && (text.starts_with("---- ") || text.trim() == "failures:") {
                break;
            }
            if in_section {
                out.push(text.to_string());
            }
        }
    }
    out.join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::spec_verify::test_verifier::TestExit;

    fn selector(package: &str, filter: &str, features: Option<&str>) -> TestSelector {
        TestSelector {
            package: Some(package.to_string()),
            features: features.map(str::to_string),
            ..TestSelector::filter_only(filter)
        }
    }

    pub(crate) struct ScriptedRunner {
        pub(crate) calls: Mutex<Vec<Vec<String>>>,
        pub(crate) exit_code: i32,
        pub(crate) stdout: String,
    }

    impl TestCommandRunner for ScriptedRunner {
        fn run(&self, args: &[String], _cwd: &Path) -> SpecResult<TestRun> {
            self.calls.lock().unwrap().push(args.to_vec());
            Ok(TestRun {
                exit: TestExit {
                    success: self.exit_code == 0,
                    code: Some(self.exit_code),
                    signal: None,
                },
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    fn group_outcome(
        exit_code: i32,
        output: &str,
        scenario_count: usize,
        wall_ms: u64,
    ) -> GroupOutcome {
        GroupOutcome {
            run: TestRun {
                exit: TestExit {
                    success: exit_code == 0,
                    code: Some(exit_code),
                    signal: None,
                },
                stdout: output.to_string(),
                stderr: String::new(),
            },
            output: output.to_string(),
            lines: parse_test_lines(output),
            wall_ms,
            scenario_count,
            source: OutcomeSource::Cargo,
            not_run: None,
        }
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn test_batch_plan_groups_scenarios_across_specs_by_package() {
        let spec_one = [
            selector("probe", "alpha", None),
            selector("probe", "beta", None),
            selector("probe", "gamma", None),
        ];
        let spec_two = [
            selector("probe", "delta", None),
            selector("probe", "alpha", None),
        ];
        let options = TestRunOptions::default();
        let plan = TestBatchPlan::build(
            spec_one.iter().chain(spec_two.iter()).map(|s| (s, false)),
            &options,
        );
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(
            plan.groups[0].filters,
            strings(&["alpha", "beta", "gamma", "delta"])
        );
        assert_eq!(plan.groups[0].scenario_count, 5);

        let runner = ScriptedRunner {
            calls: Default::default(),
            exit_code: 0,
            stdout: "test result: ok. 0 passed; 0 failed; 0 ignored".into(),
        };
        execute_plan(&plan, &runner, Path::new("."), &options).unwrap();
        assert_eq!(runner.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_batch_plan_splits_groups_by_features_and_soak() {
        let plain = [
            selector("probe", "alpha", None),
            selector("probe", "beta", None),
        ];
        let featured = selector("probe", "gamma", Some("slow-io"));
        let soak = selector("probe", "delta", None);
        let options = TestRunOptions {
            include_soak: true,
            ..TestRunOptions::default()
        };
        let bindings = plain
            .iter()
            .map(|s| (s, false))
            .chain([(&featured, false), (&soak, true)]);
        let plan = TestBatchPlan::build(bindings, &options);
        assert_eq!(plan.groups.len(), 3);
        for group in &plan.groups {
            let args = group_args(group, &options);
            assert_eq!(
                args.iter().any(|arg| arg == "--include-ignored"),
                group.key.soak,
                "{args:?}"
            );
        }
        // Without include_soak the soak binding is not planned at all.
        let deferred = TestBatchPlan::build([(&soak, true)], &TestRunOptions::default());
        assert!(deferred.groups.is_empty());
    }

    #[test]
    fn test_batch_group_args_shape() {
        let group = BatchGroup {
            key: GroupKey {
                package: Some("probe".into()),
                features: None,
                soak: false,
            },
            filters: strings(&["alpha", "beta"]),
            scenario_count: 2,
        };
        assert_eq!(
            group_args(&group, &TestRunOptions::default()),
            strings(&[
                "test",
                "-p",
                "probe",
                "--no-fail-fast",
                "--",
                "alpha",
                "beta",
                "--test-threads=1"
            ])
        );
        let four = TestRunOptions {
            test_threads: Some(4),
            ..TestRunOptions::default()
        };
        assert_eq!(
            group_args(&group, &four).last().map(String::as_str),
            Some("--test-threads=4")
        );
    }

    fn nextest_options(threads: Option<usize>) -> TestRunOptions {
        TestRunOptions {
            runner: TestRunner::Nextest,
            test_threads: threads,
            ..TestRunOptions::default()
        }
    }

    fn group(
        package: Option<&str>,
        features: Option<&str>,
        soak: bool,
        filters: &[&str],
    ) -> BatchGroup {
        BatchGroup {
            key: GroupKey {
                package: package.map(str::to_string),
                features: features.map(str::to_string),
                soak,
            },
            filters: strings(filters),
            scenario_count: filters.len(),
        }
    }

    fn adjacent(args: &[String], first: &str, second: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == first && pair[1] == second)
    }

    #[test]
    fn test_default_runner_args_do_not_mention_nextest() {
        let args = group_args(
            &group(Some("alpha"), None, false, &["case_a"]),
            &TestRunOptions::default(),
        );
        assert_eq!(&args[..3], ["test", "-p", "alpha"]);
        assert!(!args.iter().any(|arg| arg.contains("nextest")), "{args:?}");
    }

    #[test]
    fn test_nextest_group_args_shape() {
        let config = Path::new("/tmp/agent-spec-nextest/nextest.toml");
        let args = nextest_group_args(
            &group(Some("alpha"), Some("fi"), false, &["case_a", "case_b"]),
            &nextest_options(None),
            config,
        );
        assert_eq!(&args[..2], ["nextest", "run"]);
        assert!(adjacent(&args, "-p", "alpha"), "{args:?}");
        assert!(adjacent(&args, "--features", "fi"), "{args:?}");
        assert!(args.iter().any(|arg| arg == "--no-fail-fast"), "{args:?}");
        assert!(
            adjacent(&args, "-E", "test(~case_a) | test(~case_b)"),
            "{args:?}"
        );
    }

    #[test]
    fn test_nextest_group_args_threads() {
        let config = Path::new("/tmp/c.toml");
        let g = group(Some("alpha"), None, false, &["case_a"]);
        assert!(adjacent(
            &nextest_group_args(&g, &nextest_options(None), config),
            "--test-threads",
            "1"
        ));
        assert!(adjacent(
            &nextest_group_args(&g, &nextest_options(Some(8)), config),
            "--test-threads",
            "8"
        ));
    }

    #[test]
    fn test_nextest_group_args_run_ignored_only_for_soak() {
        let config = Path::new("/tmp/c.toml");
        let soak = nextest_group_args(
            &group(Some("alpha"), None, true, &["case_a"]),
            &nextest_options(None),
            config,
        );
        assert!(adjacent(&soak, "--run-ignored", "all"), "{soak:?}");
        let plain = nextest_group_args(
            &group(Some("alpha"), None, false, &["case_a"]),
            &nextest_options(None),
            config,
        );
        assert!(!plain.iter().any(|arg| arg == "--run-ignored"), "{plain:?}");
    }

    #[test]
    fn test_nextest_tool_config_points_junit_into_scratch() {
        let config = Path::new("/tmp/agent-spec-nextest-1/nextest.toml");
        let args = nextest_group_args(
            &group(Some("alpha"), None, false, &["case_a"]),
            &nextest_options(None),
            config,
        );
        assert!(
            adjacent(
                &args,
                "--tool-config-file",
                "agent-spec:/tmp/agent-spec-nextest-1/nextest.toml"
            ),
            "{args:?}"
        );
        assert!(adjacent(&args, "--profile", "agent-spec"), "{args:?}");

        let scratch = NextestScratch::create().unwrap();
        let text = nextest_tool_config(&scratch.junit);
        assert!(text.starts_with("[profile.agent-spec.junit]\n"), "{text}");
        assert!(
            text.contains(&format!("path = '{}'", scratch.junit.display())),
            "{text}"
        );
        assert!(scratch.junit.starts_with(&scratch.root));
        assert!(scratch.root.starts_with(std::env::temp_dir()));
        let root = scratch.root.clone();
        drop(scratch);
        assert!(!root.exists());
    }

    const GROUP_OUTPUT: &str = "running 4 tests\ntest tests::alpha_one ... ok\ntest tests::beta_one ... FAILED\ntest tests::beta_two ... ok\ntest tests::delta_one ... ignored, slow\n\nfailures:\n\n---- tests::beta_one stdout ----\nthread 'tests::beta_one' panicked at src/lib.rs:9:5:\nbeta broke\n\nfailures:\n    tests::beta_one\n\ntest result: FAILED. 2 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out\n";

    #[test]
    fn test_batch_attribution_by_filter_substring() {
        let group = group_outcome(101, GROUP_OUTPUT, 4, 400);
        let verdicts = ["alpha", "beta", "gamma", "delta"]
            .map(|filter| attribute(filter, ReviewMode::Auto, &group));
        assert_eq!(
            verdicts.each_ref().map(|v| v.judgment.verdict),
            [Verdict::Pass, Verdict::Fail, Verdict::Skip, Verdict::Skip]
        );
        assert_eq!(verdicts[3].judgment.kind, JudgmentKind::AllIgnored);
        let reason = super::super::test_verifier::judgment_reason(
            &verdicts[3].judgment,
            "delta",
            super::super::test_verifier::BindingSource::ExplicitScenarioSelector,
        );
        assert!(reason.contains("bound test is #[ignore]d"), "{reason}");
        assert!(
            verdicts[1].output.contains("beta broke"),
            "{}",
            verdicts[1].output
        );
        assert!(
            !verdicts[0].output.contains("beta"),
            "{}",
            verdicts[0].output
        );
    }

    #[test]
    fn test_batch_attribution_keeps_pending_review_for_human_scenarios() {
        let group = group_outcome(
            0,
            "running 1 test\ntest tests::alpha_one ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored\n",
            1,
            10,
        );
        let verdict = attribute("alpha", ReviewMode::Human, &group);
        assert_eq!(verdict.judgment.verdict, Verdict::PendingReview);
    }

    #[test]
    fn test_batch_build_failure_marks_group_uncertain() {
        let output = "error[E0425]: cannot find value `x` in this scope\nerror: could not compile `probe` (lib test) due to 1 previous error\n";
        let group = group_outcome(101, output, 2, 100);
        for filter in ["alpha", "beta"] {
            let verdict = attribute(filter, ReviewMode::Auto, &group);
            assert_eq!(verdict.judgment.verdict, Verdict::Uncertain);
            assert_eq!(verdict.judgment.failure_class, Some("compile-error"));
        }
        assert!(!group.needs_fallback());
    }

    #[test]
    fn test_batch_duration_sums_reported_times_or_estimates() {
        let timed = group_outcome(
            0,
            "test tests::alpha_one ... ok <1.500s>\ntest tests::alpha_two ... ok <0.500s>\ntest result: ok. 2 passed; 0 failed; 0 ignored\n",
            1,
            9000,
        );
        let verdict = attribute("alpha", ReviewMode::Auto, &timed);
        assert_eq!(verdict.duration_ms, 2000);
        assert!(!verdict.duration_estimated);

        let untimed = group_outcome(
            0,
            "test tests::a1 ... ok\ntest tests::b1 ... ok\ntest tests::c1 ... ok\ntest tests::d1 ... ok\ntest result: ok. 4 passed; 0 failed; 0 ignored\n",
            4,
            8000,
        );
        for filter in ["a1", "b1", "c1", "d1"] {
            let verdict = attribute(filter, ReviewMode::Auto, &untimed);
            assert_eq!(verdict.duration_ms, 2000);
            assert!(verdict.duration_estimated);
        }
    }
}
