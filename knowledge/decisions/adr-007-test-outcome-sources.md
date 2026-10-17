---
kind: decision
id: ADR-007
title: "Test Outcome Sources"
status: accepted
tags: [verify, guard, ci]
---

# Test Outcome Sources

## Context

LEP-008（accepted 2026-10-17）记录了两个缺口。ADR-006 D7 之后，guard 按
(package, features, soak) 分组、每组调用一次 `cargo test`，但各组依次执行，
组内 cargo 也逐个运行测试二进制；`--test-threads` 只决定一个二进制内的并行度。
guard 只能用 `cargo test` 执行测试（缺口 1），也不能采用同一流水线里已经跑过
的测试结果（缺口 2）。

下游工作区 amberlog（2026-10-16，43 个合约、931 个 selector，几乎都绑定
`amberlog-server` 的系统测试）的 CI 因此把同一批测试跑两遍：`test` 作业用
cargo-nextest 跨二进制并行跑完全部 1337 个测试并写出 JUnit 报告，用时 47 分钟；
`knowledge` 作业的 guard 再编译两遍、依次重跑绑定测试，用时从 92 分钟（流水线
#460）涨到 114 分钟（#462），并随合约增加继续增长。

## Decision

采纳 LEP-008 的 D1–D4，并按 2026-10-17 的裁决定稿：

- **D1 `--runner cargo|nextest`：** verify、lifecycle、guard 新增该参数，默认
  `cargo`，默认行为与 1.5.0 完全相同。取 `nextest` 时每组调用一次
  `cargo nextest run [-p <package>] [--features <F>] --no-fail-fast
  --test-threads <N> [--run-ignored all] --tool-config-file agent-spec:<临时文件>
  --profile agent-spec -E 'test(~<filter1>) | …'`；`--test-threads` 未指定时为 1
  （与 D7 的默认一致）；`--run-ignored all` 只用于 soak 组。工具配置文件只定义
  profile `agent-spec` 的 JUnit 输出路径（临时目录），自定义 profile 继承项目的
  `default` profile，项目自己的测试组与 overrides 照常生效。各组结果从该 JUnit
  报告读取（D4），不解析人读输出。未安装 `cargo nextest` 时，本次运行的全部场景
  判 `uncertain`，原因 `runner-unavailable`。nextest 不跑文档测试，只匹配到文档
  测试的 selector 判 `skip`（`zero-matched`）。`--runner nextest` 与 `--no-batch`
  同时出现时按参数错误退出。
- **D2 `--test-results <junit.xml>`（可重复）：** 给出时不建组、不调用 cargo，
  只读取报告并逐场景判定。nextest 以二进制 id 命名测试套件：库单元测试为
  `<package>`，其他目标为 `<package>::<target>`；声明 `Package: P` 的 selector
  只匹配二进制 id 等于 `P` 或以 `P::` 开头的测试用例，未声明 package 的匹配全部
  用例。用例名包含 filter 即匹配（libtest 的子串规则）。含 `<failure>` 或
  `<error>` 的用例为失败，含 `<skipped>` 的用例为未执行，其余为通过；判定沿用
  ADR-006 D2、D7 的规则：无匹配判 `skip`（`zero-matched`），无执行判 `skip`
  （`all-ignored`），有失败判 `fail`，其余判 `pass`（`Review: human` 时为
  `pending_review`）。nextest 报告不列被 ignore 的测试，只匹配到它们的 selector
  因此判 `zero-matched`，仍不是通过。多份报告含同一二进制 id 与用例名时，任一
  失败即失败。soak 场景除非 `--include-soak` 否则仍为 `deferred`。报告不存在或
  无法解析时按错误退出并指出文件，不产生任何判定。`--test-results` 与
  `--runner`、`--no-batch` 同时出现时按参数错误退出。
