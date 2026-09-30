---
kind: requirement
id: REQ-SOAK-SCENARIOS
title: "Soak Scenarios"
status: accepted
liveness: auto
tags: [verify, guard, lifecycle, authoring]
---

# Soak Scenarios

## Problem

有些已接受的场景按定义就是长时运行测试（amberlog：「持续摄入 24 小时的实例
没有删除任何文件」）。guard 每次提交都运行全部绑定测试，这类场景只能绑到一个
会 `skip` 的测试（门禁变红）、一个 `#[ignore]` 的测试（假 pass）、或一个被
缩短的测试（判定的含义被悄悄收窄）。`Mode: optimize` 与 `Review: human` 都
帮不上：两者仍要求测试在 guard 下运行，而 `pending_review` 在默认 `auto`
审查模式下算通过。

同时，解析器把任何无法识别的 `Mode:` 值静默当作 `Standard`、把无法识别的
`Review:` 值静默当作 `Auto`，旧版本遇到新取值时不会给出任何提示；selector 上
的自由字符串 `Level: soak` 已有下游在用，却不影响任何门禁。

## Requirements

### 暂缓

[REQ-SOAK-SCENARIOS-MODE] 场景 MUST 接受 `Mode: soak` 与 `模式: soak`（全角冒号同样有效），解析为 soak 模式。

[REQ-SOAK-SCENARIOS-DEFER] verify、lifecycle 与 guard 在未指定 `--include-soak` 时 MUST NOT 运行 soak 场景绑定的测试。

[REQ-SOAK-SCENARIOS-DEFERRED-VERDICT] 未运行的 soak 场景 MUST 以判定 `deferred` 报告。

[REQ-SOAK-SCENARIOS-DEFERRED-COUNT] 验证汇总 MUST 以独立计数 `deferred` 统计以 `deferred` 报告的场景。

[REQ-SOAK-SCENARIOS-DEFERRED-PASSING] 在默认审查模式下，`deferred` 场景 MUST NOT 使 spec 的通过判定为假。

### 显式运行

[REQ-SOAK-SCENARIOS-INCLUDE] 指定 `--include-soak` 时，verify 与 lifecycle MUST 运行 soak 场景并给出其真实判定。

[REQ-SOAK-SCENARIOS-INCLUDE-IGNORED] 运行 soak 场景时，测试验证器 MUST 向 libtest 传 `--include-ignored`，使标注 `#[ignore]` 的 soak 测试得以执行。

### 严格验收

[REQ-SOAK-SCENARIOS-STRICT] 在 `--review-mode strict` 下，`deferred` 场景 MUST 使 spec 的通过判定为假。

[REQ-SOAK-SCENARIOS-RESOLVE-AI] `resolve-ai` 合并后仍为 `deferred` 的场景 MUST 计为未通过。

### 字段一致性

[REQ-SOAK-SCENARIOS-LEVEL-WITHOUT-MODE] selector 声明 `Level: soak` 而其场景未声明 `Mode: soak` 时，spec lint MUST 报 Warning `soak-level-without-mode`。

[REQ-SOAK-SCENARIOS-UNKNOWN-MODE] 场景的 `Mode:` 值不是可识别取值时，spec lint MUST 报 Warning `unknown-scenario-mode`。

[REQ-SOAK-SCENARIOS-UNKNOWN-REVIEW] 场景的 `Review:` 值不是可识别取值时，spec lint MUST 报 Warning `unknown-review-mode`。

[REQ-SOAK-SCENARIOS-UNKNOWN-FALLBACK] 无法识别的 `Mode:` 与 `Review:` 值 MUST 继续分别按 `Standard` 与 `Auto` 解析。

## Scenarios

### 暂缓

Rule: REQ-SOAK-SCENARIOS-MODE

Scenario: 解析 soak 模式
  Given 一个场景声明 `Mode: soak`，另一个场景声明 `模式：soak`
  When 解析该 spec
  Then 返回的两个场景的 mode 字段都等于 Soak

Rule: REQ-SOAK-SCENARIOS-DEFER

Scenario: 默认不运行 soak 测试
  Given 一个 soak 场景绑定的测试在运行时写入标记文件
  When 运行 guard
  Then 该标记文件不存在

Rule: REQ-SOAK-SCENARIOS-DEFERRED-VERDICT

