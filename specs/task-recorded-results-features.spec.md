spec: task
name: "Recorded Results Features"
tags: [verify, guard, ci, cargo]
satisfies: [REQ-RECORDED-RESULTS-FEATURES]
risk: B
---

## Intent

报告不记录运行时开启的 cargo features，由调用方以 `--test-results-features`
声明：声明了 `Features:` 的场景只有在其 features 都被覆盖时才按报告判定，否则
`uncertain`；未声明 features 的场景接受多开了 features 的运行
（LEP-008 D3，ADR-007 D3，下游选项 B）。

## Decisions

- CLI：`--test-results-features` 以 `value_delimiter = ','` 读入；`TestSourceArgs::options` 在未给 `--test-results` 时报错。
- `RecordedResults` 保存声明项 `DeclaredFeature { package: Option<String>, feature }`：含 `/` 的项拆成 package 与 feature，不含的对任意 package 生效；空项忽略、两侧空白去掉。
- `missing_features(selector)`：selector 的每个 feature 须有声明项 `P/f`（P 等于 selector 的 package）或 `f`；未声明 package 的 selector 只认 `f`。
- 有缺少的 feature 时，场景判 `uncertain`，新增判定类别 `JudgmentKind::FeatureMismatch`，`failure_class` 为 `results-feature-mismatch`，证据输出写明缺少的 features 与报告汇总；guard 现有的非通过明细因此列出缺少的 features。
- `RecordedResults::summary` 在有声明项时追加 `; features: …`，guard 汇总行随之列出。

## Boundaries

### Allowed Changes
- src/spec_verify/**
- src/main.rs
- CHANGELOG.md
- specs/task-recorded-results-features.spec.md

### Forbidden
- 不让声明了 features 而未被覆盖的场景判 `pass` 或 `fail`
- 不改变未给 `--test-results` 时 `Features:` 的含义

## Out of Scope

- 从报告或构建产物推断运行时的 features

## Completion Criteria

场景: 解析带 package 与不带 package 的声明项
  测试: test_declared_features_parse_package_and_bare_entries
  假设 两个场景分别声明 `alpha` 的 `fi` 与 `beta` 的 `slow`，声明项为 ` alpha/fi`、`beta/slow ` 与空项
  当 按报告验证
  那么 两个场景都判 `pass`

场景: 只给 features 不给报告
  测试: test_test_results_features_require_test_results
  假设 只给 `--test-results-features alpha/fi`
  当 生成测试选项
  那么 返回提到 `--test-results` 的错误

场景: 声明项覆盖场景 feature
  测试: test_declared_features_cover_scenario_features
  假设 场景声明 `Package: alpha`、`Features: fi`
  当 分别以 `alpha/fi` 与 `fi` 声明并按通过与失败的报告验证
  那么 通过的报告判 `pass`，失败的报告判 `fail`

场景: 未覆盖的 feature 判 uncertain
  测试: test_undeclared_scenario_feature_is_uncertain
  假设 场景要 `fi`（或 `fi,slow`），声明项分别为空、`beta/fi` 与 `alpha/fi`
  当 按报告验证
  那么 三次都判 `uncertain`，failure_class 等于 `results-feature-mismatch`

场景: 列出缺少的 feature
  测试: test_feature_mismatch_lists_missing_features
  假设 场景要 `fi,slow`，只声明 `alpha/fi`
  当 按报告验证
  那么 证据输出包含 `slow`

场景: 无 package 场景只认不带 package 的声明项
  测试: test_scenario_without_package_needs_bare_feature
  假设 场景未声明 package、要 `fi`
  当 分别以 `alpha/fi` 与 `fi` 声明验证
  那么 前者判 `uncertain`，后者判 `pass`

场景: 未声明 features 的场景接受任意记录
  测试: test_scenario_without_features_accepts_any_recorded_run
  假设 场景未声明 features，报告中其用例通过
  当 分别以 `alpha/fi` 声明与不给声明项验证
  那么 两次都判 `pass`

场景: 汇总列出声明项
  测试: test_recorded_results_summary_lists_declared_features
  假设 声明项为 `alpha/fi` 与 `beta/slow`
  当 生成汇总
  那么 汇总包含 `alpha/fi` 与 `beta/slow`
