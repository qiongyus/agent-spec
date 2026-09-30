---
kind: requirement
id: REQ-SELECTOR-FEATURES
title: "Test Selector Features"
status: accepted
liveness: auto
tags: [verify, authoring, cargo]
---

# Test Selector Features

## Problem

`TestSelector` 只能表达 filter、package、level、test double 与 targets，
测试验证器只向 cargo 传 `-p` 与 filter。依赖测试专用 cargo feature 的合约
测试（amberlog 把崩溃注入暂停点放在 `fault-injection` feature 后面，确保它们
不进入 release 构建）在 guard 下被编译掉：filter 匹配不到任何测试，场景判
`skip`。下游的绕法是给 crate 加一条指向自身的 dev-dependency，它能用，但不
直观、在 `Cargo.lock` 里留下自引用边，而且无法表达「只有这一个场景需要
feature X」。

## Requirements

[REQ-SELECTOR-FEATURES-FIELD] 测试 selector MUST 接受可选字段 `Features:` 与 `特性:`（全角冒号同样有效），其值为逗号分隔的 cargo feature 列表。

[REQ-SELECTOR-FEATURES-ARGS] selector 声明了 features 时，测试验证器 MUST 在 cargo test 调用中附加 `--features <值>`。

[REQ-SELECTOR-FEATURES-ABSENT] selector 未声明 features 时，测试验证器生成的 cargo 参数 MUST 与引入本需求前完全相同。

[REQ-SELECTOR-FEATURES-EVIDENCE] 声明了 features 的场景，其测试证据 MUST 记录该 features 值。

[REQ-SELECTOR-FEATURES-EMPTY] 值为空的 `Features:` 字段 MUST 产生 spec lint 诊断。

[REQ-SELECTOR-FEATURES-EMPTY-NO-ARG] 值为空的 `Features:` 字段 MUST NOT 使 cargo 调用带上 `--features`。

## Scenarios

Rule: REQ-SELECTOR-FEATURES-FIELD

Scenario: 解析英文 Features 字段
  Given 一个场景的 selector 含 `Filter: crash_recovery` 与 `Features: fault-injection,slow-io`
  When 解析该 spec
  Then 返回的 selector 的 features 字段等于 `fault-injection,slow-io`

Scenario: 解析中文特性字段
  Given 一个场景的 selector 含 `过滤: crash_recovery` 与 `特性：fault-injection`
  When 解析该 spec
  Then 返回的 selector 的 features 字段等于 `fault-injection`

Rule: REQ-SELECTOR-FEATURES-ARGS

Scenario: features 透传给 cargo
  Given 一个 selector 含 `Package: amberlog`、`Filter: crash_recovery` 与 `Features: fault-injection`
  When 生成 cargo test 参数
  Then 返回的参数列表包含相邻的 `--features` 与 `fault-injection`

Scenario: 需要 feature 的测试在 guard 下执行
  Given 一个只在 feature `probe` 开启时编译的测试，其场景 selector 声明 `Features: probe`
  When 运行 verify
  Then 输出的该场景 verdict 等于 `pass`

Rule: REQ-SELECTOR-FEATURES-ABSENT

Scenario: 未声明 features 时参数不变
  Given 一个 selector 只含 `Package: amberlog` 与 `Filter: crash_recovery`
  When 生成 cargo test 参数
  Then 返回的参数列表不包含 `--features`

Rule: REQ-SELECTOR-FEATURES-EVIDENCE

Scenario: 证据记录 features
  Given 一个 selector 声明 `Features: probe` 的场景
  When 运行 verify --format json
  Then 输出的该场景测试证据的 features 字段等于 `probe`

Rule: REQ-SELECTOR-FEATURES-EMPTY

Scenario: 空 features 字段报 lint
  Given 一个场景的 selector 含 `Features:` 且值为空
  When 运行 agent-spec lint
  Then 输出包含针对该场景的 features 为空的诊断

Rule: REQ-SELECTOR-FEATURES-EMPTY-NO-ARG

Scenario: 空 features 字段不传参数
  Given 一个 selector 含 `Filter: crash_recovery` 与值为空的 `Features:`
  When 生成 cargo test 参数
  Then 返回的参数列表不包含 `--features`

## Dependencies

None.

## Source Trace

- proposal: LEP-007 Gap 3 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D3（selector 新增可选 `Features:` / `特性:`，原样透传为 `--features`，2026-09-30）
- 实测证据: amberlog 的 `fault-injection` feature 下的测试在 guard 中报 `running 0 tests` 并判 `skip`；下游以自引用 dev-dependency 绕过
- 代码位置: `src/spec_verify/test_verifier.rs` 的 `build_cargo_test_args` 只传 `-p` 与 filter；`src/spec_parser/keywords.rs` 的 selector 字段识别
- 补充边界: 空值 `Features:` 的 lint 诊断与不传参数，ADR-006 未涉及，2026-09-30 拆分需求时补充并经人工确认
- 先例: cargo 的 `required-features`；selector 既有字段 `Package:` / `包:` 的中英文与全角冒号写法

## Open Questions

None.
