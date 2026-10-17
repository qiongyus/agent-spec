---
kind: requirement
id: REQ-RECORDED-TEST-RESULTS
title: "Recorded Test Results"
status: proposed
liveness: auto
tags: [verify, guard, lifecycle, ci]
---

# Recorded Test Results

## Problem

guard、lifecycle 与 verify 总是自己编译并运行绑定测试。CI 流水线常在前一个
作业里跑完全部测试并留下 JUnit 报告，guard 却无法采用：它要的测试只是那次运行
的子集，仍要重新编译、重新运行。amberlog（2026-10-16，43 个合约、931 个
selector）的 `test` 作业用 nextest 47 分钟跑完 1337 个测试，`knowledge` 作业的
guard 再重跑绑定测试用了 114 分钟（流水线 #462），并随合约增加继续增长。

## Requirements

### 参数

[REQ-RECORDED-TEST-RESULTS-FLAG] guard、lifecycle 与 verify MUST 接受可重复的参数 `--test-results <path>`，每个值是一份 JUnit XML 报告。

[REQ-RECORDED-TEST-RESULTS-NO-EXECUTION] 给出 `--test-results` 时，测试验证器 MUST NOT 调用 cargo 或任何其他测试命令。

[REQ-RECORDED-TEST-RESULTS-CONFLICT] `--test-results` 与 `--runner` 或 `--no-batch` 同时出现时，命令 MUST 以参数错误退出，且不产生任何场景判定。

[REQ-RECORDED-TEST-RESULTS-UNREADABLE] 任一报告不存在或无法解析为 JUnit XML 时，命令 MUST 以非 0 退出码退出并在输出中指出该文件路径，且不产生任何场景判定。

### 匹配

[REQ-RECORDED-TEST-RESULTS-PACKAGE] 声明了 `Package: P` 的 selector MUST 只匹配二进制 id（测试用例的 `classname`，缺省时取所在测试套件的 `name`）等于 `P` 或以 `P::` 开头的测试用例。

[REQ-RECORDED-TEST-RESULTS-ANY-PACKAGE] 未声明 package 的 selector MUST 可匹配任意二进制 id 的测试用例。

[REQ-RECORDED-TEST-RESULTS-FILTER] 测试用例名包含 selector 的 filter 时，该用例 MUST 计为匹配。

### 判定

[REQ-RECORDED-TEST-RESULTS-OUTCOME] 含 `<failure>` 或 `<error>` 子元素的测试用例 MUST 计为失败。

[REQ-RECORDED-TEST-RESULTS-OUTCOME-SKIPPED] 含 `<skipped>` 子元素的测试用例 MUST 计为未执行。

[REQ-RECORDED-TEST-RESULTS-OUTCOME-PASSED] 不含 `<failure>`、`<error>`、`<skipped>` 子元素的测试用例 MUST 计为通过。

[REQ-RECORDED-TEST-RESULTS-NONE-MATCHED] 没有测试用例匹配的场景 MUST 判 `skip`，原因为 zero-matched。

[REQ-RECORDED-TEST-RESULTS-NONE-EXECUTED] 匹配到的测试用例全部未执行的场景 MUST 判 `skip`，原因为 all-ignored。

[REQ-RECORDED-TEST-RESULTS-ANY-FAILED] 匹配到的测试用例中有失败的场景 MUST 判 `fail`。

[REQ-RECORDED-TEST-RESULTS-ALL-PASSED] 匹配到的测试用例有执行、且全部通过的非人工评审场景 MUST 判 `pass`。

[REQ-RECORDED-TEST-RESULTS-HUMAN-REVIEW] 匹配到的测试用例有执行、且全部通过的 `Review: human` 场景 MUST 判 `pending_review`。

[REQ-RECORDED-TEST-RESULTS-DUPLICATE] 多份报告含二进制 id 与用例名都相同的测试用例时，任一份记为失败，该用例 MUST 计为失败。

[REQ-RECORDED-TEST-RESULTS-SOAK] 未指定 `--include-soak` 时，`Mode: soak` 场景 MUST 判 `deferred`，与不给 `--test-results` 时相同。

[REQ-RECORDED-TEST-RESULTS-SAME-RULES] 由报告得到的逐测试记录与由 cargo 输出得到的逐测试记录 MUST 由同一个判定函数逐场景判定。

