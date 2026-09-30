---
kind: requirement
id: REQ-BATCHED-TEST-EXECUTION
title: "Batched Test Execution"
status: accepted
liveness: auto
tags: [verify, guard, lifecycle, performance]
---

# Batched Test Execution

## Problem

测试验证器对每个绑定场景单独启动一次 `cargo test -q [-p <package>] <filter>`，
guard 又逐个 spec 串行验证。amberlog（17 个合约、318 个场景，其中 305 个绑定
同一个 package）一次完整 guard 约 26 分钟：51 个耗时超过 20 秒的测试约占
15 分钟，其余约 285 次调用约占 11 分钟，其中每次约 1 秒是调用本身的固定开销
（cargo 的 up-to-date 检查，以及远程构建包装器的同步与 ssh 往返），不是测试
时间。同样的测试合成一次 `cargo test -p <package>` 只需几分钟：libtest 在一个
二进制内并行运行测试，并接受多个 filter。按现在的代价 guard 留不住 pre-commit，
下游把它挪进 CI，红掉的合约要到推送之后才被发现。

## Requirements

### 分组

[REQ-BATCHED-TEST-EXECUTION-GROUP] guard、lifecycle 与 verify MUST 默认把所有待验证 spec 中绑定的 selector 按 (package, features, 是否 soak) 分组，每组只调用一次 cargo test。

[REQ-BATCHED-TEST-EXECUTION-ARGS] 每组的 cargo 调用 MUST 把组内全部 filter 作为 libtest 参数传入。

[REQ-BATCHED-TEST-EXECUTION-NO-FAIL-FAST] 每组的 cargo 调用 MUST 带 `--no-fail-fast`。

[REQ-BATCHED-TEST-EXECUTION-NOT-QUIET] 每组的 cargo 调用 MUST NOT 带 `-q`。

[REQ-BATCHED-TEST-EXECUTION-THREADS-DEFAULT] 未指定 `--test-threads` 时，每组调用 MUST 向 libtest 传 `--test-threads=1`。

[REQ-BATCHED-TEST-EXECUTION-THREADS-FLAG] 指定 `--test-threads <N>` 时，每组调用 MUST 向 libtest 传 `--test-threads=<N>`。

[REQ-BATCHED-TEST-EXECUTION-NO-BATCH] 指定 `--no-batch` 时，测试验证器 MUST 对每个场景单独调用一次 cargo test，参数与引入本需求前相同。

### 回填判定

[REQ-BATCHED-TEST-EXECUTION-ATTRIBUTE] 场景的判定 MUST 仅由组输出中测试路径包含该场景 filter 的逐测试结果行决定。

[REQ-BATCHED-TEST-EXECUTION-NONE-MATCHED] 组输出中没有测试路径包含场景 filter 时，该场景 MUST 判为 `skip`。

[REQ-BATCHED-TEST-EXECUTION-ALL-IGNORED] 匹配场景 filter 的测试全部为 ignored 时，该场景 MUST 判为 `skip`，原因包含 `bound test is #[ignore]d`。

[REQ-BATCHED-TEST-EXECUTION-ANY-FAILED] 匹配场景 filter 的已执行测试中有任何一个失败时，该场景 MUST 判为 `fail`。

[REQ-BATCHED-TEST-EXECUTION-ALL-PASSED] 匹配场景 filter 的已执行测试全部通过、且场景未声明 `Review: human` 时，该场景 MUST 判为 `pass`。

[REQ-BATCHED-TEST-EXECUTION-HUMAN-REVIEW] 匹配场景 filter 的已执行测试全部通过、且场景声明 `Review: human` 时，该场景 MUST 判为 `pending_review`。

[REQ-BATCHED-TEST-EXECUTION-BUILD-FAILURE] 组调用以非零状态退出且输出中没有任何逐测试结果行时，该组全部场景 MUST 判为 `uncertain`。

[REQ-BATCHED-TEST-EXECUTION-UNPARSEABLE] 组调用成功退出但输出中没有可识别的逐测试结果行时，测试验证器 MUST 对该组场景逐个单独调用 cargo test。

