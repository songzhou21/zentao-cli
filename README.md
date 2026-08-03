# zentao-cli

用于查询禅道 Bug、查看详情、管理认证 Cookie 和下载图片的 macOS 命令行工具。

## 安装

```bash
cargo install --path . --force
zentao --version
```

## 配置

先配置禅道站点和 Bug 所属产品：

```bash
zentao config set site http://shendao.sharexm.cn/zentao
zentao config set product 92
```

命令行参数优先级：

```text
--site > ZENTAO_SITE > config.json
--product > ZENTAO_PRODUCT > config.json
--config > ZENTAO_CONFIG > ~/.zentao/config.json
```

## Bug

```bash
# 默认只列出激活 Bug，最多 30 条
zentao bug list

# 真实禅道筛选条件
zentao bug list --title 会议 -a zhousong -s active -L 100
zentao bug list --resolved-by zhousong \
  --resolved-from 2026-07-01 --resolved-to 2026-07-31

# 表格默认截断标题；需要完整标题时加 --full-title（不影响 --title 搜索条件）
zentao bug list --title 会议优化5.1期 -s all --full-title

# JSON 输出
zentao bug view 57801 --json
zentao bug list --json=id,title,state,assignee

# 查看详情：可传 Bug ID 或完整 URL
zentao bug view 57801
zentao bug view http://shendao.sharexm.cn/zentao/bug-view-57801.html
zentao bug view 57801 -o ./bug-57801.md
zentao bug view 57801 --json=id,title,description,images,attachments
```

`--json` 单独使用时输出所有字段；指定字段时必须使用等号形式，例如 `--json=id,title`，以免把位置参数误当字段。`bug list` 支持：`--title`（可重复，最多 3 个，按 OR）、`-a/--assignee`、`--resolved-by`、`--resolved-from`、`--resolved-to`、`--module`、`-s/--state`、`-L/--limit` 和 `--full-title`。`--state` 取值为 `active`、`resolved`、`closed`、`all`。表格默认按显示宽度截断 TITLE；`--full-title` 仅影响人类可读表格，展示完整标题，不影响搜索条件；JSON 路径本身已是完整字段，可与 `--full-title` 并用（静默忽略）。

在终端（TTY，如 Kitty）中，`bug list` 表格的 TITLE 默认可点击：使用 OSC 8 超链接，目标地址与 JSON `url` 相同（`<site>/bug-view-<id>.html`）。标题外观不变（无额外下划线或变色）；管道/重定向时不注入控制序列。

`bug view` 的 JSON 支持 `images` 字段，返回描述和历史记录中的图片 URL 数组。

列表和详情 JSON 的 `url` 都是稳定的详情页地址：`<site>/bug-view-<id>.html`，不包含重定向产生的临时查询参数。

列表 JSON 的 `openedDate`、`resolvedDate` 和 `deadline` 保留禅道列表原始文本；部分实例会省略年份，例如 `07-31 10:00`，不要将其当作完整 ISO 日期。

## 认证

默认从 Chrome 读取 Cookie。账号密码登录写入本地 Cookie 文件，并自动把 Cookie 来源切换为 `file`：

```bash
zentao auth status
printf '%s' "$ZENTAO_PASSWORD" | zentao auth login --username <username> --password-stdin
zentao auth select-chrome-profile
zentao auth status --show-cookie-values
```

`auth status` 默认隐藏 Cookie 值；仅在需要本地诊断时使用 `--show-cookie-values`。

登录时显式传入 `--site` 会把该地址保存到配置中，成为后续命令的默认 Site；一次性切换站点时请留意这一持久化行为。

## 配置管理

```bash
zentao config list
zentao config get cookie-source
zentao config set cookie-source chrome
zentao config set chrome-profile "/Users/you/Library/Application Support/Google/Chrome/Profile 1"
```

## 图片

```bash
zentao image download --url http://shendao.sharexm.cn/zentao/file-read-59561.png
zentao image download --url http://shendao.sharexm.cn/zentao/file-read-59561.png -o /tmp/bug57801
```

图片下载会按当前认证配置携带 ZenTao Cookie，并且仅在最终响应不是登录页、`Content-Type` 为 `image/*` 时写入文件。

## 开发

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