- **D3 `--test-results-features <列表>`：** 报告不记录运行时开启的 cargo
  features，由调用方声明，写法为 `<package>/<feature>`（只写 `<feature>` 时对
  任意 package 生效），逗号分隔，只能与 `--test-results` 同用。声明了
  `Features: F` 的场景，仅当 F 中每个 feature 都已就其 package 声明时才按报告
  判定，否则判 `uncertain`，原因 `results-feature-mismatch`，guard 打印缺少的
  features。未声明 features 的场景按报告判定，不论记录的运行多开了哪些
  features——这比现场运行的条件宽：cargo features 是累加的，开着额外 feature
  通过的测试被当作不开也通过。guard 的汇总行写明判定来源：报告份数、用例数与
  声明的 features。
- **D4 一种结果记录、一个判定函数：** cargo runner 的 libtest 逐测试行、nextest
  runner 的 JUnit 报告与外部提供的报告，都转换成同一种逐测试记录（已知时含二进制
  id，及测试路径、结果、用时），由同一个判定函数逐场景判定；场景证据增加可选的
  `source` 字段（`cargo`、`nextest` 或报告路径）。

D1–D4 在同一个版本（1.6.0）发布；版本内先实现 D4 与 D2、D3，再实现 D1。

## Consequences

Good, because 已在流水线里跑过全部测试的项目不必再跑第二遍：amberlog 的
`knowledge` 作业预计从约 114 分钟降到几分钟，整条流水线从串行约 170 分钟降到
约 60 分钟。

Good, because 仍需现场运行 guard 的场合（本地、soak）可以用 nextest 的跨二进制
并行，合约不用改。

Good, because 三种结果来源共用一个判定函数，判定规则不会分叉。

Bad, because guard 多了一种模式，其判定只与交给它的报告一样可靠；报告的来源
（是否同一提交、同一流水线）由调用方负责，agent-spec 不校验。

Bad, because D3 对未声明 features 的场景接受多开了 features 的运行，条件弱于
现场运行；只在额外 feature 下行为不同的测试可能在报告里通过、现场运行失败。
缓解：features 集合显式写在命令行与汇总行里，不能接受的项目继续现场运行 guard。

Bad, because nextest runner 不跑文档测试，绑定文档测试的 selector 在该 runner
下不通过；默认 runner 因此仍是 `cargo`。

## Alternatives Considered

- 只调大 `--test-threads` —— 被否：二进制仍依次执行，慢的系统测试占主导；下游
  本机实测 17 个合约时 16 线程 7 分 50 秒、4 线程 9 分钟。下游把它作为过渡措施。
- 按合约把 guard 拆到多个 CI 作业 —— 被否：每个分片都要重新编译，还和重复运行
  争同一批执行器槽位。
- 用 `cargo` 包装脚本把 guard 的调用改写成 nextest 并伪造 libtest 输出 —— 被否：
  判定会依赖 agent-spec 之外的文本模仿。
- D3 取严格规则（未声明 features 的场景要求运行未开额外 feature）—— 被否：
  下游 CI 的测试运行对整个工作区开启测试专用 feature，严格规则会让 D2 在那里
  无法使用；2026-10-17 下游裁决接受放宽。

## Source Trace

- proposal: LEP-008
  (knowledge/proposals/2026-10-17-test-runners-and-recorded-results.md,
  accepted 2026-10-17)
- 裁决来源: 2026-10-17 qiongyus：批准 LEP-008；参数名照提案；D1–D4 一起发布；
  D3 接受放宽（下游选项 B）
- 实测证据: amberlog 流水线 #460、#462（2026-10-16）的 `test` 与 `knowledge`
  作业用时；nextest 0.9.146 JUnit 报告结构（测试套件 = 二进制 id，用例 = 测试
  路径，失败带 `<failure>`，被 ignore 的测试不列出），2026-10-17 以探针 crate
  实测
- 代码位置: `src/spec_verify/test_batch.rs` 的 `group_args`、`execute_plan`、
  `attribute`；`src/spec_verify/test_verifier.rs` 的 `CargoRunner` 与
  `TestCommandRunner`
- 先例: cargo-nextest 的过滤表达式、JUnit 报告与 `--tool-config-file`；CI 系统
  以 JUnit 作为跑测试的作业与读结果的作业之间的交换格式
- governed requirements: REQ-NEXTEST-RUNNER（D1）、REQ-RECORDED-TEST-RESULTS（D2、D4）、REQ-RECORDED-RESULTS-FEATURES（D3）
