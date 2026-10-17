---
kind: requirement
id: REQ-NEXTEST-RUNNER
title: "Nextest Runner"
status: accepted
liveness: auto
tags: [verify, guard, lifecycle, performance, cargo]
---

# Nextest Runner

## Problem

测试验证器只能用 `cargo test` 执行绑定测试：各组依次执行，组内 cargo 逐个运行
测试二进制，`--test-threads` 只决定一个二进制内的并行度。测试二进制多而慢的
工作区（amberlog 的 `amberlog-server` 有几十个系统测试二进制）无法用能跨二进制
并行的 cargo-nextest，尽管 nextest 的过滤表达式 `test(~<filter>)` 正是 libtest
的子串匹配，而且 nextest 能写出机器可读的 JUnit 报告。

## Requirements

[REQ-NEXTEST-RUNNER-FLAG] guard、lifecycle 与 verify MUST 接受参数 `--runner <cargo|nextest>`。

[REQ-NEXTEST-RUNNER-DEFAULT] 未给出 `--runner` 或取 `cargo` 时，测试验证器的 cargo 调用 MUST 与引入本需求前完全相同。

[REQ-NEXTEST-RUNNER-COMMAND] 取 `nextest` 时，每组 MUST 调用一次 `cargo nextest run`，带该组的 `-p <package>`（有 package 时）、`--features <F>`（有 features 时）与 `--no-fail-fast`。

[REQ-NEXTEST-RUNNER-FILTER] 取 `nextest` 时，每组调用 MUST 带过滤表达式 `-E`，其值为组内每个 filter 的 `test(~<filter>)` 以 ` | ` 连接。

[REQ-NEXTEST-RUNNER-THREADS] 取 `nextest` 且指定 `--test-threads <N>` 时，每组调用 MUST 带 `--test-threads <N>`。

[REQ-NEXTEST-RUNNER-THREADS-DEFAULT] 取 `nextest` 且未指定 `--test-threads` 时，每组调用 MUST 带 `--test-threads 1`。

[REQ-NEXTEST-RUNNER-SOAK] 取 `nextest` 时，soak 组的调用 MUST 带 `--run-ignored all`。

[REQ-NEXTEST-RUNNER-NON-SOAK] 取 `nextest` 时，非 soak 组的调用 MUST NOT 带 `--run-ignored`。

[REQ-NEXTEST-RUNNER-PROFILE] 取 `nextest` 时，每组调用 MUST 带 `--tool-config-file agent-spec:<文件>` 与 `--profile agent-spec`。

[REQ-NEXTEST-RUNNER-JUNIT-PATH] 取 `nextest` 时，工具配置文件 MUST 把 profile `agent-spec` 的 JUnit 报告路径设在本次运行的临时目录中。

[REQ-NEXTEST-RUNNER-RESULTS] 取 `nextest` 时，各场景的判定 MUST 从该组写出的 JUnit 报告读取，按 REQ-RECORDED-TEST-RESULTS 的匹配与判定规则得出。

[REQ-NEXTEST-RUNNER-NO-REPORT] 取 `nextest` 时，某组调用结束后没有写出 JUnit 报告，该组全部场景 MUST 判 `uncertain`，原因按调用输出归类（编译错误或 runner 退出）。

[REQ-NEXTEST-RUNNER-UNAVAILABLE] 取 `nextest` 而 `cargo nextest` 无法启动时，本次运行的全部场景 MUST 判 `uncertain`，原因为 runner-unavailable。

[REQ-NEXTEST-RUNNER-NO-BATCH] `--runner nextest` 与 `--no-batch` 同时出现时，命令 MUST 以参数错误退出，且不产生任何场景判定。

[REQ-NEXTEST-RUNNER-EVIDENCE] 取 `nextest` 时，场景测试证据的 `source` 字段 MUST 等于 `nextest`。

[REQ-NEXTEST-RUNNER-EVIDENCE-CARGO] 取 `cargo` 时，场景测试证据的 `source` 字段 MUST 等于 `cargo`。

## Scenarios

Rule: REQ-NEXTEST-RUNNER-FLAG

Scenario: 接受 nextest 取值
  Given 一个场景绑定一个通过的测试，环境中已安装 cargo-nextest
  When 运行 `agent-spec verify <spec> --code . --runner nextest`
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-NEXTEST-RUNNER-DEFAULT

Scenario: 默认 runner 的参数不变
  Given 一个 selector 为 `Package: alpha`、`Filter: case_a` 的批次组
  When 不给 `--runner` 生成该组的调用参数
  Then 返回的参数列表以 `test`、`-p`、`alpha` 开头且不包含 `nextest`

Rule: REQ-NEXTEST-RUNNER-COMMAND

Scenario: nextest 组调用带 package、features 与 no-fail-fast
  Given 一个 selector 为 `Package: alpha`、`Filter: case_a`、`Features: fi` 的批次组
  When 以 `--runner nextest` 生成该组的调用参数
  Then 返回的参数列表以 `nextest`、`run` 开头，并包含相邻的 `-p`、`alpha`，相邻的 `--features`、`fi`，以及 `--no-fail-fast`

Rule: REQ-NEXTEST-RUNNER-FILTER

Scenario: 多个 filter 合成一个表达式
  Given 一个含 filter `case_a` 与 `case_b` 的批次组
  When 以 `--runner nextest` 生成该组的调用参数
  Then 返回的参数列表包含相邻的 `-E` 与 `test(~case_a) | test(~case_b)`

