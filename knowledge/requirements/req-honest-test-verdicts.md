---
kind: requirement
id: REQ-HONEST-TEST-VERDICTS
title: "Honest Test Verdicts"
status: accepted
liveness: auto
tags: [verify, guard, lifecycle]
---

# Honest Test Verdicts

## Problem

测试验证器按 `running N tests` 行累计「已执行」数。libtest 对匹配到
`#[ignore]` 测试的 filter 同样打印 `running 1 test`，却并不运行它，退出码为
0，于是场景被判 `pass`：给任何合约测试加上 `#[ignore]` 就能让它变绿。这违反
了 REQ-REQUIREMENTS-COMPILER-PLAN-DAG-ZERO-TEST「执行零个匹配测试即
`skip`」的既有承诺。

另一方面，cargo 在打印任何 `running` 行之前以非零退出时，判定一律为
`uncertain`，编译错误、工具链故障与 runner 未启动混在一起；guard 只打印每个
spec 的汇总计数，证据里也没有退出状态，区分「值得重试一次的环境问题」与
「永远不该重试的编译错误」只能手工重跑。

## Requirements

### 执行计数

[REQ-HONEST-TEST-VERDICTS-SUMMARY-COUNT] 测试验证器 MUST 从 libtest 的 `test result:` 汇总行计算已执行数，已执行数等于各测试二进制的 passed 与 failed 之和。

[REQ-HONEST-TEST-VERDICTS-IGNORED-SEPARATE] 测试验证器 MUST 把 ignored 数与已执行数分开统计。

### 判定

[REQ-HONEST-TEST-VERDICTS-ALL-IGNORED] cargo 成功退出、已执行数为 0 且 ignored 数大于 0 时，场景判定 MUST 为 `skip`。

[REQ-HONEST-TEST-VERDICTS-IGNORED-NOTE] 因全部匹配测试被忽略而判 `skip` 的场景，其原因 MUST 包含文本 `bound test is #[ignore]d`。

[REQ-HONEST-TEST-VERDICTS-MIXED] 匹配测试中部分被执行、部分被忽略时，场景判定 MUST 仅由已执行的测试决定。

[REQ-HONEST-TEST-VERDICTS-MIXED-EVIDENCE] 匹配测试中有被忽略的测试时，场景证据 MUST 记录 ignored 数。

[REQ-HONEST-TEST-VERDICTS-NO-SUMMARY] cargo 成功退出但输出中没有可解析的 `test result:` 汇总行时，场景判定 MUST 为 `uncertain`。

[REQ-HONEST-TEST-VERDICTS-NEVER-PASS] 测试验证器 MUST NOT 在已执行数为 0 时给出 `pass` 或 `pending_review`。

### 非通过原因

[REQ-HONEST-TEST-VERDICTS-EXIT-STATUS] 测试证据 MUST 记录 cargo 的退出状态：正常退出时记录退出码，被信号终止时记录信号编号。

[REQ-HONEST-TEST-VERDICTS-UNCERTAIN-CLASS] 在任何测试执行之前以非零状态退出的判定 MUST 归为两类之一：输出含 `error[E` 或 `could not compile` 行时为 `compile-error`，否则为 `runner-exited`。

[REQ-HONEST-TEST-VERDICTS-GUARD-DETAIL] guard 对每个判定为 `fail` 或 `uncertain` 的场景 MUST 在该 spec 的汇总行之下打印场景名、selector、退出状态与合并输出的最后至多 40 行。

[REQ-HONEST-TEST-VERDICTS-GUARD-CLASS] guard 对判定为 `uncertain` 的场景 MUST 同时打印其类别 `compile-error` 或 `runner-exited`。

[REQ-HONEST-TEST-VERDICTS-NO-RETRY] agent-spec MUST NOT 自动重试任何测试调用。

[REQ-HONEST-TEST-VERDICTS-PASS-QUIET] guard 对判定为 `pass` 的场景 MUST NOT 打印其测试输出。

## Scenarios

### 执行计数

Rule: REQ-HONEST-TEST-VERDICTS-SUMMARY-COUNT

Scenario: 已执行数取自汇总行
  Given 一段 cargo 输出含两个二进制的汇总行 `test result: ok. 2 passed; 0 failed; 1 ignored` 与 `test result: ok. 1 passed; 0 failed; 0 ignored`
  When 计算已执行数
  Then 返回的已执行数等于 3

Rule: REQ-HONEST-TEST-VERDICTS-IGNORED-SEPARATE

Scenario: ignored 数单独统计
  Given 同上 cargo 输出
  When 计算 ignored 数
  Then 返回的 ignored 数等于 1

### 判定

Rule: REQ-HONEST-TEST-VERDICTS-ALL-IGNORED

Scenario: 唯一匹配的测试被忽略时判 skip
  Given 一个场景绑定的 filter 只匹配一个标注 `#[ignore]` 的测试
  When 运行 verify
  Then 输出的该场景 verdict 等于 `skip`