### 耗时

[REQ-BATCHED-TEST-EXECUTION-DURATION] 组输出为匹配测试报告了各自耗时时，场景的 `duration_ms` MUST 等于这些耗时之和。

[REQ-BATCHED-TEST-EXECUTION-DURATION-ESTIMATE] 组输出未报告逐测试耗时时，场景的 `duration_ms` MUST 等于组的墙钟时间除以组内场景数，并标注为估算值。

## Scenarios

### 分组

Rule: REQ-BATCHED-TEST-EXECUTION-GROUP

Scenario: 同 package 的场景合并为一次调用
  Given 两个 spec 共五个场景，全部绑定 package `probe` 且未声明 features
  When 运行 guard
  Then 调用日志文件中记录的 cargo test 调用次数等于 1

Scenario: 不同 features 分开调用
  Given 三个绑定 package `probe` 的场景，其中一个声明 `Features: slow-io`
  When 运行 verify
  Then 调用日志文件中记录的 cargo test 调用次数等于 2

Scenario: soak 场景与普通场景分开调用
  Given 两个绑定 package `probe` 的场景，其中一个为 soak 模式
  When 运行 verify --include-soak
  Then 调用日志文件中记录的 cargo test 调用次数等于 2，且只有 soak 组的调用参数包含 `--include-ignored`

Rule: REQ-BATCHED-TEST-EXECUTION-ARGS

Scenario: 组调用传入全部 filter
  Given 两个绑定 package `probe` 的场景，filter 分别为 `alpha` 与 `beta`
  When 生成组调用参数
  Then 返回的参数包含 `alpha` 与 `beta`

Rule: REQ-BATCHED-TEST-EXECUTION-NO-FAIL-FAST

Scenario: 组调用不在首个失败时停止
  Given 同上场景
  When 生成组调用参数
  Then 返回的参数包含 `--no-fail-fast`

Rule: REQ-BATCHED-TEST-EXECUTION-NOT-QUIET

Scenario: 组调用保留逐测试输出
  Given 同上场景
  When 生成组调用参数
  Then 返回的参数不包含 `-q`

Rule: REQ-BATCHED-TEST-EXECUTION-THREADS-DEFAULT

Scenario: 默认单线程
  Given 同上场景
  When 生成组调用参数
  Then 返回的参数包含 `--test-threads=1`

Rule: REQ-BATCHED-TEST-EXECUTION-THREADS-FLAG

Scenario: 指定线程数
  Given 同上场景
  When 以 --test-threads 4 生成组调用参数
  Then 返回的参数包含 `--test-threads=4`，且不包含 `--test-threads=1`

Rule: REQ-BATCHED-TEST-EXECUTION-NO-BATCH

Scenario: no-batch 退回逐场景调用
  Given 两个绑定 package `probe` 的场景
  When 运行 verify --no-batch
  Then 调用日志文件中记录的 cargo test 调用次数等于 2，每次调用的参数包含 `-q`

### 回填判定

Rule: REQ-BATCHED-TEST-EXECUTION-ATTRIBUTE

Scenario: 按 filter 子串回填
  Given 组输出含 `test tests::alpha_one ... ok` 与 `test tests::beta_one ... FAILED`，场景 A 的 filter 为 `alpha`
  When 回填场景判定
  Then 返回的场景 A 的 verdict 等于 `pass`

Rule: REQ-BATCHED-TEST-EXECUTION-NONE-MATCHED

Scenario: 无匹配测试判 skip
  Given 组输出只含 `test tests::alpha_one ... ok`，场景 C 的 filter 为 `gamma`
  When 回填场景判定
  Then 返回的场景 C 的 verdict 等于 `skip`

Rule: REQ-BATCHED-TEST-EXECUTION-ALL-IGNORED

Scenario: 匹配测试全部 ignored 判 skip
  Given 组输出含 `test tests::delta_one ... ignored`，场景 D 的 filter 为 `delta`
  When 回填场景判定
  Then 返回的场景 D 的 verdict 等于 `skip`，原因包含 `bound test is #[ignore]d`

