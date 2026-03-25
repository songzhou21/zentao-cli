# zentao-cli (Rust)

禅道 CLI 工具，支持两种 Cookie 来源：`file`（默认）和 `chrome`（macOS）。

## 功能

- 默认从 `~/.zentao/cookies` 读取 zentao Cookie（`keepLogin/za/zentaosid/zp`）
- 支持账号密码登录并保存 Cookie 到 `~/.zentao/cookies`
  - 登录落盘仅保留：`keepLogin`、`za`、`zp`、`zentaosid`
  - `login` 仅写 cookie 文件，不更新 `config.json`
- 可切换为从 Chrome（macOS）读取 Cookie（`cookie_source=chrome`）
- 输出 Cookie 明细（含到期时间格式化）
- 可选校验 Cookie 是否有效（根路径重定向规则）
- 支持管理 Chrome Profile 并保存到 `config.json`
- 支持按 Bug 详情 URL 抓取详情并输出 Markdown（正文图片地址自动补全）
- 支持按图片 URL 直接下载到本地（`image download --url`）
- 支持搜索 Bug（按指派者、解决者、解决日期范围筛选，输出文本列表）
  - `search` 默认携带 `pagerBugBrowse=20`，可通过 `--page-size` 覆盖

## 搜索字段（当前支持）

- `assigned_to`（指派给）
- `resolved_by`（解决者）
- `resolved_date_from`（解决日期起始，含）
- `resolved_date_to`（解决日期截止，含）

## 搜索分组限制

- 禅道查询采用两组条件：`group1`（slot1~3）与 `group2`（slot4~6），两组由 `groupAndOr` 连接。
- **每个 group 最多支持 3 个搜索条件**（对应 3 个 slot）。
- `--title` 支持重复传入，重复值按 OR 处理（最多 3 个标题）。
- 标题 OR 条件可与 `module` / `bug_status` / `assigned_to` / `resolved_by` / `resolved_date_from` / `resolved_date_to` 混用；超出每组 3 条时会报错。

## TODO（搜索字段）

- 更多字段待补充

## 搜索行为结论（2026-03-04）

- 使用 `cookie_source=file`（`~/.zentao/cookies`）对同一组查询条件做了实测：
  - `title=系统测试`
  - `module=1099`
  - `status=active`
- 对比了 `title` 放在 `slot1` / `slot3` / `slot6` 三种 form 组合，返回结果一致（同页命中数与前序 Bug ID 一致）。
- 对比了更多参数的跨 slot 组合（`module`、`status`、`assignedTo`、`resolvedDate >= / <=`），返回结果也一致。
- 结论：在当前禅道实例下，查询条件的 **slot 序号本身不会改变搜索结果**；关键在于 `field/operator/value` 是否正确。

## 构建与运行

```bash
# 1) 构建二进制
cargo build --release

# 2) 查看帮助
./target/release/zentao --help

# 3) 运行示例（读取 cookie）
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao

# 4) 账号密码登录并保存 cookie 文件
./target/release/zentao login --url http://shendao.sharexm.cn/zentao \
  --username <username> --password '<password>'

# 5) 登录时显式指定代理（例如 socks5）
./target/release/zentao login --url http://shendao.sharexm.cn/zentao \
  --username <username> --password '<password>' \
  --proxy socks5h://127.0.0.1:1080
```

```bash
# 不落地 release 二进制，直接运行
cargo run -- --help
```

## 安装

```bash
# 安装到 ~/.cargo/bin（覆盖已有版本）
cargo install --path . --force

# 验证
which zentao
zentao --help
```

如果 `which zentao` 不是 `~/.cargo/bin/zentao`，请将以下配置加入 `~/.zshrc`：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## 使用示例

