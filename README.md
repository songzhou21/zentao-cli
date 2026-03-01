# zentao-cli (Rust)

禅道 CLI 工具，当前以 Chrome（macOS）中的登录会话 Cookie 为认证来源。

## 功能

- 每次从 Chrome 读取 zentao Cookie（`za/zentaosid/zp`）
- 输出 Cookie 明细（含到期时间格式化）
- 可选校验 Cookie 是否有效（根路径重定向规则）
- 支持管理 Chrome Profile 并保存到 `config.json`
- 支持按 Bug ID 抓取详情并输出 Markdown（正文图片地址自动补全）
- 支持按图片 URL 直接下载到本地（`image download --url`）
- 支持搜索 Bug（按指派者、解决者、解决日期范围筛选，输出文本列表）

## 搜索字段（当前支持）

- `assigned_to`（指派给）
- `resolved_by`（解决者）
- `resolved_date_from`（解决日期起始，含）
- `resolved_date_to`（解决日期截止，含）

## TODO（搜索字段）

- 更多字段待补充

## 构建与运行

```bash
# 1) 构建二进制
cargo build --release

# 2) 查看帮助
./target/release/zentao --help

# 3) 运行示例（读取 cookie）
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao
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

# 2) 读取 Cookie（默认使用配置中的 chrome_profile）
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao

# 3) 临时覆盖 profile
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao \
  --profile "/Users/you/Library/Application Support/Google/Chrome/Profile 1"

# 4) 读取后执行校验
./target/release/zentao cookie --url http://shendao.sharexm.cn/zentao --verify

# 5) 按 Bug ID 输出 Markdown 到终端
./target/release/zentao bug show 51214 --url http://shendao.sharexm.cn/zentao

# 6) 按 Bug ID 输出 Markdown 到文件
./target/release/zentao bug show 51214 --url http://shendao.sharexm.cn/zentao --out ./bug-51214.md

# 7) 直接传 Bug 详情 URL（自动提取 ID）
./target/release/zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html --url http://shendao.sharexm.cn/zentao

# 8) 直接下载图片 URL 到本地
./target/release/zentao image download --url http://shendao.sharexm.cn/zentao/file-read-59561.png

# 9) 搜索指派给某人的 Bug
./target/release/zentao search --assigned-to zhousong

# 10) 按解决者 + 解决日期范围搜索
./target/release/zentao search --resolved-by zhousong \
  --resolved-date-from 2025-11-14 --resolved-date-to 2025-11-22

# 11) 输出 JSON（便于脚本消费）
./target/release/zentao search --assigned-to zhousong --json
```

## 配置说明

- 配置文件路径：`~/.zentao/config.json`
- 字段：
  - `url`（可被 `--url` 覆盖）
  - `chrome_profile`（由 `zentao chrome profile` 写入）
- Cookie 不会持久化到配置文件

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
