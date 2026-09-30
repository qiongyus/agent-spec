spec: task
name: "Guard Per-Contract Boundaries"
tags: [guard, boundaries, verify]
satisfies: [REQ-GUARD-CONTRACT-BOUNDARIES]
risk: B
---

## Intent

guard 不再用整个提交的改动集去检查每个 active 合约的边界：一个合约的
`Forbidden` 与 `Allowed changes` 只约束触及其领地的提交，另一个合约领地内的
路径不算越界，不属于任何合约的路径在触及领地的提交里越界、在其余提交里不检查。
`--strict-boundaries` 保留原行为（LEP-007 Gap 6，ADR-006 D6）。

## Decisions

- `boundaries.rs` 新增纯函数 `plan_contract_boundaries(contracts, changes) -> Vec<ContractBoundaryPlan { touched, exempt }>`；输入为每个 active 合约的 allow-list、合约文件的相对路径，以及归一化后的相对改动路径。
- 领地：至少一条改动匹配该合约 allow-list，或等于合约文件路径，或等于 `specs/roadmap/<合约文件名>`（移动源）。
- `exempt` = 该合约的合约文件与移动源 ∪ 匹配其他 active 合约 allow-list 且不匹配本合约 allow-list 的改动；豁免路径只跳过 Allowed 检查，仍受 Forbidden 检查。
- `BoundariesVerifier` 改为携带 `exempt: Vec<String>` 的结构体（`BoundariesVerifier::default()` 为空豁免），`SpecGateway::with_boundary_exemptions` 传入。
- `cmd_guard`：未触及领地的合约以空改动集验证（不产生边界诊断）；触及领地的合约以完整改动集加豁免验证；`--strict-boundaries` 时所有合约以完整改动集、无豁免验证。

## Boundaries

### Allowed Changes
- src/spec_verify/boundaries.rs
- src/spec_verify/mod.rs
- src/spec_gateway/**
- src/main.rs
- CHANGELOG.md
- specs/task-guard-contract-boundaries.spec.md
- specs/roadmap/task-guard-contract-boundaries.spec.md

### Forbidden
- 不依据提交者身份区分提交
- 不改变 `lifecycle` 与 `verify` 的单合约边界检查

## Out of Scope

- 按提交范围（多个提交）聚合的边界检查
- 对 `specs/roadmap/` 下的 staged 合约做边界检查

## Completion Criteria

场景: 未触及领地的合约不检查
  测试: test_guard_boundaries_skip_contract_whose_territory_is_untouched
  假设 合约 A 允许 `crates/ingest/**`、禁止 `knowledge/**`，合约 B 允许 `crates/query/**`，改动只有 `crates/query/src/lib.rs`
  当 规划并执行 guard 边界检查
  那么 合约 A 的边界诊断为空，合约 B 的边界检查通过

场景: 实现提交不能改 Forbidden 路径
  测试: test_guard_boundaries_forbidden_applies_to_touching_commit
  假设 同上两个合约，改动为 `crates/ingest/src/lib.rs` 与 `knowledge/x.md`
  当 规划并执行 guard 边界检查
  那么 合约 A 报 `knowledge/x.md` 匹配 forbidden 边界

场景: 跨合约提交各自在边界内
  测试: test_guard_boundaries_other_contract_paths_are_not_out_of_bounds
  假设 同上两个合约，改动为 `crates/ingest/src/lib.rs` 与 `crates/query/src/lib.rs`
  当 规划并执行 guard 边界检查
  那么 两个合约的边界检查都通过

场景: 合约晋升提交不被自身拦下
  测试: test_guard_boundaries_contract_file_and_move_source_are_in_bounds
  假设 合约 C 允许 `crates/server/**`，改动为 `specs/task-c.spec.md` 与已删除的 `specs/roadmap/task-c.spec.md`
  当 规划并执行 guard 边界检查
  那么 合约 C 的领地被触及，且其边界检查通过

场景: 触及领地时无主路径越界
  测试: test_guard_boundaries_unowned_path_fails_every_touched_contract
  假设 同上合约 A 与 B，改动为 `crates/ingest/src/lib.rs`、`crates/query/src/lib.rs` 与 `docs/guide.md`
  当 规划并执行 guard 边界检查
  那么 合约 A 与合约 B 都报 `docs/guide.md` not covered by any allowed boundary

场景: 纯文档与纯治理提交不做边界检查
  测试: test_guard_boundaries_commit_touching_no_territory_passes
  假设 同上合约 A 与 B，改动一次为 `docs/guide.md` 与 `.gitlab-ci.yml`，另一次为 `knowledge/requirements/req-x.md`
  当 规划并执行 guard 边界检查
  那么 两次都没有任何边界诊断

场景: 严格模式保留原行为
  测试: test_guard_boundaries_strict_mode_checks_every_contract
  假设 同上合约 A 与 B，改动只有 `docs/guide.md`
  当 以 strict 规划并执行 guard 边界检查
  那么 合约 A 与合约 B 都报 `docs/guide.md` not covered by any allowed boundary
