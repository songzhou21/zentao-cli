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
zentao bug list --opened-by chenjie --opened-by niuweilong -s active
zentao bug list --resolved-by zhousong \
  --resolved-from 2026-07-01 --resolved-to 2026-07-31

# 表格默认截断标题；需要完整标题时加 --full-title（不影响 --title 搜索条件）
zentao bug list --title 会议优化5.1期 -s all --full-title

# 纯文本表格：无超链接、无颜色（便于复制）
zentao bug list --title 会议优化5.1期 -s all --full-title --plain

# 按指派人统计状态剖面（默认 --state all、-L 1000；样本制）
zentao bug stats --title 会议优化5.1期
zentao bug stats --module 1099 --json=assignee,active,resolved,closed,total

# 解决日期快捷（list/stats 均可；与 --resolved-from/to 互斥）
zentao bug stats --week                 # 本周一～本周日
zentao bug stats --month                # 本月
zentao bug list --day -s all            # 今天

# JSON 输出
zentao bug view 57801 --json
zentao bug list --json=id,title,state,assignee

# 查看详情：可传 Bug ID 或完整 URL
zentao bug view 57801
zentao bug view http://shendao.sharexm.cn/zentao/bug-view-57801.html
zentao bug view 57801 -o ./bug-57801.md
zentao bug view 57801 --json=id,title,description,images,attachments
```

`--json` 单独使用时输出所有字段；指定字段时必须使用等号形式，例如 `--json=id,title`，以免把位置参数误当字段。`bug list` 支持：`--title`（可重复，最多 3 个，按 OR）、`-a/--assignee`、`--opened-by`（可重复，最多 3 个，按 OR；值为禅道用户账号如 `chenjie`，不是中文显示名）、`--resolved-by`、`--resolved-from`、`--resolved-to`、`--week` / `--month` / `--day`（解决日期快捷，与手动 from/to 互斥）、`--module`、`-s/--state`、`-L/--limit`、`--full-title` 和 `--plain`。`--state` 取值为 `active`、`resolved`、`closed`、`all`。表格默认按显示宽度截断 TITLE；`--full-title` 仅影响人类可读表格，展示完整标题，不影响搜索条件；JSON 路径本身已是完整字段，可与 `--full-title` / `--plain` 并用（静默忽略）。

解决日期快捷含义（按本地日历日，含首尾）：`--week` 为本周一～本周日；`--month` 为本月 1 日～月末；`--day` 为今天。三者互斥，且会写入与 `--resolved-from` / `--resolved-to` 相同的禅道 `resolvedDate` 条件。`bug list` 在带有任一解决日期条件且未显式 `-s/--state` 时，状态自动为 `all`（避免默认 `active` 与「已解决日期」互斥导致空列表）；显式指定状态时仍以用户为准。

`bug stats` 复用与 list 相同的筛选参数，但默认 `--state all`、`-L 1000`，且**没有** `--full-title`。它在本次样本上对 **active / resolved** 按当前 assignee 分组；**closed 不按人归类**（列表里关闭单的指派给常为 `Closed`），单独记入 `(已关闭)` 行并计入合计。人员行排序：**激活降序 → 待验证降序 → 名字**。人类表格表头：`指派给` / `激活` / `待验证` / `关闭` / `合计`。空指派为 `(未指派)`；表底有 `合计`。JSON 字段名仍为英文（`assignee`/`active`/`resolved`/`closed`/`total`）。统计是样本制：只聚合不超过 limit 的 Bug，**不保证全集**；触顶时 stderr 警告，JSON `incomplete` 为 `true`。

stats JSON 形态为对象（不是 bug 数组），字段包括 `groupBy`、`sampleSize`、`limit`、`incomplete`、`fetchedAt`、`resolvedFrom`、`resolvedTo`、`rows`、`total`；行字段为 `assignee`、`active`、`resolved`、`closed`、`total`。`--json=assignee,active` 可裁剪 `rows`/`total` 内字段；`total` 不含 `assignee`。人类表格在合计行下方单独输出元信息：有解决日期条件时一行 `解决日期: from ~ to`，再一行 `更新时间: YYYY-MM-DD HH:MM:SS`（本地抓取时刻）。

在终端（TTY，如 Kitty）中，`bug list` 表格的 TITLE 默认可点击：使用 OSC 8 超链接，目标地址与 JSON `url` 相同（`<site>/bug-view-<id>.html`）。标题外观不变（无额外下划线或变色）；管道/重定向时不注入控制序列。需要纯文本（无超链接、无颜色）时加 `--plain`。

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
