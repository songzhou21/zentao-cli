---
name: zentao
description: 查询和查看禅道 Bug；适用于按标题、指派人、解决人、日期、模块或状态筛选 Bug，获取详情、分析 Bug、整理周报/月报和编写提交说明。
---

# Zentao Bug Workflow

使用 `zentao` CLI 获取禅道 Bug。命令面只使用下列入口：

```bash
zentao bug list
zentao bug view <ID|URL>
zentao auth status
zentao image download --url <URL>
```

## 搜索 Bug

按用户意图选择真实、已验证的禅道筛选条件：

```bash
# 标题包含匹配；重复 --title 最多 3 个，按 OR
zentao bug list --title 会议

# 指派人、状态和数量
zentao bug list -a zhousong -s active -L 100

# 已解决 Bug 的解决人和日期范围
zentao bug list --resolved-by zhousong \
  --resolved-from 2026-07-01 --resolved-to 2026-07-31

# 所属模块
zentao bug list --module 1099
```

`bug list` 默认查询 `active` 状态、最多返回 30 条。产品范围来自 `--product`、`ZENTAO_PRODUCT` 或配置；缺失时先提示用户配置产品，不要猜测产品 ID。

用户明确需要 JSON、脚本消费或字段筛选时，使用字段化 JSON：

```bash
zentao bug view 57801 --json
zentao bug list --json id,title,state,assignee
```

`--json` 单独使用时输出全部字段；传字段列表时只输出指定字段。

可用列表字段：`id`、`title`、`state`、`severity`、`priority`、`confirmed`、`openedBy`、`openedDate`、`assignee`、`resolvedDate`、`resolution`、`deadline`、`url`。

## 查看详情

`bug view` 接受 Bug ID 或完整详情 URL：

```bash
zentao bug view 57801
zentao bug view http://shendao.sharexm.cn/zentao/bug-view-57801.html
zentao bug view 57801 --json id,title,description,images,attachments
```

传 URL 时，使用 URL 自己的站点；传 ID 时需要已配置的 Site。JSON 的 `images` 字段列出描述与历史记录中的图片 URL。默认输出 Markdown，包含 `# Bug #<id> <标题>`、`## 描述`、`## 历史记录` 和 `## 附件`。

## 排查 Bug

只有用户的目标是排查、修复或分析 Bug 时，才需要下载并查看描述中的图片。若附件包含 ZIP 日志，也下载到 `/tmp` 的独立目录后解压分析。普通查询、周报和提交说明不强制下载媒体。

图片下载使用本地 shell：

```bash
zentao image download --url "<image-url>" -o "/tmp/bug-<id>"
```

分析结论应结合描述、截图和日志时序；不要只根据标题下结论。

## 认证失败

出现 Cookie 失效或站点缺失时，先运行：

```bash
zentao auth status
zentao config list
```

默认 Cookie 来源是 Chrome；`auth login` 成功后会改用本地 `file` Cookie。除非用户要求，不要显示 Cookie 值。
