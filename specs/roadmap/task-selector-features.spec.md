spec: task
name: "Test Selector Features"
tags: [verify, authoring, cargo]
satisfies: [REQ-SELECTOR-FEATURES]
risk: B
---

## Intent

让测试 selector 能声明其测试所需的 cargo feature：`Features:` / `特性:` 的值
原样透传为 `cargo test --features`，使依赖测试专用 feature 的合约测试在 guard
下被编译并执行，而不是因 `running 0 tests` 被判 `skip`（LEP-007 Gap 3，ADR-006 D3）。

## Decisions

- `TestSelector` 新增 `features: Option<String>`（`skip_serializing_if = "Option::is_none"`），`TestSelector::filter_only` 与所有构造点置 `None`。
- `keywords.rs` 新增 `TestSelectorField::Features`：识别 `Features:`（大小写不敏感）、`特性:` 与 `特性：`。
- `build_cargo_test_args`：features 非空时在 `-p` 之后、filter 之前追加 `--features` 与原值；值为空（trim 后）时不追加。
- `Evidence::TestOutput` 新增 `features: Option<String>`（`skip_serializing_if = "Option::is_none"`），记录声明的值。
- 新增 spec linter `EmptySelectorFeaturesLinter`，规则名 `selector-features-empty`，级别 Warning，对 `Features:` 值为空的场景各报一条。

## Boundaries

### Allowed Changes
- src/spec_core/ast.rs
- src/spec_core/verify.rs
- src/spec_parser/keywords.rs
- src/spec_parser/parser.rs
- src/spec_verify/test_verifier.rs
- src/spec_lint/linters.rs
- src/spec_lint/pipeline.rs
- src/spec_report/**
- src/spec_gateway/**
- src/main.rs
- CHANGELOG.md
- specs/task-selector-features.spec.md
- specs/roadmap/task-selector-features.spec.md

### Forbidden
- 不改变未声明 features 的 selector 生成的 cargo 参数
- 不新增 crate 依赖

## Out of Scope

- 非 cargo 测试运行器
- `--all-features` / `--no-default-features` 等其他 cargo feature 开关

## Completion Criteria

场景: 解析英文与中文 features 字段
  测试: test_parse_selector_features_field_in_both_languages
  假设 一个场景的 selector 含 `Features: fault-injection,slow-io`，另一个含 `特性：fault-injection`
  当 解析该 spec
  那么 两个 selector 的 features 分别等于 `fault-injection,slow-io` 与 `fault-injection`

场景: features 透传给 cargo
  测试: test_build_cargo_test_args_appends_features
  假设 一个 selector 含 `Package: amberlog`、`Filter: crash_recovery` 与 `Features: fault-injection`
  当 生成 cargo test 参数
  那么 参数依次为 `test`、`-q`、`-p`、`amberlog`、`--features`、`fault-injection`、`crash_recovery`

场景: 未声明 features 时参数不变
  测试: test_build_cargo_test_args_without_features_is_unchanged
  假设 一个 selector 只含 `Package: amberlog` 与 `Filter: crash_recovery`
  当 生成 cargo test 参数
  那么 参数依次为 `test`、`-q`、`-p`、`amberlog`、`crash_recovery`

场景: 空 features 被拒绝：不传参数并报 lint
  测试: test_empty_selector_features_warns_and_passes_no_flag
  假设 一个场景的 selector 含 `Filter: crash_recovery` 与值为空的 `Features:`
  当 生成 cargo test 参数并对该 spec 运行 lint
  那么 参数不包含 `--features`
  并且 lint 输出包含一条 `selector-features-empty` Warning

场景: 证据记录 features
  测试: test_test_output_evidence_records_selector_features
  假设 一个 selector 声明 `Features: probe` 的场景
  当 构造该场景的测试证据并序列化为 JSON
  那么 JSON 的 features 字段等于 `probe`
  并且 未声明 features 的证据 JSON 不包含 features 字段