### 输出

[REQ-RECORDED-TEST-RESULTS-SUMMARY] 给出 `--test-results` 时，guard 的汇总行 MUST 写明报告份数与读到的测试用例总数。

[REQ-RECORDED-TEST-RESULTS-EVIDENCE] 由报告判定的场景，其测试证据的 `source` 字段 MUST 等于提供该判定的报告路径。

## Scenarios

Rule: REQ-RECORDED-TEST-RESULTS-FLAG

Scenario: 读取两份报告
  Given 两份 JUnit 报告 a.xml 与 b.xml，分别含场景 S1、S2 绑定的通过用例
  When 运行 `agent-spec verify <spec> --code . --test-results a.xml --test-results b.xml`
  Then 输出的 S1 与 S2 verdict 都等于 `pass`

Rule: REQ-RECORDED-TEST-RESULTS-NO-EXECUTION

Scenario: 给出报告时不调用 cargo
  Given 一份含场景 S1 绑定用例的 JUnit 报告，以及一个记录调用次数的测试命令替身
  When 以 `--test-results` 运行 verify
  Then 返回的替身调用次数等于 0

Rule: REQ-RECORDED-TEST-RESULTS-CONFLICT

Scenario: 报告与 nextest runner 同时给出
  Given 一份 JUnit 报告
  When 运行 `agent-spec guard --spec-dir specs --code . --test-results r.xml --runner nextest`
  Then 退出码非 0 且输出不包含任何场景 verdict

Scenario: 报告与 no-batch 同时给出
  Given 一份 JUnit 报告
  When 运行 `agent-spec verify <spec> --code . --test-results r.xml --no-batch`
  Then 退出码非 0 且输出不包含任何场景 verdict

Rule: REQ-RECORDED-TEST-RESULTS-UNREADABLE

Scenario: 报告文件不存在
  Given 路径 missing.xml 不存在
  When 运行 `agent-spec guard --spec-dir specs --code . --test-results missing.xml`
  Then 退出码非 0 且输出包含 `missing.xml`

Scenario: 报告不是 JUnit XML
  Given 文件 bad.xml 的内容是 `not xml`
  When 运行 `agent-spec guard --spec-dir specs --code . --test-results bad.xml`
  Then 退出码非 0 且输出包含 `bad.xml`

Rule: REQ-RECORDED-TEST-RESULTS-PACKAGE

Scenario: 只匹配本 package 的二进制
  Given 一份报告含通过用例 `alpha::sys` / `case_x` 与失败用例 `beta::sys` / `case_x`，场景 selector 为 `Package: alpha`、`Filter: case_x`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Scenario: 库单元测试的二进制 id 等于 package 名
  Given 一份报告含通过用例 `alpha` / `tests::case_y`，场景 selector 为 `Package: alpha`、`Filter: case_y`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Scenario: 名字前缀相同的另一个 package 不匹配
  Given 一份报告只含通过用例 `alpha-extra::sys` / `case_z`，场景 selector 为 `Package: alpha`、`Filter: case_z`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `skip`

Rule: REQ-RECORDED-TEST-RESULTS-ANY-PACKAGE

Scenario: 未声明 package 时匹配任意二进制
  Given 一份报告只含通过用例 `beta::sys` / `case_w`，场景 selector 只有 `Filter: case_w`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-RECORDED-TEST-RESULTS-FILTER

Scenario: filter 按子串匹配
  Given 一份报告含通过用例 `alpha::sys` / `recovery::after_crash`，场景 selector 为 `Package: alpha`、`Filter: after_crash`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-RECORDED-TEST-RESULTS-OUTCOME

Scenario: error 子元素计为失败
  Given 一份报告中场景绑定的唯一用例带 `<error>` 子元素
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `fail`

Rule: REQ-RECORDED-TEST-RESULTS-OUTCOME-SKIPPED

Scenario: skipped 子元素计为未执行
  Given 一份报告中场景绑定的两个用例都带 `<skipped/>` 子元素
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `skip`

Rule: REQ-RECORDED-TEST-RESULTS-OUTCOME-PASSED

Scenario: 没有子元素的用例计为通过
  Given 一份报告中场景绑定的唯一用例是自闭合的 `<testcase .../>`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-RECORDED-TEST-RESULTS-NONE-MATCHED

