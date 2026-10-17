spec: task
name: "Recorded Test Results"
tags: [verify, guard, lifecycle, ci]
satisfies: [REQ-RECORDED-TEST-RESULTS]
risk: B
---

## Intent

让 guard、lifecycle 与 verify 能用已有的 JUnit 报告判定场景，不编译、不运行任何
测试：CI 里前一个作业已经跑完全部测试并写出报告时，contract 门禁不必再跑第二遍
（LEP-008 Gap 2，ADR-007 D2、D4）。判定规则与现场运行完全相同。

## Decisions

- 新模块 `src/spec_verify/junit.rs`：`parse_junit` 用 quick-xml 把每个 `<testcase>` 读成一条 `TestLine`——二进制 id 取 `classname`，缺省时取所在 `<testsuite name>`；路径取 `name`；`<failure>` / `<error>` 为失败（其 message 与文本作为失败输出），`<skipped>` 为未执行，其余为通过。`load_junit` 读不到或解析失败时返回指出文件路径的错误。
- `TestLine` 增加 `binary_id`、`detail`（失败输出）与 `report`（报告序号）；libtest 结果行三者为空。
- `RecordedResults::load(paths, features)` 依次读取全部报告，同一 (二进制 id, 用例名) 只留一条，失败 > 通过 > 未执行。`TestRunOptions.recorded` 持有它；给出时 `TestVerifier::prefetch` 返回 `None`，`verify` 逐场景判定、不调用 runner。
- 按 package 取用例：`Package: P` 取二进制 id 等于 `P` 或以 `P::` 开头的用例，未声明 package 取全部；得到的 `GroupOutcome` 交给现有 `attribute` 判定（filter 子串匹配与原有判定规则不变）。
- `GroupOutcome` 增加 `source: OutcomeSource`（`Cargo` / `Nextest` / `Recorded(paths)`）；场景证据 `TestOutputDetail.source` 记 `cargo`、`nextest` 或匹配用例所在的报告路径（无匹配时为全部报告路径），以 `, ` 连接。
- CLI：verify、lifecycle、guard 以 `#[command(flatten)] TestSourceArgs` 接受可重复的 `--test-results <path>`；`TestSourceArgs::options` 在验证开始前检查组合（与 `--runner`、`--no-batch` 冲突即报错）并读取报告，出错时命令以非 0 退出、不产生判定。
- guard 汇总行在给出报告时追加 `(results from N report(s): M test case(s)…)`，成功与失败时都打印。

## Boundaries

