# zentao-cli (Rust)

macOS 禅道 CLI。Chrome Cookie 读取依赖 macOS Keychain；本项目的支持边界仅为 macOS。

## 命令约定

- Bug 资源入口：`zentao bug list`、`zentao bug view <ID|URL>`。
- 不保留 `zentao search` 或 `zentao bug show` 兼容入口。
- 全局配置：`--site`、`--config`。
- `bug list` 的 Product 必须来自 `--product`、`ZENTAO_PRODUCT` 或配置，禁止硬编码产品 ID。
- 默认状态为 `active`，限制参数为 `-L, --limit`，默认 30。
- JSON 使用 `--json [fields]`；省略字段时输出全部字段。
- 仅暴露经验证的禅道字段；不提供模拟 GitHub 查询语法的 `--search`。

## 认证与配置

- 默认 Cookie 来源为 Chrome。
- `zentao auth login` 只通过 `--password-stdin` 接收密码，禁止明文密码参数。
- 登录写入 `~/.zentao/cookies` 后，自动把 `cookie-source` 切换为 `file`。
- `auth status` 默认隐藏 Cookie 值；只有 `--show-cookie-values` 才显示。
- `zentao config list|get|set` 只管理：`site`、`product`、`cookie-source`、`chrome-profile`。

## 搜索限制

- 禅道查询仍使用两组、各三个条件槽位。
- 重复 `--title` 最多三个值，按 OR；可与最多三个非标题条件组合。
- 不要重新加入标题前缀“模块分组”：它不是禅道模块字段。

## 测试

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

HTTP 流程单测会绑定临时 localhost 端口。
