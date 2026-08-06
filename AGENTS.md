# zentao-cli (Rust)

macOS 禅道 CLI。Chrome Cookie 读取依赖 macOS Keychain；本项目的支持边界仅为 macOS。

## 命令约定

- Bug 资源入口：`zentao bug list`、`zentao bug stats`、`zentao bug view <ID|URL>`。
- 不保留 `zentao search` 或 `zentao bug show` 兼容入口。
- 全局配置：`--site`、`--config`。
- `bug list` / `bug stats` 的 Product 必须来自 `--product`、`ZENTAO_PRODUCT` 或配置，禁止硬编码产品 ID。
- `bug list` 默认状态为 `active`，限制参数为 `-L, --limit`，默认 30。
- `bug stats` 按当前 assignee 聚合状态剖面；默认 `--state all`、`-L 1000`；样本制（与 limit 相同上限，不保证全集）；触顶时 stderr 警告。
- `bug stats`：仅 active/resolved 按人聚合；`closed` 进 `(已关闭)`；排序激活↓再待验证↓；表头为指派给/激活/待验证/关闭/合计；无 `--full-title`，无 `--group-by`。
- `list`/`stats` 解决日期快捷：`--week`（周一～周日）、`--month`（本月）、`--day`（今天）；映射为 `resolvedDate` 区间，与 `--resolved-from/to` 互斥。
- JSON 使用 `--json[=fields]`；裸 `--json` 输出全部字段，指定字段必须使用 `--json=id,title`。
- 列表人类表格表头为中文：编号 / 状态 / 创建者 / 创建日期（`openedDate`，非指派日期） / 标题 / 指派给；默认截断标题（显示宽度 65）；`--full-title` 仅展开表格标题为完整单行，不改变 `--title` 搜索条件，不放开指派给 / 创建者截断，与 `--json` 可并用（JSON 路径静默忽略，字段名仍为英文）。
- 列表表格在 stdout 为 TTY 时，TITLE 默认包 OSC 8 超链接（目标与 JSON `url` 相同的 `bug-view-<id>.html`）；可见文本不变、不加下划线/变色；管道/重定向不输出 OSC；不跟 `NO_COLOR` 绑死。
- `--plain` 关闭表格交互装饰（OSC 8 超链接、表头/状态颜色），输出纯文本；可与 `--full-title` 并用；`--json` 路径静默忽略。
- 列表 JSON 日期保留禅道原始展示文本，可能省略年份；不要推断或伪造完整日期。
- 仅暴露经验证的禅道字段；不提供模拟 GitHub 查询语法的 `--search`。

## 认证与配置

- 默认 Cookie 来源为 Chrome。
- `image download` 使用当前 Cookie，并仅写入最终响应为 `image/*` 的文件。
- `zentao auth login` 只通过 `--password-stdin` 接收密码，禁止明文密码参数。
- 登录写入 `~/.zentao/cookies` 后，自动把 `cookie-source` 切换为 `file`；显式 `--site` 也会保存为后续命令默认 Site。
- `auth status` 默认隐藏 Cookie 值；只有 `--show-cookie-values` 才显示。
- `zentao config list|get|set` 只管理：`site`、`product`、`cookie-source`、`chrome-profile`。

## 搜索限制

- 禅道查询仍使用两组、各三个条件槽位。
- 重复 `--title` 最多三个值，按 OR；可与最多三个非标题条件组合。
- 重复 `--opened-by` 最多三个值（用户账号，非中文显示名），按 OR；可与最多三个非创建者条件组合。
- 同时重复 `--title` 与 `--opened-by` 时两组槽位都被 OR 占满，不能再叠加其他筛选（含默认 `active` 状态，需 `--state all`）。
- 默认 `--state active` 也占用一个条件槽位；不需状态筛选时使用 `--state all` 释放该槽位。
- 不要重新加入标题前缀“模块分组”：它不是禅道模块字段。

## 测试

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

HTTP 流程单测会绑定临时 localhost 端口。
