# zentao-cli (Rust)

macOS 禅道 CLI。Chrome Cookie 读取依赖 macOS Keychain；本项目的支持边界仅为 macOS。项目约定与用法以本文为准。

## 命令约定

- Bug 资源入口：`zentao bug list`、`zentao bug stats`、`zentao bug view <ID|URL>`、`zentao bug candidates --build`。
- `bug candidates --build`（版本候选）/ `--module`（模块候选）默认人类表格（编号/版本 或 编号/模块）；`--json[=value,name]` 输出 JSON（`value`,`name`）。`value` 用于 `--opened-build` / `--resolved-build` / `--module`。可跟关键词做名称包含过滤。
- `bug view` 拉 `bug-view-<id>.json`（`{status,data,md5}`），只输出 JSON；默认完整对象，`--json=fields` 裁剪。不提供 Markdown 文档，无 `-o`。
- 详情 JSON 字段：`id,title,priority,state,openedBy,openedDate,assignee,resolvedBy,resolvedDate,resolvedBuild,description,history,images,attachments,url`。`state` 为三态，不要 `resolution`。人员用显示名；`resolvedBuild` 用 `builds` 展示名；日期用详情接口完整时间。
- `description` 与历史 `comment` 是去样式 HTML（留 `p` `br` `ol` `ul` `li` `img` `a`）。`images` 从描述/备注 HTML 的 `<img src>` 收集绝对地址。附件用 `files.title` + `webPath`。`history` 是事件数组（`at`/`action`/`actor`，按需 `assignee`/`changes`/`comment`）；`changes` 为 `field`+`label`+`old`+`new`。
- `bug view --raw-json` 输出接口原始 JSON：把 `data` 从转义字符串展开成对象后再格式化。与 `--json` 互斥。
- 不保留 `zentao search`、`zentao bug show` 或 `zentao image` 兼容入口。
- 全局配置：`--site`、`--config`。
- `bug list` / `bug stats` 的 Product 必须来自 `--product`、`ZENTAO_PRODUCT` 或配置，禁止硬编码产品 ID。
- `bug list` 默认状态为 `active`，限制参数为 `-L, --limit`，默认 30。要查已解决/关闭需显式 `-s`。
- `bug stats` 默认 `--state all`、`-L 1000`；样本制（与 limit 相同上限，不保证全集）；触顶时 stderr 警告。无 `--by`，无 `--full-title`，无通用 `--group-by`。
- `bug list` / `bug stats` 都先 POST 与 list 相同的关键词搜索（`search-buildQuery.html`），再读浏览 JSON；不拉、不解析列表 HTML。
- `bug stats` 两张表。主表：人员 / 激活 / 已解决 / 关闭 / 合计。激活按当前指派给；已解决 / 关闭按解决者；合计 = 激活+已解决+关闭（列加总）。写出量 = 已解决+关闭。无解决者的关闭单进 `(未解决)`。排序合计↓再激活↓再已解决↓。待验证单独一张表（当前指派且状态 `resolved`），空则不输出。JSON：`rows` 为 `assignee,active,solved,closed,total`；`pending.rows` 为 `assignee,resolved`（`resolved`=待验证）。
- `list`/`stats` 解决日期快捷：`--week`（周一～周日）、`--month`（本月）、`--day`（今天）；映射为 `resolvedDate` 区间，与 `--resolved-from/to` 互斥。
- `list`/`stats` 版本筛选：`--opened-build`（影响版本）、`--resolved-build`（解决版本）；数字当作版本 ID 原样提交；非数字按候选名称唯一包含匹配后转 ID 再搜。多个/零个匹配时报错，并指向 `zentao bug candidates --build`。对应禅道 `openedBuild` / `resolvedBuild`，操作符 `=`。
- JSON 使用 `--json[=fields]`；裸 `--json` 输出全部字段，指定字段必须使用 `--json=id,title`。
- 列表 JSON 日期为浏览接口完整时间（如 `2026-08-20 11:30:31`）；`resolution` 为禅道代码（如 `fixed`）；`confirmed` 为布尔（`"1"` → true，其余为 false）；人员用显示名；含 `resolvedBy`（显示名，未解决为 `null`）。
- 列表表头为中文：编号 / 状态 / 创建者 / 创建日期（`openedDate` 完整时间，非指派日期） / 标题 / 指派给；默认截断标题（显示宽度 65）；`--full-title` 仅展开表格标题为完整单行，不改变 `--title` 搜索条件，不放开指派给 / 创建者截断，与 `--json` 可并用（JSON 路径静默忽略）。
- 列表表格在 stdout 为 TTY 时，TITLE 默认包 OSC 8 超链接（目标与 JSON `url` 相同的 `bug-view-<id>.html`）；可见文本不变、不加下划线/变色；管道/重定向不输出 OSC；不跟 `NO_COLOR` 绑死。
- `--plain` 关闭表格交互装饰（OSC 8 超链接、表头/状态颜色），输出纯文本；可与 `--full-title` 并用；`--json` 路径静默忽略。
- 仅暴露经验证的禅道字段；不提供模拟 GitHub 查询语法的 `--search`。

## 开发规范

- 凡有结构化输出的命令，先做 `--json` API；人类可读再投影同一份字段，不要另算列或另做一套数据。
- 表头中文，字段名仍英文。无人类表格的命令（如 `bug view`）只出 JSON。

## 技能文档

- `skills/zentao/SKILL.md` 只写用法（命令、参数、字段含义），不写实现细节（请求路径、HTML/JSON 刮取、内部数据源）。

## 认证与配置

- 默认 Cookie 来源为 Chrome。
- `zentao auth login` 只通过 `--password-stdin` 接收密码，禁止明文密码参数。
- 登录写入 `~/.zentao/cookies` 后，自动把 `cookie-source` 切换为 `file`；显式 `--site` 也会保存为后续命令默认 Site。
- `auth status` 默认隐藏 Cookie 值；只有 `--show-cookie-values` 才显示。
- `zentao config list|get|set` 只管理：`site`、`product`、`cookie-source`、`chrome-profile`。
- 候选缓存：`~/.zentao/cache/<product>.json`（`fetchedAt` + `kinds.build` / `kinds.module` 的 `{value,name}` 数组）。名称筛选读缓存，TTL 1 小时；list/stats 的浏览 JSON 会回写。数字 ID 不读缓存。不是 config 项。

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

fixture、断言要点和重抓步骤见 `docs/TESTING.md`。
