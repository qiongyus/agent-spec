---
kind: decision
id: ADR-006
title: "Staged Coverage and Honest Test Verdicts"
status: accepted
tags: [knowledge, verify, guard, requirements]
---

# Staged Coverage and Honest Test Verdicts

## Context

LEP-007（accepted 2026-09-30）记录了下游工作区 amberlog 把 17 个需求块、
300 余个场景接入 CI 与 pre-commit 时暴露的八个缺口，均在 agent-spec 1.4.0
上复现。它们分两类：一类让绿色门禁不再意味着「合约成立」——`#[ignore]` 的
测试被判 `pass`、soak 场景无处安放、依赖被 supersede 的块时分批静默出错；
另一类让门禁明确变红，逼下游写绕过脚本——roadmap 里的合约报
`requirement-uncovered`、selector 无法请求 cargo feature、guard 用每个合约的
边界去查整个提交、guard 每个场景起一次 `cargo test` 导致全量运行约 26 分钟、
`uncertain` 不说原因。

根本矛盾在第一个缺口：让规格领先实现一轮的项目，知识门禁与合约门禁在现有
规则下永远不能同时为绿。

## Decision

采纳 LEP-007 的 D1–D8，并按 2026-09-30 对其十一项未决问题的裁决定稿：

- **D1 staged 覆盖：** `requirements plan` 把 `specs/roadmap/` 作为 staged 层
  纳入覆盖统计，默认开启、不设开关。只有 staged 合约的就绪需求报 Info
  `requirement-staged`；0 active + ≥2 staged 报 Error
  `requirement-multiple-staged-specs`；active 与 staged 并存报 Warning
  `requirement-staged-and-active`；其余分类不变。`test-obligations` 在无
  active 合约时以 staged 合约为义务来源并标注 `staged`。
- **D2 ignored 不算执行：** 判定依据改为 libtest 的 `test result:` 汇总行，
  executed = passed + failed。全部匹配被 ignore 判 `skip` 并注明
  "bound test is #[ignore]d"；部分被 ignore 时按已执行的判定，ignored 数记入
  证据；汇总行缺失或无法解析退回 `uncertain`，绝不退回 `pass`。
- **D3 `Features:` / `特性:`：** selector 新增可选字段，原样透传为
  `--features`。
- **D4 `Mode: soak` / `模式: soak`：** 唯一的暂缓开关。guard 与 lifecycle
  不运行 soak 场景，计入新汇总项 `deferred`，不影响 `is_passing`；
  `--include-soak` 运行它们并传 `-- --include-ignored`；`resolve-ai` 与
  `--review-mode strict` 把无运行记录的 soak 场景视为未通过。`Level:` 保持
  描述性；`lint` 对「`Level: soak` 而无 `Mode: soak`」报
  `soak-level-without-mode`，并对无法识别的 `Mode:` / `Review:` 值报
  `unknown-scenario-mode` / `unknown-review-mode`，不再静默回落到
  `Standard` / `Auto`。
- **D5 依赖经 supersede 解析：** `## Dependencies` 中被 supersede 的 id 沿
  `supersedes:` 前行到第一个未被取代的块；proposed 后继不取代 accepted 块；
  一个块被多个后继取代（拆分）时解析到全部后继，各自按同一规则继续。无后继
  或 supersedes 链成环报 **Error** `dependency-supersession-unresolved`，边保留
  声明 id。解析只是视图，图中声明 id 不变，`requirements export` 仍可往返；
  plan 边在改写时携带 `declared_as`。
- **D6 边界按合约领地生效：** 一个合约的 `Forbidden` 与 `Allowed changes`
  只约束「触及其领地」的提交——至少一条改动路径匹配其 allow-list，或就是
  合约文件本身（或从 `specs/roadmap/` 移入时的源路径）。在这样的提交里，
  所有路径都受该合约 `Forbidden` 检查；路径归属于该合约（匹配其 allow-list、
  是合约文件或移动源、或不匹配任何 active 合约的 allow-list）时再受
  `Allowed changes` 检查。不触及任何合约领地的提交不做边界检查。
  `guard --strict-boundaries` 保留原行为。