Rule: REQ-HONEST-TEST-VERDICTS-IGNORED-NOTE

Scenario: skip 原因说明测试被忽略
  Given 同上场景
  When 运行 verify --format json
  Then 输出的该场景原因包含 `bound test is #[ignore]d`

Rule: REQ-HONEST-TEST-VERDICTS-MIXED

Scenario: 部分忽略时按已执行的判定
  Given 一个场景绑定的 filter 匹配两个测试，一个通过、一个标注 `#[ignore]`
  When 运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Scenario: 部分忽略且已执行的失败时判 fail
  Given 一个场景绑定的 filter 匹配两个测试，一个失败、一个标注 `#[ignore]`
  When 运行 verify
  Then 输出的该场景 verdict 等于 `fail`

Rule: REQ-HONEST-TEST-VERDICTS-MIXED-EVIDENCE

Scenario: 证据记录 ignored 数
  Given 一个场景绑定的 filter 匹配两个测试，一个通过、一个标注 `#[ignore]`
  When 运行 verify --format json
  Then 输出的该场景测试证据的 ignored 字段等于 1

Rule: REQ-HONEST-TEST-VERDICTS-NO-SUMMARY

Scenario: 缺少汇总行时判 uncertain
  Given 一段退出码为 0、含 `running 1 test` 但不含 `test result:` 行的 cargo 输出
  When 计算场景判定
  Then 返回的 verdict 等于 `uncertain`

Rule: REQ-HONEST-TEST-VERDICTS-NEVER-PASS

Scenario: 零执行的 human review 场景不进入 pending_review
  Given 一个声明 `Review: human` 的场景，其 filter 只匹配一个标注 `#[ignore]` 的测试
  When 运行 verify
  Then 输出的该场景 verdict 等于 `skip`

### 非通过原因

Rule: REQ-HONEST-TEST-VERDICTS-EXIT-STATUS

Scenario: 证据记录退出码
  Given 一个场景绑定的测试失败，cargo 以退出码 101 退出
  When 运行 verify --format json
  Then 输出的该场景测试证据的 exit_code 字段等于 101

Rule: REQ-HONEST-TEST-VERDICTS-UNCERTAIN-CLASS

Scenario: 编译错误归为 compile-error
  Given 一段退出码非零、不含 `test result:` 行且含 `error[E0425]` 的 cargo 输出
  When 计算场景判定
  Then 返回的 verdict 等于 `uncertain`，类别等于 `compile-error`

Scenario: 无 cargo 输出的退出归为 runner-exited
  Given 一次以退出码 255 结束、stdout 与 stderr 都为空的测试调用
  When 计算场景判定
  Then 返回的 verdict 等于 `uncertain`，类别等于 `runner-exited`

Rule: REQ-HONEST-TEST-VERDICTS-GUARD-DETAIL

Scenario: guard 打印失败场景的细节
  Given 一个 spec 含一个绑定失败测试的场景
  When 运行 guard
  Then stderr 在该 spec 汇总行之后包含该场景名、selector、`exit code 101` 与测试输出中的 panic 消息

Rule: REQ-HONEST-TEST-VERDICTS-GUARD-CLASS

Scenario: guard 打印 uncertain 类别
  Given 一个 spec 含一个绑定测试所在 crate 无法编译的场景
  When 运行 guard
  Then stderr 包含该场景名与 `compile-error`

Rule: REQ-HONEST-TEST-VERDICTS-NO-RETRY

Scenario: runner 退出不触发重试
  Given 一个测试调用以退出码 255 结束且没有 cargo 输出
  When 运行 verify
  Then 该场景对应的测试命令只被调用一次，输出的 verdict 等于 `uncertain`

Rule: REQ-HONEST-TEST-VERDICTS-PASS-QUIET

Scenario: 通过的场景不打印测试输出
  Given 一个 spec 的全部场景都通过
  When 运行 guard
  Then stderr 不包含任何测试输出行

## Dependencies

- REQ-REQUIREMENTS-COMPILER-PLAN-DAG（零执行判 skip 的既有条款由它定义，本需求补上 ignored 的情形）

## Source Trace

- proposal: LEP-007 Gap 2、Gap 8 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D2（ignored 不算执行；部分忽略按已执行判定；缺汇总行退回 uncertain）与 D8（打印非通过原因、区分两类 uncertain、不重试，2026-09-30）
- 实测证据: 双场景探针 spec 中绑定 `#[ignore]` 测试的场景被判 `pass`；amberlog 2026-09-30 远程构建包装器以退出码 255 无输出退出，单个场景 `uncertain` 拦下提交，单独重跑通过
- 代码位置: `src/spec_verify/test_verifier.rs` 的 `cargo_test_executed_count` 与 `test_run_verdict`；`Evidence::TestOutput` 目前只保存合并输出与 `passed`，不含退出状态
- 参数取值: 输出尾部 40 行，ADR-006 只规定「最后若干行」，2026-09-30 拆分需求时经人工确认

## Open Questions

None.