### Allowed Changes
- src/spec_verify/**
- src/spec_core/verify.rs
- src/spec_gateway/lifecycle.rs
- src/main.rs
- Cargo.toml
- Cargo.lock
- CHANGELOG.md
- specs/task-recorded-test-results.spec.md

### Forbidden
- 不改变不给 `--test-results` 时的任何行为
- 不在判定函数之外另写一套判定规则
- 不校验报告属于哪个提交

## Out of Scope

- 报告的来源校验（提交号、流水线）
- JUnit 之外的报告格式

## Completion Criteria

场景: 读取两份报告
  测试: test_recorded_results_merge_two_reports
  假设 两份报告分别含场景 S1、S2 绑定的通过用例
  当 以两个 `--test-results` 验证
  那么 S1 与 S2 的 verdict 都等于 `pass`

场景: 给出报告时不调用测试命令
  测试: test_recorded_results_run_no_test_command
  假设 一份含场景绑定用例的报告与一个计数的测试命令替身
  当 以该报告 prefetch 并验证
  那么 prefetch 返回空，替身调用次数等于 0，场景 verdict 等于 `pass`

场景: 报告与 runner 或 no-batch 冲突
  测试: test_test_results_conflicts_with_runner_and_no_batch
  假设 `--test-results r.xml` 分别与 `--runner nextest`、`--no-batch` 同时给出
  当 生成测试选项
  那么 两次都返回错误，错误信息分别提到 `--runner` 与 `--no-batch`

场景: 报告不存在或无法解析
  测试: test_test_results_unreadable_report_is_an_error
  假设 路径 missing.xml 不存在，bad.xml 的内容是 `not xml`
  当 分别读取
  那么 两次都返回错误，错误信息分别包含 `missing.xml` 与 `bad.xml`

场景: 不是 JUnit 的输入被拒绝
  测试: test_parse_junit_rejects_non_junit_input
  假设 输入为 `not xml`、`<html></html>` 与标签不闭合的 testsuite
  当 解析
  那么 三次都返回错误

场景: 只匹配本 package 的二进制
  测试: test_recorded_results_match_binaries_of_the_package
  假设 报告含 `alpha::sys` 的通过用例、`beta::sys` 的同名失败用例、`alpha` 的单元测试用例与 `alpha-extra::sys` 的用例
  当 以 `Package: alpha` 的三个场景验证
  那么 verdict 依次为 `pass`、`pass`、`skip`

场景: 未声明 package 时匹配任意二进制
  测试: test_recorded_results_without_package_match_any_binary
  假设 报告只含 `beta::sys` 的通过用例 `case_w`，场景 selector 只有 `Filter: case_w`
  当 验证
  那么 verdict 等于 `pass`

场景: filter 按子串匹配
  测试: test_recorded_results_match_filter_by_substring
  假设 报告含用例 `recovery::after_crash`，场景 filter 为 `after_crash`
  当 验证
  那么 verdict 等于 `pass`

场景: 读取 nextest 报告的用例
  测试: test_parse_junit_reads_nextest_report
  假设 一份 nextest 0.9.146 形状的报告
  当 解析
  那么 得到三条用例，二进制 id、路径、用时与失败输出与报告一致

场景: failure、error、skipped 与无子元素的用例
  测试: test_parse_junit_error_and_skipped_children
  假设 一个 testsuite 含带 `<error>`、带 `<skipped/>` 与无子元素的三个用例，均无 classname
  当 解析
  那么 结果依次为失败、未执行、通过，二进制 id 都等于 testsuite 名

场景: 判定沿用无匹配、无执行、失败、通过与人工评审规则
  测试: test_recorded_results_verdicts_follow_attribution_rules
  假设 六个场景分别对应无匹配、全部 skipped、一过一败、error、全部通过与 `Review: human` 全部通过
  当 验证
  那么 verdict 依次为 `skip`、`skip`、`fail`、`fail`、`pass`、`pending_review`，前两者原因分别为 zero-matched 与 ignored

场景: 任一报告失败即失败
  测试: test_recorded_results_failure_in_any_report_wins
  假设 同一用例在 a.xml 中通过、在 b.xml 中失败
  当 以两种顺序给出两份报告验证
  那么 verdict 都等于 `fail`，证据 source 都等于 b.xml 的路径

场景: soak 场景仍暂缓
  测试: test_recorded_results_keep_soak_scenarios_deferred
  假设 一个 `Mode: soak` 场景，报告中其用例通过
  当 不带 include_soak 验证
  那么 verdict 等于 `deferred`

场景: 两种来源得到同一判定
  测试: test_recorded_and_libtest_outcomes_judge_alike
  假设 同一组结果分别以 libtest 输出与 JUnit 报告表示
  当 对四个 filter 分别判定
  那么 两种来源的 verdict 与原因类别相同

场景: 汇总写明报告份数与用例数
  测试: test_recorded_results_summary_names_reports_and_cases
  假设 一份含 3 个用例的报告
  当 生成汇总
  那么 汇总包含 `1 report` 与 `3 test cases`

场景: 证据记录报告路径
  测试: test_recorded_results_evidence_source_is_report_path
  假设 路径以 reports/junit.xml 结尾的报告，场景用例通过
  当 验证
  那么 证据的 source 等于该报告路径
