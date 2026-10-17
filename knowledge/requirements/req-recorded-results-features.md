---
kind: requirement
id: REQ-RECORDED-RESULTS-FEATURES
title: "Recorded Results Features"
status: accepted
liveness: auto
tags: [verify, guard, ci, cargo]
---

# Recorded Results Features

## Problem

JUnit 报告不记录运行时开启了哪些 cargo features。按报告判定（REQ-RECORDED-TEST-RESULTS）
时，guard 无从知道声明了 `Features:` 的场景，其测试是否在开启这些 features 的
构建里运行。下游 amberlog 的 CI 测试运行对整个工作区开启测试专用 feature
`amberlog-server/fault-injection`，148 个场景声明了它，其余场景没有声明。

## Requirements

[REQ-RECORDED-RESULTS-FEATURES-FLAG] guard、lifecycle 与 verify MUST 接受参数 `--test-results-features <列表>`，列表以逗号分隔，每项写作 `<package>/<feature>` 或 `<feature>`。

[REQ-RECORDED-RESULTS-FEATURES-REQUIRES-RESULTS] 未给出 `--test-results` 而给出 `--test-results-features` 时，命令 MUST 以参数错误退出，且不产生任何场景判定。

[REQ-RECORDED-RESULTS-FEATURES-COVERED] 声明了 `Features: F` 与 `Package: P` 的场景，当 F 中每个 feature f 都有声明项 `P/f` 或 `f` 时，MUST 按报告判定。

[REQ-RECORDED-RESULTS-FEATURES-MISMATCH] 声明了 features 的场景，只要其中有一个 feature 未被声明项覆盖，MUST 判 `uncertain`，原因为 results-feature-mismatch。

[REQ-RECORDED-RESULTS-FEATURES-MISSING-LISTED] 场景因 results-feature-mismatch 判 `uncertain` 时，guard 的输出 MUST 列出该场景缺少的 features。

[REQ-RECORDED-RESULTS-FEATURES-NO-PACKAGE] 声明了 features、未声明 package 的场景，其每个 feature f MUST 只由声明项 `f` 覆盖。

[REQ-RECORDED-RESULTS-FEATURES-UNDECLARED-SCENARIO] 未声明 features 的场景 MUST 按报告判定，不论声明项列出了哪些 features。

[REQ-RECORDED-RESULTS-FEATURES-SUMMARY] 给出 `--test-results-features` 时，guard 的汇总行 MUST 列出全部声明项。

## Scenarios

Rule: REQ-RECORDED-RESULTS-FEATURES-FLAG

Scenario: 解析两个声明项
  Given 一份报告含场景 S1、S2 绑定的通过用例；S1 声明 `Package: alpha`、`Features: fi`，S2 声明 `Package: beta`、`Features: slow`
  When 以 `--test-results r.xml --test-results-features alpha/fi,beta/slow` 运行 verify
  Then 输出的 S1 与 S2 verdict 都等于 `pass`

Rule: REQ-RECORDED-RESULTS-FEATURES-REQUIRES-RESULTS

Scenario: 只给 features 不给报告
  Given 一个含绑定场景的 spec 目录
  When 运行 `agent-spec guard --spec-dir specs --code . --test-results-features alpha/fi`
  Then 退出码非 0 且输出不包含任何场景 verdict

Rule: REQ-RECORDED-RESULTS-FEATURES-COVERED

Scenario: 带 package 的声明项覆盖场景 feature
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi`，报告中 `alpha::sys` / `case_f` 通过
  When 以 `--test-results-features alpha/fi` 运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Scenario: 不带 package 的声明项覆盖场景 feature
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi`，报告中 `alpha::sys` / `case_f` 通过
  When 以 `--test-results-features fi` 运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Scenario: 被覆盖的场景照报告判失败
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi`，报告中 `alpha::sys` / `case_f` 带 `<failure>`
  When 以 `--test-results-features alpha/fi` 运行 verify
  Then 输出的该场景 verdict 等于 `fail`

Rule: REQ-RECORDED-RESULTS-FEATURES-MISMATCH

Scenario: 未声明场景要的 feature
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi`，报告中 `alpha::sys` / `case_f` 通过
  When 以 `--test-results r.xml` 运行 verify，不给 `--test-results-features`
  Then 输出的该场景 verdict 等于 `uncertain` 且原因为 results-feature-mismatch

Scenario: 声明在另一个 package 上
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi`，报告中 `alpha::sys` / `case_f` 通过
  When 以 `--test-results-features beta/fi` 运行 verify
  Then 输出的该场景 verdict 等于 `uncertain` 且原因为 results-feature-mismatch

Scenario: 只覆盖了一部分 features
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi,slow`，报告中 `alpha::sys` / `case_f` 通过
  When 以 `--test-results-features alpha/fi` 运行 verify
  Then 输出的该场景 verdict 等于 `uncertain` 且原因为 results-feature-mismatch

Rule: REQ-RECORDED-RESULTS-FEATURES-MISSING-LISTED

Scenario: guard 列出缺少的 feature
  Given 场景 selector 为 `Package: alpha`、`Filter: case_f`、`Features: fi,slow`，报告中 `alpha::sys` / `case_f` 通过
  When 以 `--test-results-features alpha/fi` 运行 guard
  Then 输出包含 `slow` 且不把该场景计为通过

Rule: REQ-RECORDED-RESULTS-FEATURES-NO-PACKAGE

Scenario: 无 package 场景不被带 package 的声明项覆盖
  Given 场景 selector 为 `Filter: case_g`、`Features: fi`，报告中 `alpha::sys` / `case_g` 通过
  When 以 `--test-results-features alpha/fi` 运行 verify
  Then 输出的该场景 verdict 等于 `uncertain` 且原因为 results-feature-mismatch

Rule: REQ-RECORDED-RESULTS-FEATURES-UNDECLARED-SCENARIO

Scenario: 未声明 features 的场景接受多开 feature 的运行
  Given 场景 selector 为 `Package: alpha`、`Filter: case_h`，报告中 `alpha::sys` / `case_h` 通过
  When 以 `--test-results-features alpha/fi` 运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Scenario: 不给声明项时未声明 features 的场景照常判定
  Given 场景 selector 为 `Package: alpha`、`Filter: case_h`，报告中 `alpha::sys` / `case_h` 通过
  When 以 `--test-results r.xml` 运行 verify，不给 `--test-results-features`
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-RECORDED-RESULTS-FEATURES-SUMMARY

Scenario: 汇总行列出声明项
  Given 一份报告，全部场景通过
  When 以 `--test-results-features alpha/fi,beta/slow` 运行 guard
  Then 输出的汇总行包含 `alpha/fi` 与 `beta/slow`

## Dependencies

- REQ-RECORDED-TEST-RESULTS（本需求只约束按报告判定时的 features 条件）
- REQ-SELECTOR-FEATURES（场景以 `Features:` 声明所需 features）

## Source Trace

- proposal: LEP-008 D3 (knowledge/proposals/2026-10-17-test-runners-and-recorded-results.md)
- decision: ADR-007 D3（调用方声明运行时 features；声明了 features 的场景须被覆盖，否则 uncertain；未声明 features 的场景接受多开 features 的运行，2026-10-17 下游选项 B）
- 实测证据: amberlog 2026-10-17，931 个 selector 中 148 个声明 `特性: fault-injection`；CI `test` 作业以 `--features amberlog-server/fault-injection` 运行全部测试
- 补充边界: 无 package 场景只由不带 package 的声明项覆盖、未给报告时 features 参数报错，ADR-007 未逐条写出，2026-10-17 拆分需求时补充

## Open Questions

None.