Rule: REQ-BATCHED-TEST-EXECUTION-ANY-FAILED

Scenario: 任一匹配测试失败判 fail
  Given 组输出含 `test tests::beta_one ... ok` 与 `test tests::beta_two ... FAILED`，场景 B 的 filter 为 `beta`
  When 回填场景判定
  Then 返回的场景 B 的 verdict 等于 `fail`

Rule: REQ-BATCHED-TEST-EXECUTION-ALL-PASSED

Scenario: 匹配测试全部通过判 pass
  Given 组输出含 `test tests::alpha_one ... ok` 与 `test tests::alpha_two ... ok`，场景 A 的 filter 为 `alpha`
  When 回填场景判定
  Then 返回的场景 A 的 verdict 等于 `pass`

Rule: REQ-BATCHED-TEST-EXECUTION-HUMAN-REVIEW

Scenario: human review 场景全部通过时为 pending_review
  Given 组输出含 `test tests::alpha_one ... ok`，场景 A 的 filter 为 `alpha` 且声明 `Review: human`
  When 回填场景判定
  Then 返回的场景 A 的 verdict 等于 `pending_review`

Rule: REQ-BATCHED-TEST-EXECUTION-BUILD-FAILURE

Scenario: 构建失败时整组 uncertain
  Given 组调用以非零状态退出，输出含 `error[E0425]` 且没有任何 `test ` 结果行
  When 回填场景判定
  Then 返回的该组每个场景的 verdict 都等于 `uncertain`

Rule: REQ-BATCHED-TEST-EXECUTION-UNPARSEABLE

Scenario: 无法识别的输出回退为逐场景调用
  Given 一个 `harness = false` 的测试目标，组调用成功退出但输出不含 `test ` 结果行，组内有两个场景
  When 运行 verify
  Then 调用日志文件中记录的 cargo test 调用次数等于 3

### 耗时

Rule: REQ-BATCHED-TEST-EXECUTION-DURATION

Scenario: 按报告的逐测试耗时累加
  Given 组输出为场景 A 匹配的两个测试分别报告 1.5s 与 0.5s
  When 回填场景判定
  Then 返回的场景 A 的 duration_ms 等于 2000

Rule: REQ-BATCHED-TEST-EXECUTION-DURATION-ESTIMATE

Scenario: 无逐测试耗时时均摊并标注估算
  Given 一个含四个场景的组，墙钟时间 8000 ms，输出未报告逐测试耗时
  When 回填场景判定
  Then 返回的每个场景的 duration_ms 等于 2000，且 duration_estimated 字段为 true

## Dependencies

- REQ-HONEST-TEST-VERDICTS（回填判定沿用 ignored 不算执行的规则）
- REQ-SELECTOR-FEATURES（分组键包含 selector 的 features）
- REQ-SOAK-SCENARIOS（soak 场景单独成组）

## Source Trace

- proposal: LEP-007 Gap 7 (knowledge/proposals/2026-09-28-staged-coverage-and-honest-test-verdicts.md)
- decision: ADR-006 D7（按 package 与 features 分组批量执行，默认开启；默认单线程；`--no-batch` 退回；guard 不并行验证 spec，2026-09-30）
- 实测证据: amberlog 2026-09-30，17 个合约、318 个场景的完整 guard 约 26 分钟，其中约 285 次调用的固定开销约 11 分钟
- 代码位置: `src/spec_verify/test_verifier.rs` 对每个绑定场景各起一次 `cargo test -q`
- 拆分时的推导: soak 场景单独成组，因为 `--include-ignored` 会让同组普通场景匹配到的 ignored 测试也被执行
- 暂缓: ADR-006 提到的 `[verify] test_threads` 配置项未纳入本需求，agent-spec 目前没有配置文件机制，仅提供 `--test-threads` 参数；2026-09-30 拆分需求时经人工确认

## Open Questions

None.
