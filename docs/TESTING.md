# TESTING

本文说明 `zentao-cli`（Rust 版）的测试策略和执行方式。

## 目标

- 优先保证回归稳定性
- 以单元测试为主，聚焦核心规则
- `bug view` 使用真实 fixture（JSON + 对照 HTML）做解析回归，不访问禅道；Markdown 从完整 JSON 投影，不另算字段
- `bug list` / `bug stats` / `bug report` 只使用固化浏览 JSON fixture（`bug-browse-…-bySearch-myQueryID.json`）做解析、列表 JSON、聚合与报告分组回归，不保留列表 HTML 样本
- 避免依赖真实禅道、真实 Chrome 环境

## 测试分层

### 1. Unit Test（默认必须跑）

覆盖模块：

- `src/config.rs`
- `src/cache.rs`
- `src/api.rs`
- `src/bug.rs`
- `src/cli/`
- `src/browser.rs`（仅纯函数和稳定逻辑）

重点：

- 参数与分支逻辑
- `bug list/stats/report/view`、认证、配置和 JSON 字段契约
- 错误路径与边界条件
- 核心业务规则断言
- HTTP 流程（`src/api_test.rs`、`src/cli/cli_test.rs`）在 `127.0.0.1:0` 起临时 mock，不访问禅道

### 2. Fixture 回归

`bug view` 拉 `bug-view-<id>.json`，不再刮详情 HTML。行为测试使用真实 fixture，不访问禅道。既有 HTML 样本保留，并补对应 JSON：标题、描述图、附件与 JSON 对齐；描述/备注为去样式 HTML；`images` 从 HTML `<img>` 按出现顺序收集；历史为事件数组。登录页 HTML 视为 cookie 失效。

`bug` 模块复用：

- `tests/fixtures/bug/bug_48919_real.html` / `bug_48919.json`：标题、描述图、附件；JSON 历史隐藏指派、保留严重程度/优先级 `changes`
- `tests/fixtures/bug/bug_51267_real.html` / `bug_51267.json`：描述两张图按出现顺序、绝对 `file-read` 地址
- `tests/fixtures/bug/bug_48433_real.html` / `bug_48433.json`：长历史、备注；过滤模块和步骤；备注不含编辑框 HTML
- `tests/fixtures/bug/bug_missing_title.html`
- `tests/fixtures/bug/bug_missing_desc.html`
- `tests/fixtures/bug/bug_58688.json` / `bug_58688_real.html`：多图+多附件。解析后标题与 HTML 一致；描述两张图按出现顺序输出绝对 `file-read` 地址；附件用 `files.title` + `webPath`，文件名为「安卓下拉选择状态.mp4」「ios下拉选择状态.mp4」
- `tests/fixtures/bug/bug_58441.json` / `bug_58441_real.html`：元数据（显示名、上线版本展示名、完整时间）；历史数组含创建、张涛指派给周松、编辑抄送、解决及备注；`changes` 含 `field`+`label`；隐藏指派/解决方案；备注为去样式 HTML，不含编辑框 HTML
- `tests/fixtures/search/browse_bysearch_myqueryid.json`（`bug-browse-…-bySearch-myQueryID.json` 实抓）
- `tests/fixtures/search/browse_assigned_to_zhousong.json`（`--resolved-by zhousong --resolved-from 2026-07-01 --resolved-to 2026-07-31 -L 5` 实抓；关闭后 `assignedTo` 为 Closed）
- `tests/fixtures/search/browse_empty.json`（无匹配标题实抓，`bugs` 为空数组）
- `tests/fixtures/search/browse_assigned_date_desc.json`（`--module 1144 -s active --sort assignedDate -L 5` 实抓；`assignedDate` 倒序，编号非 id 倒序）
- `tests/fixtures/search/browse_resolved_by_zhousong_month.json`（`--resolved-by zhousong --month -s all -L 1000` 实抓；`bug report` 标题前缀分组回归）

更新浏览 JSON fixture（需本机 Cookie 可用）：

```bash
ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_bysearch_myqueryid.json \
  zentao bug stats --title 会议优化5.1
ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_assigned_to_zhousong.json \
  zentao bug list --resolved-by zhousong --resolved-from 2026-07-01 --resolved-to 2026-07-31 -L 5
ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_empty.json \
  zentao bug list --title __zentao_cli_no_match_xyz_9f3a2__ -L 5
ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_assigned_date_desc.json \
  zentao bug list --module 1144 -s active --sort assignedDate -L 5
ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_resolved_by_zhousong_month.json \
  zentao bug list --resolved-by zhousong --month -s all -L 1000
```

然后按新样本更新 `parses_browse_json_fixture` / `parses_assigned_browse_json_fixture` / `parses_empty_browse_json_fixture` / `aggregate_browse_json_fixture` / `groups_zhousong_month_browse_fixture` 中的条数断言。

更新 `bug view` JSON fixture：

```bash
ZENTAO_DEBUG_JSON=tests/fixtures/bug/bug_58688.json zentao bug view 58688
ZENTAO_DEBUG_JSON=tests/fixtures/bug/bug_58441.json zentao --site http://zentao.test.sharexm.cn/zentao bug view 58441
```

## 运行方式

### 默认测试

```bash
cargo test
```

### 仅测试某模块

```bash
cargo test bug::tests
cargo test browser::tests
cargo test cli::tests
```

## 当前限制

- 本地需先安装 Rust toolchain（`rustup` + `cargo`）
- 本地无 Rust 环境时无法执行编译与测试
