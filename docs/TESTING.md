# TESTING

本文说明 `zentao-cli`（Rust 版）的测试策略和执行方式。

## 目标

- 优先保证回归稳定性
- 以单元测试为主，聚焦核心规则
- `bug view` 使用固化 HTML fixture 做解析回归
- `bug stats` 使用固化浏览 JSON fixture（`bug-browse-…-bySearch-myQueryID.json`）做解析与聚合回归
- 避免依赖真实禅道、真实 Chrome 环境

## 测试分层

### 1. Unit Test（默认必须跑）

覆盖模块：

- `src/config.rs`
- `src/api.rs`
- `src/bug.rs`
- `src/cli.rs`
- `src/browser.rs`（仅纯函数和稳定逻辑）

重点：

- 参数与分支逻辑
- `bug list/stats/view`、认证、配置和 JSON 字段契约
- 错误路径与边界条件
- 核心业务规则断言

### 2. Fixture 回归

`bug` 模块复用：

- `tests/fixtures/bug/bug_48919_real.html`
- `tests/fixtures/bug/bug_missing_title.html`
- `tests/fixtures/bug/bug_missing_desc.html`
- `tests/fixtures/search/browse_bysearch_myqueryid.json`（`bug-browse-…-bySearch-myQueryID.json` 实抓）

更新浏览 JSON fixture（需本机 Cookie 可用）：

```bash
ZENTAO_DEBUG_JSON=tests/fixtures/search/browse_bysearch_myqueryid.json \
  zentao bug stats --title 会议优化5.1
```

然后按新样本更新 `parses_browse_json_fixture` / `aggregate_browse_json_fixture` 中的条数断言。

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