Scenario: 报告里没有绑定的测试
  Given 一份报告不含任何名字包含 `case_absent` 的用例，场景 selector 为 `Filter: case_absent`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `skip` 且原因为 zero-matched

Scenario: 无匹配的场景使 guard 不通过
  Given 一份报告不含场景 S1 绑定的测试
  When 以该报告运行 guard
  Then 输出的 S1 verdict 等于 `skip` 且 guard 的退出码非 0

Rule: REQ-RECORDED-TEST-RESULTS-NONE-EXECUTED

Scenario: 匹配的用例全部 skipped
  Given 一份报告中场景绑定的唯一用例带 `<skipped/>` 子元素
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `skip` 且原因为 all-ignored

Rule: REQ-RECORDED-TEST-RESULTS-ANY-FAILED

Scenario: 一个通过一个失败
  Given 一份报告中场景 filter 匹配两个用例，一个通过、一个带 `<failure>`
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `fail`

Rule: REQ-RECORDED-TEST-RESULTS-ALL-PASSED

Scenario: 全部通过
  Given 一份报告中场景 filter 匹配的两个用例都通过
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-RECORDED-TEST-RESULTS-HUMAN-REVIEW

Scenario: 人工评审场景全部通过
  Given 一个 `Review: human` 场景，报告中其 filter 匹配的用例都通过
  When 以该报告运行 verify
  Then 输出的该场景 verdict 等于 `pending_review`

Rule: REQ-RECORDED-TEST-RESULTS-DUPLICATE

Scenario: 两份报告结论不同
  Given 报告 a.xml 中用例 `alpha::sys` / `case_d` 通过，报告 b.xml 中同一用例带 `<failure>`
  When 以两份报告运行 verify
  Then 输出的绑定 `case_d` 的场景 verdict 等于 `fail`

Rule: REQ-RECORDED-TEST-RESULTS-SOAK

Scenario: soak 场景仍暂缓
  Given 一个 `Mode: soak` 场景，报告中其 filter 匹配的用例通过
  When 以该报告运行 guard
  Then 输出的该场景 verdict 等于 `deferred`

Rule: REQ-RECORDED-TEST-RESULTS-SAME-RULES

Scenario: 两种来源得到同一判定
  Given 同一组逐测试结果分别以 libtest 输出与 JUnit 报告表示
  When 分别判定同一个场景
  Then 返回的两个 verdict 与原因相同

Rule: REQ-RECORDED-TEST-RESULTS-SUMMARY

Scenario: 汇总行写明报告来源
  Given 一份含 3 个用例的报告，全部场景通过
  When 以该报告运行 guard
  Then 输出的汇总行包含 `1 report` 与 `3 test cases`

Rule: REQ-RECORDED-TEST-RESULTS-EVIDENCE

Scenario: 证据记录报告路径
  Given 一份路径为 reports/junit.xml 的报告，场景 S1 绑定的用例通过
  When 以该报告运行 verify --format json
  Then 输出的 S1 测试证据的 source 字段等于 `reports/junit.xml`

## Dependencies

- REQ-BATCHED-TEST-EXECUTION（逐场景判定沿用分批执行的回填规则与非通过原因）
- REQ-HONEST-TEST-VERDICTS（未执行的测试不算通过）
- REQ-SOAK-SCENARIOS（soak 场景默认暂缓）

## Source Trace

- proposal: LEP-008 Gap 2 (knowledge/proposals/2026-10-17-test-runners-and-recorded-results.md)
- decision: ADR-007 D2、D4（读取 JUnit 报告判定、不调用 cargo；与 cargo 输出共用一个判定函数，2026-10-17）
- 实测证据: amberlog 流水线 #462（2026-10-16）`test` 47 分钟、`knowledge` 114 分钟；nextest 0.9.146 JUnit 报告结构，2026-10-17 探针 crate 实测
- 代码位置: `src/spec_verify/test_batch.rs` 的 `attribute` 与 `parse_test_lines`
- 补充边界: 报告不可读时报错不判定、二进制 id 前缀以 `P::` 区分同前缀 package、`classname` 缺省取测试套件名，ADR-007 未逐条写出，2026-10-17 拆分需求时补充

## Open Questions

None.
