---
name: zentao
description: 通过禅道 Bug ID 或禅道 Bug 详情页 URL 抓取 Bug 详情并输出 Markdown。用于查看 bug 详情、分析 bug 上下文，并在编写 git commit message 时提供问题背景与描述参考。适用于“提取禅道 bug”“根据 bug 链接抓详情”“导出 bug 描述”“分析某个 bug”“根据 bug 信息组织提交说明”等请求。
---

# Zentao Bug Fetch

## 目标

核心用途：

- 输入 `bug id` 或 `bug 详情 URL`
- 抓取该 bug 详情
- 输出结构化 Markdown（终端输出或写文件）
- 作为 bug 分析与 `git commit message` 编写参考

## 输入规范

支持两种输入：

1. Bug ID
- 示例：`51214`

2. Bug 详情 URL
- 示例：`http://shendao.sharexm.cn/zentao/bug-view-51214.html`

`zentao bug show` 已支持直接传 `ID_OR_URL`。

## 执行步骤

1. 确定站点地址
- 使用 `~/.zentao/config.json` 中的 `url`
- 若未配置，先完成 `zentao` CLI 的站点配置

2. 抓取 bug 详情
- 命令：

```bash
zentao bug show <ID_OR_URL>
```
- 该命令默认直接输出 Markdown 到 stdout

3. 输出
- 默认输出到终端（即上一步 stdout 的 Markdown）
- 需要落盘时使用：

```bash
zentao bug show <ID_OR_URL> --out ./bug-<id>.md
```

4. 当输出里包含图片链接时，按下面流程在 **local shell** 下载图片

4.1 触发条件
- Markdown 描述中出现 `![img#n](http://.../file-read-xxxx.png)` 等图片链接

4.2 执行环境要求
- 必须使用本地 shell 执行（OpenAI shell tool `environment.type: "local"`）
- 不要在 hosted/container shell 中下载图片

4.3 下载命令（local shell，优先使用 zentao 内置下载命令）

```bash
zentao image download --url "<image-url>"
```

可批量下载（推荐，直接消费 `zentao bug show` 的 stdout Markdown）：

```bash
zentao bug show <ID_OR_URL> \
  | rg -o '!\[[^]]*\]\((https?://[^)]+)\)' \
  | sed -E 's/.*\((https?:\/\/[^)]+)\).*/\1/' \
  | while read -r url; do
      zentao image download --url "$url"
    done
```

fallback（网络受限或手工排障时）：

```bash
mkdir -p /tmp/zentao-images
curl -fL "<image-url>" -o "/tmp/zentao-images/<bug-id>-img-<n>.png"
```

4.4 失败处理
- 若访问 `sharexm.cn` 失败，直接申请放开该域名网络权限（含 DNS 解析与 HTTP 访问）后重试

4.5 场景约束
- `排查 bug`：必须下载图片并查看后再分析；不能只基于文字描述
- `git commit`：下载图片是可选动作；若文字信息充分，可不下载
- 排查时结论需结合截图中的 UI 状态、按钮文案、抓包字段

## 输出格式约束

输出内容必须包含：

- `# Bug #<id> <标题>`
- `## 描述`

并遵循：

- 描述中的图片地址补全为绝对 URL
- 空图片 alt 自动命名为 `img#<n>`
- 有附件时追加 `Attachments:` 列表

## 错误处理

- `bug id` 缺失或解析失败：报参数错误
- 页面跳登录：报 cookie 失效
- 页面为空：报“页面内容为空”
- 缺标题/缺描述：报解析错误

## 常用示例

```bash
# 通过 bug id 抓取
zentao bug show 51214

# 通过 bug id 抓取并写文件
zentao bug show 51214 --out ./bug-51214.md

# 通过 bug 详情 URL 抓取（自动提取 ID）
zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html 
```
