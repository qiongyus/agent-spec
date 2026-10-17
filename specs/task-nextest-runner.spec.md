spec: task
name: "Nextest Runner"
tags: [verify, guard, lifecycle, performance, cargo]
satisfies: [REQ-NEXTEST-RUNNER]
risk: B
---

## Intent

让测试验证器可以用 cargo-nextest 执行每个批次组：沿用 (package, features, soak)
分组，每组一次 `cargo nextest run`，结果从 nextest 写出的 JUnit 报告读取并交给
同一个判定函数，测试二进制之间因此并行（LEP-008 Gap 1，ADR-007 D1）。

## Decisions

- `TestRunOptions` 增加 `runner: TestRunner`（`Cargo` 默认 / `Nextest`）与 `recorded`，不再是 `Copy`；`TestRunner::parse` 只认 `cargo`、`nextest`。
- `nextest_group_args`：`nextest run`，可选 `-p`、`--features`，`--no-fail-fast`，`--no-tests=pass`（无匹配时成功并写出空报告，场景判 zero-matched），`--test-threads <N>`（默认 1），soak 组加 `--run-ignored all`，`--tool-config-file agent-spec:<文件>`，`--profile agent-spec`，`-E` 为各 filter 的 `test(~<filter>)` 以 ` | ` 连接。
- `NextestScratch` 在系统临时目录建本次运行专用目录，内含工具配置文件与 JUnit 路径，drop 时删除；`nextest_tool_config` 只定义 `[profile.agent-spec.junit] path`，自定义 profile 继承项目 `default` profile。
- `execute_plan` 按 runner 分派；nextest 时每组先删旧报告再运行，读到报告即以 `junit::parse_junit` 得到逐测试记录；输出含 ``no such command: `nextest` `` 时 `not_run = runner-unavailable`；没有可用报告时 `not_run` 取输出归类（compile-error / runner-exited）。`GroupOutcome.not_run` 有值时组内场景全部 `uncertain`。
- 只有 cargo 结果在无逐测试行时回退为逐场景调用；nextest 与报告结果不回退。
- CLI：`--runner` 由 `TestSourceArgs` 提供；与 `--no-batch` 同时出现时报错。场景证据 `source` 记 `nextest` 或 `cargo`。

## Boundaries

### Allowed Changes
- src/spec_verify/**
- src/spec_core/verify.rs
- src/main.rs
- CHANGELOG.md
- specs/task-nextest-runner.spec.md

### Forbidden
- 不改变默认（cargo）runner 的调用参数
- 不解析 nextest 的人读输出来判定
- 不并行执行多个批次组

## Out of Scope

- 文档测试（nextest 不运行）
- 超时、重试与 flaky 策略

## Completion Criteria

场景: runner 取值
  测试: test_runner_flag_parses_cargo_and_nextest
  假设 取值 `cargo`、`nextest` 与 `bazel`
  当 解析
  那么 前两者分别得到 Cargo 与 Nextest，`bazel` 报错，默认为 Cargo

场景: 默认 runner 的参数不变
  测试: test_default_runner_args_do_not_mention_nextest
  假设 一个 `Package: alpha` 的批次组
  当 以默认选项生成调用参数
  那么 参数以 `test`、`-p`、`alpha` 开头且不含 `nextest`

场景: 默认 runner 的参数形状保持原样
  测试: test_batch_group_args_shape
  假设 一个含 package `probe`、filter `alpha` 与 `beta` 的组
  当 生成组调用参数
  那么 参数依次为 `test`、`-p`、`probe`、`--no-fail-fast`、`--`、`alpha`、`beta`、`--test-threads=1`

场景: nextest 组调用的参数与过滤表达式
  测试: test_nextest_group_args_shape
  假设 一个 `Package: alpha`、`Features: fi`、filter 为 `case_a` 与 `case_b` 的组
  当 以 nextest 生成调用参数
  那么 参数以 `nextest`、`run` 开头，含相邻的 `-p alpha`、`--features fi`，含 `--no-fail-fast`，`-E` 的值为 `test(~case_a) | test(~case_b)`

场景: 线程数
  测试: test_nextest_group_args_threads
  假设 一个批次组
  当 不给与给 `--test-threads 8` 分别生成调用参数
  那么 分别含相邻的 `--test-threads 1` 与 `--test-threads 8`

场景: 只有 soak 组运行被忽略的测试
  测试: test_nextest_group_args_run_ignored_only_for_soak
  假设 一个 soak 组与一个非 soak 组
  当 以 nextest 生成调用参数
  那么 只有 soak 组含相邻的 `--run-ignored all`

场景: 工具配置与 JUnit 路径
  测试: test_nextest_tool_config_points_junit_into_scratch
  假设 一个批次组与一个临时目录
  当 生成调用参数与工具配置
  那么 参数含 `--tool-config-file agent-spec:<文件>` 与 `--profile agent-spec`，配置定义 `[profile.agent-spec.junit]` 且路径在临时目录内，目录随后被删除

场景: 从报告判定通过、失败与被忽略
  测试: test_nextest_outcomes_are_read_from_the_junit_report
  假设 nextest 替身写出含一个通过、一个失败用例的报告，第三个场景的测试被忽略因而不在报告里
  当 以 nextest 验证三个场景
  那么 只调用一次 `nextest run`，verdict 依次为 `pass`、`fail`、`skip`

场景: nextest 组不回退为逐场景调用
  测试: test_nextest_groups_never_fall_back_to_per_scenario_runs
  假设 nextest 替身成功退出并写出不含用例的报告，组内有两个场景
  当 以 nextest 验证
  那么 只调用一次，两个场景都判 `skip`

场景: 没有报告的组判 uncertain
  测试: test_nextest_group_without_report_is_uncertain
  假设 nextest 替身以编译错误退出且不写报告
  当 以 nextest 验证
  那么 verdict 等于 `uncertain`，failure_class 等于 `compile-error`

场景: 未安装 nextest
  测试: test_missing_nextest_marks_scenarios_runner_unavailable
  假设 替身输出 ``error: no such command: `nextest` ``
  当 以 nextest 验证两个不同组的场景
  那么 两个场景都判 `uncertain`，failure_class 与原因都含 `runner-unavailable`

场景: nextest 与 no-batch 冲突
  测试: test_runner_nextest_conflicts_with_no_batch
  假设 `--runner nextest` 与 `--no-batch` 同时给出
  当 生成测试选项
  那么 返回提到 `--no-batch` 的错误，不带 `--no-batch` 时得到 Nextest 选项

场景: 证据记录 runner
  测试: test_evidence_source_names_the_runner
  假设 一个通过的场景
  当 分别以 nextest 替身与默认 cargo 替身验证
  那么 证据 source 分别等于 `nextest` 与 `cargo`
