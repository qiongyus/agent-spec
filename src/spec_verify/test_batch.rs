//! Batched test execution (LEP-007 D7): one `cargo test` per
//! (package, features, soak) group instead of one per scenario, with each
//! scenario's verdict read back from libtest's per-test result lines.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use crate::spec_core::{ReviewMode, SpecResult, TestSelector, Verdict};

use super::test_verifier::{
    JudgmentKind, LibtestSummary, TestCommandRunner, TestJudgment, TestRun, TestRunOptions,
    judge_test_run, parse_libtest_summary, selector_features,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOutcome {
    Ok,
    Failed,
    Ignored,
}

/// One libtest `test <path> ... <result>` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestLine {
    pub path: String,
    pub outcome: LineOutcome,
    /// `<1.500s>` suffix, when libtest reports per-test time.
    pub duration_ms: Option<u64>,
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
            })
        })
        .collect()
}

/// What one group's cargo run produced.
#[derive(Debug, Clone)]
pub struct GroupOutcome {
    pub run: TestRun,
    pub output: String,
    pub lines: Vec<TestLine>,
    pub wall_ms: u64,
    pub scenario_count: usize,
}

impl GroupOutcome {
    /// Succeeded and reported tests, but printed no per-test line this parser
    /// recognizes (for example a `harness = false` target): read nothing
    /// from it and run the group's scenarios one by one instead. A summary
    /// that reports zero tests is an ordinary "matched nothing".
    pub fn needs_fallback(&self) -> bool {
        if !self.run.exit.success || !self.lines.is_empty() {
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
            },
        );
    }
    Ok(outcomes)
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
}

/// Judge one scenario from its group's run, with the same rules as a
/// per-scenario run: the matched tests are those whose path contains the
/// filter (libtest's substring semantics).
pub fn attribute(filter: &str, review: ReviewMode, group: &GroupOutcome) -> BatchedVerdict {
    let estimated = || group.wall_ms / group.scenario_count.max(1) as u64;
    if group.lines.is_empty() && !group.run.exit.success {
        // Nothing ran (build, toolchain or runner failure): every scenario
        // in the group is uncertain, classed from the group output.
        return BatchedVerdict {
            judgment: judge_test_run(&group.run.exit, &group.output, review),
            output: group.output.clone(),
            duration_ms: estimated(),
            duration_estimated: true,
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
