# zentao-cli

一个用于禅道日常工作的命令行工具：读取/校验 Cookie、登录、抓取 Bug 详情、下载图片、按条件搜索 Bug。

## 适合谁用

- 需要在终端快速查看禅道 Bug
- 需要把 Bug 信息整理到周报/月报
- 需要脚本化搜索 Bug（支持 JSON 输出）

## 安装

```bash
cargo install --path . --force
zentao --help
```

如果 `zentao` 不在 PATH，请在 `~/.zshrc` 添加：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## 快速开始

1. 准备站点地址（例如 `http://shendao.sharexm.cn/zentao`）
2. 准备 Cookie（默认从 `~/.zentao/cookies` 读取）
3. 验证可用性：

```bash
zentao cookie --url http://shendao.sharexm.cn/zentao --verify
```

## 常用命令

```bash
# 1) 登录并写入 cookie 文件
zentao login --url http://shendao.sharexm.cn/zentao \
  --username <username> --password '<password>'

# 2) 通过 Bug ID 或详情 URL 输出 Markdown
zentao bug show 51214
zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html

# 3) 下载描述中的图片
zentao image download --url http://shendao.sharexm.cn/zentao/file-read-59561.png

# 4) 搜索（文本输出）
zentao search --assigned-to zhousong
zentao search --title 系统测试 --module 1099 --bug-status active

# 5) 搜索（JSON 输出，便于脚本处理）
zentao search --resolved-by zhousong \
  --resolved-date-from 2025-11-01 --resolved-date-to 2025-11-30 \
  --json
```

## 测试

```bash
cargo test
```

## 文档分工

- `README.md`：面向使用者的快速上手与常见命令
- `AGENTS.md`：完整细节（字段、行为结论、扩展说明、更多示例）