Rule: REQ-NEXTEST-RUNNER-THREADS-DEFAULT

Scenario: 默认单线程
  Given 一个批次组
  When 以 `--runner nextest` 且不给 `--test-threads` 生成调用参数
  Then 返回的参数列表包含相邻的 `--test-threads` 与 `1`

Rule: REQ-NEXTEST-RUNNER-THREADS

Scenario: 指定线程数
  Given 一个批次组
  When 以 `--runner nextest --test-threads 8` 生成调用参数
  Then 返回的参数列表包含相邻的 `--test-threads` 与 `8`

Rule: REQ-NEXTEST-RUNNER-SOAK

Scenario: soak 组运行被忽略的测试
  Given 一个 soak 批次组
  When 以 `--runner nextest` 生成调用参数
  Then 返回的参数列表包含相邻的 `--run-ignored` 与 `all`

Rule: REQ-NEXTEST-RUNNER-NON-SOAK

Scenario: 非 soak 组不运行被忽略的测试
  Given 一个非 soak 批次组
  When 以 `--runner nextest` 生成调用参数
  Then 返回的参数列表不包含 `--run-ignored`

Rule: REQ-NEXTEST-RUNNER-PROFILE

Scenario: 调用带工具配置与 profile
  Given 一个批次组
  When 以 `--runner nextest` 生成调用参数
  Then 返回的参数列表包含 `--tool-config-file`，其值以 `agent-spec:` 开头，并包含相邻的 `--profile` 与 `agent-spec`

Rule: REQ-NEXTEST-RUNNER-JUNIT-PATH

Scenario: 工具配置把 JUnit 写到临时目录
  Given 一次以 `--runner nextest` 生成的工具配置文件
  When 读取该文件
  Then 文件定义 `[profile.agent-spec.junit]` 且其 `path` 位于本次运行的临时目录内

Rule: REQ-NEXTEST-RUNNER-RESULTS

Scenario: 从报告判定失败
  Given 一个场景绑定一个会失败的测试，环境中已安装 cargo-nextest
  When 运行 `agent-spec verify <spec> --code . --runner nextest`
  Then 输出的该场景 verdict 等于 `fail`

Scenario: 被忽略的测试不算通过
  Given 一个非 soak 场景只绑定一个 `#[ignore]` 的测试，环境中已安装 cargo-nextest
  When 运行 `agent-spec verify <spec> --code . --runner nextest`
  Then 输出的该场景 verdict 等于 `skip`

Rule: REQ-NEXTEST-RUNNER-NO-REPORT

Scenario: 编译失败的组
  Given 一个场景所在 package 有编译错误，环境中已安装 cargo-nextest
  When 运行 `agent-spec verify <spec> --code . --runner nextest`
  Then 输出的该场景 verdict 等于 `uncertain`

Rule: REQ-NEXTEST-RUNNER-UNAVAILABLE

Scenario: 未安装 nextest
  Given 一个测试命令替身，对 `nextest` 子命令返回「no such command」
  When 以 `--runner nextest` 运行 verify
  Then 输出的每个绑定场景 verdict 等于 `uncertain` 且原因为 runner-unavailable

Rule: REQ-NEXTEST-RUNNER-NO-BATCH

Scenario: nextest 与 no-batch 冲突
  Given 一个含绑定场景的 spec
  When 运行 `agent-spec verify <spec> --code . --runner nextest --no-batch`
  Then 退出码非 0 且输出不包含任何场景 verdict

Rule: REQ-NEXTEST-RUNNER-EVIDENCE

Scenario: 证据记录 runner
  Given 一个场景绑定一个通过的测试，环境中已安装 cargo-nextest
  When 运行 `agent-spec verify <spec> --code . --runner nextest --format json`
  Then 输出的该场景测试证据的 source 字段等于 `nextest`

Rule: REQ-NEXTEST-RUNNER-EVIDENCE-CARGO

Scenario: 默认 runner 的证据记录 cargo
  Given 一个场景绑定一个通过的测试
  When 运行 `agent-spec verify <spec> --code . --format json`
  Then 输出的该场景测试证据的 source 字段等于 `cargo`

## Dependencies

- REQ-BATCHED-TEST-EXECUTION（沿用 (package, features, soak) 分组）
- REQ-RECORDED-TEST-RESULTS（组结果按报告的匹配与判定规则得出）
- REQ-SOAK-SCENARIOS（soak 组运行被忽略的测试）

## Source Trace

- proposal: LEP-008 Gap 1 (knowledge/proposals/2026-10-17-test-runners-and-recorded-results.md)
- decision: ADR-007 D1（`--runner cargo|nextest`，默认 cargo；每组一次 `cargo nextest run`，结果从工具配置指定的临时 JUnit 报告读取；未安装判 runner-unavailable；与 `--no-batch` 冲突，2026-10-17）
- 实测证据: amberlog `test` 作业改用 nextest 后由 60～89 分钟降到约 33～47 分钟（流水线 #456、#462）；nextest 0.9.146 JUnit 报告结构，2026-10-17 探针 crate 实测
- 代码位置: `src/spec_verify/test_batch.rs` 的 `group_args` 与 `execute_plan`；`src/spec_verify/test_verifier.rs` 的 `CargoRunner`
- 补充边界: 没有写出报告的组判 uncertain、默认线程数 1 与 D7 一致，ADR-007 未逐条写出，2026-10-17 拆分需求时补充

## Open Questions

None.
