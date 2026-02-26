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
- 优先用显式参数 `--url`
- 否则回退 `~/.zentao/config.json` 的 `url`

2. 抓取 bug 详情
- 命令：

```bash
zentao bug show <ID_OR_URL> --url <zentao-url>
```

3. 输出
- 默认输出到终端
- 需要落盘时使用：

```bash
zentao bug show <ID_OR_URL> --url <zentao-url> --out ./bug-<id>.md
```

4. 当输出里包含图片链接时，用 `curl` 下载图片，图片目录可以是 `/tmp`
- 触发条件：Markdown 描述中出现 `![img#n](http://.../file-read-xxxx.png)` 等图片链接
- 若访问 `sharexm.cn` 失败，直接申请放开该域名网络权限（含 DNS 解析与 HTTP 访问）后重试
- `排查 bug` 场景：必须使用 `curl` 下载图片到本地，并查看图片内容后再进行问题分析
- `git commit` 场景：下载图片为可选动作；如仅需组织提交说明且文字信息充分，可不下载图片

- 分析要求（排查 bug 时）：不能只基于文字描述；需要结合截图中的 UI 状态、按钮文案、抓包字段给出结论

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
zentao bug show 51214 --url http://shendao.sharexm.cn/zentao/

# 通过 bug id 抓取并写文件
zentao bug show 51214 --url http://shendao.sharexm.cn/zentao/ --out ./bug-51214.md

# 通过 bug 详情 URL 抓取（自动提取 ID）
zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html --url http://shendao.sharexm.cn/zentao/
```
