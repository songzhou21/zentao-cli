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

禅道搜索有两组、各三个条件槽位。默认 `active` 状态也会占用一个槽位；筛选条件触及上限且不需状态筛选时，使用 `--state all` 释放该槽位。

用户明确需要 JSON、脚本消费或字段筛选时，使用字段化 JSON：

```bash
zentao bug view 57801 --json
zentao bug list --json=id,title,state,assignee
```

`--json` 单独使用时输出全部字段；传字段列表时必须使用等号形式，例如 `--json=id,title`，以免吞掉 Bug ID 等位置参数。

可用列表字段：`id`、`title`、`state`、`severity`、`priority`、`confirmed`、`openedBy`、`openedDate`、`assignee`、`resolvedDate`、`resolution`、`deadline`、`url`。

`openedDate`、`resolvedDate` 和 `deadline` 保留禅道列表的原始日期文本，可能省略年份；不要把它们当作完整 ISO 日期或自行补全年份。

## 查看详情

`bug view` 接受 Bug ID 或完整详情 URL：

```bash
zentao bug view 57801
zentao bug view http://shendao.sharexm.cn/zentao/bug-view-57801.html
zentao bug view 57801 --json=id,title,description,images,attachments
```

传 URL 时，使用 URL 自己的站点；传 ID 时需要已配置的 Site。JSON 的 `images` 字段列出描述与历史记录中的图片 URL。默认输出 Markdown，包含 `# Bug #<id> <标题>`、`## 描述`、`## 历史记录` 和 `## 附件`。

## 排查 Bug

只有用户的目标是排查、修复或分析 Bug 时，才需要下载并查看描述中的图片。若附件包含 ZIP 日志，也下载到 `/tmp` 的独立目录后解压分析。普通查询、周报和提交说明不强制下载媒体。

图片下载使用本地 shell：

```bash
zentao image download --url "<image-url>" -o "/tmp/bug-<id>"
```

下载会使用当前 ZenTao Cookie；登录页或非图片响应会失败，不要把失败响应当作图片处理。

分析结论应结合描述、截图和日志时序；不要只根据标题下结论。

## 认证失败

出现 Cookie 失效或站点缺失时，先运行：

```bash
zentao auth status
zentao config list
```

默认 Cookie 来源是 Chrome；`auth login` 成功后会改用本地 `file` Cookie。