Scenario: soak 场景报告为 deferred
  Given 一个 spec 含一个 soak 场景
  When 运行 verify --format json
  Then 输出的该场景 verdict 等于 `deferred`

Rule: REQ-SOAK-SCENARIOS-DEFERRED-COUNT

Scenario: 汇总含 deferred 计数
  Given 一个 spec 含两个通过的场景与一个 soak 场景
  When 运行 verify --format json
  Then 输出的 summary 中 deferred 等于 1，passed 等于 2

Rule: REQ-SOAK-SCENARIOS-DEFERRED-PASSING

Scenario: deferred 不阻塞 guard
  Given 一个 spec 的场景除一个 soak 场景外全部通过
  When 运行 guard
  Then 进程以退出码 0 exits

Rule: REQ-SOAK-SCENARIOS-INCLUDE

Scenario: 显式运行 soak 场景
  Given 一个 soak 场景绑定一个失败的测试
  When 运行 verify --include-soak --format json
  Then 输出的该场景 verdict 等于 `fail`

Rule: REQ-SOAK-SCENARIOS-INCLUDE-IGNORED

Scenario: 被忽略的 soak 测试在显式运行时执行
  Given 一个 soak 场景绑定一个标注 `#[ignore]` 且通过的测试
  When 运行 verify --include-soak --format json
  Then 输出的该场景 verdict 等于 `pass`

### 严格验收

Rule: REQ-SOAK-SCENARIOS-STRICT

Scenario: 严格审查下 deferred 不算通过
  Given 一个 spec 的场景除一个 soak 场景外全部通过
  When 运行 lifecycle --review-mode strict
  Then 进程以非零退出码 exits

Rule: REQ-SOAK-SCENARIOS-RESOLVE-AI

Scenario: resolve-ai 后仍为 deferred 的场景计为未通过
  Given 一份含一个 `deferred` 场景的验证报告，以及不含该场景的 decisions 文件
  When 运行 resolve-ai
  Then 输出的报告中 spec 的通过判定为 false

### 字段一致性

Rule: REQ-SOAK-SCENARIOS-LEVEL-WITHOUT-MODE

Scenario: Level soak 而无 Mode soak 时警告
  Given 一个场景的 selector 含 `Level: soak`，场景未声明 `Mode:`
  When 运行 agent-spec lint
  Then 输出包含 `soak-level-without-mode`

Rule: REQ-SOAK-SCENARIOS-UNKNOWN-MODE

Scenario: 未知 Mode 值时警告
  Given 一个场景声明 `Mode: soke`
  When 运行 agent-spec lint
  Then 输出包含 `unknown-scenario-mode`

Rule: REQ-SOAK-SCENARIOS-UNKNOWN-REVIEW

Scenario: 未知 Review 值时警告
  Given 一个场景声明 `Review: humans`
  When 运行 agent-spec lint
  Then 输出包含 `unknown-review-mode`

Rule: REQ-SOAK-SCENARIOS-UNKNOWN-FALLBACK

Scenario: 未知取值保持原有回落
  Given 一个场景声明 `Mode: soke` 与 `Review: humans`
  When 解析该 spec
  Then 返回的场景 mode 字段等于 Standard，review 字段等于 Auto

## Dependencies

- REQ-HONEST-TEST-VERDICTS（soak 测试常标注 `#[ignore]`，其判定依赖 ignored 不算执行的规则）

## Source Trace

- proposal: LEP-007 Gap 4 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D4（`Mode: soak` 为唯一暂缓开关，`deferred` 计数，`--include-soak`，严格路径不放行，`Level`/`Mode` 一致性与未知取值 lint，2026-09-30）
- 实测证据: amberlog 场景「持续摄入 24 小时的实例没有删除任何文件」无法在每次提交的 guard 下诚实验证
- 代码位置: `src/spec_parser/parser.rs` 把 `optimize` 以外的 `Mode:` 值映射为 `Standard`、把 `human` 以外的 `Review:` 值映射为 `Auto`；`VerificationSummary` 已有独立的 `pending_review` 计数
- 表示选择: 以新判定值 `deferred` 表示暂缓场景，与 `pending_review` 同构；ADR-006 只规定汇总计数，2026-09-30 拆分需求时经人工确认
- 先例: libtest 的 `--ignored` / `--include-ignored`

## Open Questions

None.