```bash
# 1) 列出并选择 Chrome profile，保存到配置
./target/release/zentao chrome profile

# 2) 读取 Cookie（默认使用 file: ~/.zentao/cookies）
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao

# 3) 临时覆盖 profile
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao \
  --profile "/Users/you/Library/Application Support/Google/Chrome/Profile 1"

# 4) 读取后执行校验
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao --verify

# 5) 按 Bug 详情 URL 输出 Markdown 到终端
./target/release/zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html

# 6) 按 Bug 详情 URL 输出 Markdown 到文件
./target/release/zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html --out ./bug-51214.md

# 7) 直接传 Bug 详情 URL（自动提取 ID，并优先按该 URL 的站点做 cookie 匹配）
./target/release/zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html --url http://shendao.sharexm.cn/zentao

# 7.1) 直接传 Bug 详情 URL 时，默认优先按该 URL 的站点做 cookie 匹配
#      不再支持纯数字 ID，避免跳转后的站点不一致导致找不到 cookie；
#      仅当显式传 --url 时才覆盖
./target/release/zentao bug show http://zentao.test.sharexm.cn/zentao/bug-view-51214.html

# 8) 直接下载图片 URL 到本地
./target/release/zentao image download --url http://shendao.sharexm.cn/zentao/file-read-59561.png

# 9) 搜索指派给某人的 Bug
./target/release/zentao search --assigned-to zhousong

# 10) 指定每页条数（通过 cookie pagerBugBrowse=<N> 传给禅道）
./target/release/zentao search --assigned-to zhousong --page-size 1000

# 11) 按解决者 + 解决日期范围搜索
./target/release/zentao search --resolved-by zhousong \
  --resolved-date-from 2025-11-14 --resolved-date-to 2025-11-22

# 12) 按所属模块 + Bug 状态搜索
./target/release/zentao search --module 1099 --bug-status active

# 13) 按标题关键词搜索（包含匹配）
./target/release/zentao search --title 系统测试 --module 1099 --bug-status active

# 14) 按标题模块分组（module）输出
./target/release/zentao search --group module

# 15) 按指派对象分组（assigned-to）输出 JSON
./target/release/zentao search --group assigned-to --json

# 16) 输出 JSON（便于脚本消费）
./target/release/zentao search --assigned-to zhousong --json

# 17) 打印 search-buildQuery 调试信息（精简 form + Lisp 条件树）
./target/release/zentao search --module 1099 --bug-status active \
  --assigned-to zhousong --debug
```

`--group` 说明：
- 取值：`module`（按标题前缀分组，如 `【IM数据库改造】`）、`assigned-to`（按指派对象分组）
- 排序规则（`--json` 与非 `--json` 一致）：
  - 先按分组内“最新创建时间”倒序排列分组
  - 再按每个分组内 bug 的创建时间倒序排列

## 配置说明

- 配置文件路径：`~/.zentao/config.json`
- 字段：
  - `url`（可被 `--url` 覆盖）
  - `cookie_source`：`chrome` 或 `file`（缺失时默认 `chrome`）
  - `chrome_profile`（由 `zentao chrome profile` 写入；仅在 `cookie_source=chrome` 时生效）
- Cookie 值持久化到 `~/.zentao/cookies`（Netscape cookie jar）
- `config.json` 仅保存非敏感配置项（如 `url`、`cookie_source`、`chrome_profile`）

## cookie 输出示例

`zentao cookie --url http://shendao.sharexm.cn/zentao` 输出为：

```text
Cookie source: /Users/you/.zentao/cookies
目标域名: shendao.sharexm.cn

cookie 域名: shendao.sharexm.cn [OK]

cookie 状态:
- zentaosid: [OK]
- za: [OK]
- zp: [OK]
- keepLogin: [OK]

cookie 明细:
name       value      domain              path      secure  httpOnly  expires
zentaosid  ...        shendao.sharexm.cn  /         false   true      session
za         ...        shendao.sharexm.cn  /zentao/  false   true      2026-03-31 ...
zp         ...        shendao.sharexm.cn  /zentao/  false   true      2026-03-31 ...
keepLogin  on         shendao.sharexm.cn  /zentao/  false   true      2026-03-31 ...
```

## 测试

```bash
# 运行全部测试
cargo test

# 仅运行 bug 模块测试（包含 fixture 回归）
cargo test bug::tests

# 仅运行某个模块测试
cargo test browser::tests

# 仅运行搜索模块测试
cargo test search::tests

# 仅运行 API 模块测试（含 search form 构建）
cargo test api::tests
```

更多测试说明见 `docs/TESTING.md`。

## 平台支持

- 当前仅支持 macOS（Chrome Cookie 读取依赖 macOS Keychain + Chrome SQLite）