- **D7 按 package 批量执行：** 默认按 `(package, features)` 分组，每组一次
  `cargo test --no-fail-fast -- <filter>…`，不加 `-q`，按逐测试结果行与子串
  匹配回填各场景判定（沿用 D2 规则）；默认 `--test-threads=1`，可由
  `--test-threads <N>` 或 `[verify] test_threads` 覆盖；整组构建失败、没有任何
  逐测试行时，该组场景判 `uncertain`；输出格式无法识别的组（如
  `harness = false`）回退为逐场景调用；`--no-batch` 保留旧行为。guard 不并行
  验证 spec。
- **D8 说明非通过的原因：** guard 对每个 `fail` / `uncertain` 场景打印场景名、
  selector、退出码或信号与输出尾部；`uncertain` 区分「cargo 报告编译错误」与
  「runner 退出且无 cargo 输出」。agent-spec 不自动重试，也不提供重试开关。

D5 的首版实现（`e38c34f`）以 Warning 报告无法解析的依赖、把拆分视为无法
解析，须按本决策修正。

## Consequences

Good, because 规格领先实现一轮的工作方式下，`lint-knowledge --gate` 与
`guard` 可以同时为绿，下游的豁免脚本与 `--change-scope none` 可以撤掉。

Good, because 没有任何场景能在其测试未执行时被判 `pass`：`#[ignore]`、缺失
的 feature、soak 暂缓都各有显式的非通过或 deferred 状态。

Good, because 分批与依赖顺序不再静默出错：能解析的依赖被正确解析，不能解析
的直接让门禁失败。

Good, because guard 的代价从每场景一次 cargo 调用降到每 package 一次，重新
放得进 pre-commit。

Bad, because 诊断词表增加四个 knowledge 诊断码、三个 spec lint 警告与一个
汇总计数，文档与技能需同步。

Bad, because D2、D5、D6 在升级时改变行为：依赖 ignored 测试的 spec 转红，
supersede 数据有误的工作区门禁失败，边界检查对不触及合约领地的提交放行。

Bad, because D6 允许把越界改动与其所属实现拆成两次提交来绕过边界检查；
缓解靠 `--strict-boundaries` 与按提交范围评审。

Bad, because D7 让同一 package 的测试共享进程树，泄漏全局状态的测试可能
影响后续测试的结果；缓解靠 `--no-batch` 与逐测试结果行。

## Alternatives Considered

- 下游豁免脚本（amberlog 现行做法）—— 被否：解析文本输出，每个采用者都要
  重建，且随输出格式漂移失效。
- CI 中对知识门禁 `allow_failure` —— 被否：同时掩盖门禁要捕获的其他 Error。
- 合约留在 `specs/` 并绑定占位测试 —— 被否：下游构建规则禁止占位进入实现，
  且按 D2 的逻辑，不运行的占位不得通过。
- 以 `Review: human` 充当 soak 机制 —— 被否：测试仍在 guard 下运行，且
  `pending_review` 在 `auto` 模式下算通过。
- 以现有 `Level: soak` 作为暂缓开关 —— 被否：`Level:` 是自由字符串，把其中
  一个取值赋予门禁语义会让已有 spec 在升级时静默改变行为；改为 lint 检查两者
  一致。
- `Forbidden` 全局生效、按提交者区分治理提交 —— 被否：区分提交者需要身份
  字段，ADR-001 排除此类字段。

## Source Trace

- proposal: LEP-007
  (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md,
  accepted 2026-09-30)
- 十一项裁决来源: 2026-09-30 结构化多选交互，对应 LEP-007 的
  `## Resolved Questions`
- 实测证据: amberlog 2026-09-28 至 2026-09-30，17 个合约、313–318 个场景；
  全量 guard 约 26 分钟；45 条依赖边指向被 supersede 的块，plan 5 批对手工
  推导 9 层
- 先例: libtest 的 `--ignored` / `--include-ignored`；cargo 的
  `required-features`；`requirements status` 的 active / staged / archived
  三层；ADR-001 的无身份字段原则
- 后续: 依赖指向非 Ready 块时依赖边脱离分批的既有规则，留待后续提案
- governed requirements: REQ-STAGED-COVERAGE（D1）、REQ-HONEST-TEST-VERDICTS（D2、D8）、REQ-SELECTOR-FEATURES（D3）、REQ-SOAK-SCENARIOS（D4）、REQ-SUPERSESSION-DEPENDENCY-RESOLUTION（D5）、REQ-GUARD-CONTRACT-BOUNDARIES（D6）、REQ-BATCHED-TEST-EXECUTION（D7）
