---
name: zentao
description: 查询和查看禅道 Bug；适用于按标题、指派人、解决人、日期、模块或状态筛选 Bug，获取详情、分析 Bug、整理周报/月报和编写提交说明。
---

# Zentao Bug Workflow

用 `zentao` CLI 查禅道。**Agent 默认加 `--json`（或 `--json=fields`）读结构化输出**，不要依赖人类可读表格。

入口：`bug list` / `bug stats` / `bug view <ID|URL>` / `auth status` / `image download --url <URL>`。

## 列表

```bash
# 默认 active、最多 30 条；product 来自 --product / ZENTAO_PRODUCT / 配置，缺失则提示配置，禁止猜 ID
zentao bug list --json=id,title,state,assignee,url

# 筛选（按需组合）
zentao bug list --title 会议 --json=id,title,state,assignee,url
zentao bug list -a zhousong -s active -L 100 --json=id,title,state,assignee
zentao bug list --opened-by chenjie --opened-by niuweilong --opened-by cuiwenbo -s active --json=id,title,state,openedBy,assignee,url
zentao bug list --resolved-by zhousong --resolved-from 2026-07-01 --resolved-to 2026-07-31 -s all --json
zentao bug list --module 1099 --json=id,title,state
```

- `--title` 可重复最多 3 个，OR；不是展示开关。
- `--opened-by` 可重复最多 3 个，OR；值为用户**账号**（如 `chenjie`），列表展示的中文名不能直接当 value。
- 状态：`active`（默认）/ `resolved` / `closed` / `all`。查已解决/关闭或解决日期范围时要显式 `-s`。
- 搜索两组各 3 槽；默认 `active` 占一槽。条件顶满且不需状态时用 `--state all`。
- `--json` 必须用等号形式指定字段：`--json=id,title`（裸 `--json` = 全部字段）。
- 列表字段：`id` `title` `state` `severity` `priority` `confirmed` `openedBy` `openedDate` `assignee` `resolvedBy` `resolvedDate` `resolution` `deadline` `url`。
- 日期是完整时间（如 `2026-08-20 11:30:31`）。`resolution` 是禅道代码（`fixed` 等）。`confirmed` 为布尔。人员是显示名。`resolvedBy` 未解决为 `null`。

## 统计

```bash
zentao bug stats --title 会议优化5.1 --json
zentao bug stats --title 会议优化5.1 --json=assignee,active,solved,closed,total
```

- 两张表：主表人员 / 激活 / 已解决 / 关闭 / 合计；待验证单独一张。
- 激活按当前指派给；已解决 / 关闭按解决者。合计 = 激活+已解决+关闭。写出量 = 已解决+关闭。
- 默认 `--state all`、`-L 1000`；样本制，触顶时 `incomplete` 为 `true`。无 `--by`。
- 筛选参数与 list 相同。
- JSON：`rows` 为 `assignee,active,solved,closed,total`；`pending.rows` 为 `assignee,resolved`（待验证）。`solved` 为已解决，`total` 为主表列加总。

## 详情

```bash
zentao bug view 57801
zentao bug view 57801 --json=id,title,state,assignee,description,history,images,attachments,url
zentao bug view http://example/zentao/bug-view-57801.html --json=id,title,history
zentao bug view 58441 --raw-json
```

- ID 需已配置 site；URL 用 URL 自身 site。默认打完整 JSON；`--json=fields` 裁剪。
- 详情字段：`id` `title` `priority` `state` `openedBy` `openedDate` `assignee` `resolvedBy` `resolvedDate` `resolvedBuild` `description` `history` `images` `attachments` `url`。
- `state`：`active` / `resolved` / `closed`。人员是显示名；`resolvedBuild` 是上线版本展示名。
- `description` 与 `history[].comment` 是去样式 HTML，不是 Markdown。`images` 从描述/备注 HTML 的 `<img src>` 收集。
- `history` 是数组：`at` `action` `actor`，按需 `assignee` `changes` `comment`。`changes` 含 `field` `label` `old` `new`。
- `url` 稳定为 `<site>/bug-view-<id>.html`。接口原文用 `--raw-json`。

## 排查（仅当用户要分析/修 Bug）

```bash
zentao image download --url "<image-url>" -o "/tmp/bug-<id>"
```

用当前 Cookie；非 `image/*` 或登录页则失败。ZIP 日志可下到 `/tmp` 再解压。结合描述/图/日志，勿只看标题。

## 认证失败

```bash
zentao auth status
zentao config list
```

默认 Chrome Cookie；`auth login` 成功后改用 `file`。
