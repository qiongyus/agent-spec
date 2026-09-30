spec: task
name: "Honest Test Verdicts"
tags: [verify, guard, lifecycle]
satisfies: [REQ-HONEST-TEST-VERDICTS]
risk: B
---

## Intent

让测试判定只以真正执行过的测试为依据：已执行数取自 libtest 的 `test result:`
汇总行，被 `#[ignore]` 的测试不再让场景变成 `pass`；非通过判定带上退出状态与
失败类别，guard 直接打印原因，不必手工重跑（LEP-007 Gap 2、Gap 8，ADR-006 D2、D8）。

## Decisions

- 新增 `parse_libtest_summary(output) -> Option<LibtestSummary { passed, failed, ignored }>`，累加所有以 `test result:` 开头的行；没有这样的行时返回 `None`。
- 判定：cargo 非零退出且已执行数（passed + failed）> 0 → `fail`；非零退出且已执行数为 0 → `uncertain`；成功退出且无汇总行 → `uncertain`；成功退出、已执行数为 0、ignored > 0 → `skip`，原因含 `bound test is #[ignore]d`；成功退出、已执行数与 ignored 都为 0 → `skip`（原因不变）；成功退出且已执行数 > 0 → `pass`，`Review: human` 时 `pending_review`。
- `uncertain` 的类别：输出含以 `error[E` 开头的行或含 `could not compile` 时为 `compile-error`，否则为 `runner-exited`。
- `Evidence::TestOutput` 新增可选字段 `exit_code`、`signal`、`ignored`、`failure_class`，均 `skip_serializing_if` 为空；`ignored` 仅在大于 0 时写入。
- 新增 `TestCommandRunner` trait（`run(args, cwd) -> SpecResult<TestRun { status, stdout, stderr }>`），默认实现 `CargoRunner` 调用真实 cargo；`TestVerifier` 持有 runner，单元测试注入假 runner。agent-spec 不重试任何调用。
- guard 对每个 `fail` / `uncertain` 场景在对应 spec 的错误行下打印缩进的细节：场景名、selector、`exit code N` 或 `signal N`、`uncertain` 的类别、合并输出的最后至多 40 行；`pass` 场景不打印输出。guard 的全部输出沿用 stderr。

## Boundaries

### Allowed Changes
- src/spec_core/verify.rs
- src/spec_verify/**
- src/spec_gateway/**
- src/spec_report/**
- src/spec_knowledge/**
- src/main.rs
- CHANGELOG.md
- knowledge/requirements/req-honest-test-verdicts.md
- specs/task-honest-test-verdicts.spec.md
- specs/roadmap/task-honest-test-verdicts.spec.md

### Forbidden
- 不在已执行数为 0 时给出 `pass` 或 `pending_review`
- 不自动重试测试调用
- 不新增 crate 依赖

## Out of Scope

- 批量执行（REQ-BATCHED-TEST-EXECUTION）
- `--retry-uncertain` 之类的重试开关
- 测试调用超时

## Completion Criteria

场景: 已执行数与 ignored 数取自汇总行
  测试: test_parse_libtest_summary_sums_binaries_and_separates_ignored
  假设 一段 cargo 输出含 `test result: ok. 2 passed; 0 failed; 1 ignored` 与 `test result: ok. 1 passed; 0 failed; 0 ignored`
  当 解析汇总
  那么 passed 等于 3，failed 等于 0，ignored 等于 1

场景: 唯一匹配的测试被忽略时判 skip 并说明
  测试: test_verdict_is_skip_when_every_matched_test_is_ignored
  假设 一次成功退出的调用，输出含 `running 1 test` 与 `test result: ok. 0 passed; 0 failed; 1 ignored`
  当 计算场景判定
  那么 verdict 等于 `skip`，原因包含 `bound test is #[ignore]d`

场景: human review 场景零执行时不进入 pending_review
  测试: test_verdict_never_passes_or_pends_with_zero_executed
  假设 一个 `Review: human` 场景，其调用成功退出且汇总为 0 passed、0 failed、1 ignored
  当 计算场景判定
  那么 verdict 等于 `skip`

场景: 部分忽略时按已执行的判定并记录 ignored
  测试: test_verdict_uses_executed_tests_when_some_are_ignored
  假设 一次调用的汇总为 1 passed、0 failed、1 ignored，另一次为非零退出、0 passed、1 failed、1 ignored
  当 计算两个场景的判定与证据
  那么 第一个 verdict 等于 `pass`、第二个 verdict 等于 `fail`，两者证据的 ignored 都等于 1

场景: 缺少汇总行时判 uncertain
  测试: test_verdict_is_uncertain_without_summary_line
  假设 一次成功退出的调用，输出含 `running 1 test` 但不含 `test result:`
  当 计算场景判定
  那么 verdict 等于 `uncertain`

场景: 编译错误与 runner 退出分为两类
  测试: test_uncertain_distinguishes_compile_error_from_runner_exit
  假设 一次非零退出的调用输出含 `error[E0425]: cannot find value`，另一次以退出码 255 结束且 stdout 与 stderr 都为空
  当 计算两个场景的判定与证据
  那么 两者 verdict 都等于 `uncertain`，failure_class 分别等于 `compile-error` 与 `runner-exited`
  并且 第二个证据的 exit_code 等于 255

场景: runner 退出不触发重试
  测试: test_runner_exit_is_not_retried
  假设 一个假 runner 对每次调用都返回退出码 255 与空输出
  当 TestVerifier 验证一个绑定测试的场景
  那么 假 runner 的调用次数等于 1，verdict 等于 `uncertain`

场景: guard 打印非通过场景的细节
  测试: test_guard_detail_lines_for_non_passing_scenarios
  假设 一份验证报告含一个 `fail` 场景（exit code 101，输出 45 行，最后一行为 panic 消息）与一个 `pass` 场景
  当 渲染 guard 的细节行
  那么 细节包含 fail 场景名、selector、`exit code 101` 与 panic 消息，且只含输出的最后 40 行
  并且 细节不包含 pass 场景名

场景: guard 打印 uncertain 类别
  测试: test_guard_detail_lines_name_uncertain_class
  假设 一份验证报告含一个 failure_class 为 `compile-error` 的 `uncertain` 场景
  当 渲染 guard 的细节行
  那么 细节包含该场景名与 `compile-error`
