# 禅道 CLI 技术方案（Go）

## 概述

当前认证策略为：**每次命令执行时从 Chrome（macOS）读取现有会话 Cookie，不将 Cookie 持久化到 `config.json`**。

## 目标

- 稳定读取浏览器中当前有效的 zentao 会话 Cookie
- 提供 Cookie 有效性校验能力
- 提供可选的 Chrome Profile 管理能力并持久化选择
- 支持按 Bug ID 抓取详情并导出 Markdown
- 降低配置文件中敏感信息落盘风险

## 认证与校验规则

- Cookie 来源：Chrome `Cookies` SQLite（macOS）
- 关键 Cookie：`za`、`zentaosid`、`zp`
- 校验规则：
  1. 访问站点根路径（如 `http://shendao.sharexm.cn/zentao`）
  2. 若最终跳转到 `/my/` 判定有效
  3. 若最终跳转到 `user-login-*.html` 判定失效

## CLI 设计

- `zentao cookie`
  - 参数：`--url`、`--profile`、`--verify`、`--api-version`、`--config`
  - profile 优先级：`--profile` > `config.chrome_profile` > 自动选择最新 profile
- `zentao chrome profile`
  - 列出可用 Chrome profiles
  - 交互选择并保存到 `config.chrome_profile`
- `zentao bug show <id>`
  - 参数：`--url`、`--profile`、`--config`、`--out`
  - 从 Bug 页面抓取标题和描述，输出 Markdown
  - `--out` 指定后写入文件并返回写入提示（不在终端回显正文）

## Bug 抓取策略

- 页面地址模板：`<url>/bug-view-<id>.html`
- 例如：`http://shendao.sharexm.cn/zentao/bug-view-51214.html`
- 解析内容：
  - 标题（优先 `div.page-title span.text`，优先使用其 `title` 属性）
  - 描述（HTML 转 Markdown）
  - 描述中的图片地址自动补全为绝对地址（保留在正文中）
  - 空图片 alt 自动命名：`img-<编号>-<序号>`（如 `img-59651-1`）

## 配置文件

- 路径：`~/.zentao/config.json`
- 主要字段：
  - `url`
  - `chrome_profile`（可选）
- Cookie 不落盘

## 数据流

1. 读取 `url`（命令行优先，配置回退）
2. 确定 profile（CLI 覆盖 > 配置 > 自动）
3. 从 Chrome 读取并解密 Cookie
4. 访问目标页面（Cookie 鉴权）
5. 输出结果：
   - `cookie`：过期时间+明细；可选校验
   - `bug show`：Markdown（标题、描述）

## 输出

- `cookie`：
  - 高亮过期时间（UTC 格式化；会话 Cookie 为 `session`）
  - 明细字段：`name/value/domain/path/secure/httpOnly`
  - 校验结果：成功绿色，失败红色
- `bug show`：
  - `# Bug #<id> <标题>`
  - `## 描述`
  - 正文中的图片链接为绝对地址
  - 不单独输出“图片地址”分节

## 安全建议

- 默认不将 Cookie 持久化到配置文件
- 输出日志避免泄露到共享日志系统（含完整 cookie 值）
