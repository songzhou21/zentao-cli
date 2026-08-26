# Zentao CLI

面向禅道日常 Bug 查询、详情获取和认证管理的命令行上下文。术语以禅道的实际模型为准，而非套用其他缺陷管理工具的名称。

## Language

**Bug**:
禅道中被追踪的缺陷实体，以站点内唯一的 Bug ID 标识。
_Avoid_: Issue, ticket

**Site**:
一个禅道部署的基础 URL，包含可能存在的部署子路径，例如 `https://example.com/zentao`。
_Avoid_: Hostname, server

**Product**:
禅道中限定 Bug 列表查询范围的产品实体，以产品 ID 标识。
_Avoid_: Repository, project

**Cookie Source**:
CLI 读取认证会话的来源；可为 Chrome Profile 或本地 Cookie 文件。
_Avoid_: Login method, credential store

**Bug List**:
在一个 Site 和 Product 作用域内，按禅道可验证筛选条件查询得到的 Bug 集合。数据来自浏览 JSON，不是列表 HTML。`--json` 用完整时间、禅道 `resolution` 代码、布尔 `confirmed` 和显示名；人类表格投影同一套 JSON 字段。
_Avoid_: Search result, report, 列表 HTML 短日期, 中文解决方案

**Bug View**:
按 Bug ID 或详情 URL 读取单条 Bug 的详情。默认打印完整 JSON；`--json=fields` 裁剪字段。`--markdown` 从完整 JSON 投影人类 Markdown，不是第二套详情数据。状态用三态 `state`（激活 / 待验证 / 关闭），不用解决方案 `resolution`。人员字段用显示名；上线版本用展示名。创建时间、解决时间是独立字段。描述和备注正文是去掉全部样式后的 HTML 结构。图片列表从 HTML 的 `<img>` 收集。历史是事件数组，不是一篇叙述。
_Avoid_: show, 页面刮取, 解决状态, 账号当人员字段, 版本 ID, 默认必须带 --json, 从 Markdown 反解 images, history 字符串, 人读另算一套

**Description HTML**:
Bug 描述（及历史备注正文）在详情 JSON 里的形态：保留 `p` `br` `ol` `ul` `li` `img` `a`；拆掉 `span` `strong` `b` `em` `i` 等包装；去掉全部 `style` / `class` / `onload` 等属性。`img` 只留绝对 `src`，`a` 只留 `href`。`images` 从这些 `<img src>` 按出现顺序去重收集。
_Avoid_: Markdown 正文, 带皮肤的编辑器 HTML, 强调标签, 从 Markdown 再抠图

**History Entry**:
Bug View JSON 里的一条历史。字段为 `at`、`action`（禅道动作名，如 `opened` / `assigned` / `edited` / `resolved` / `commented`）、`actor`（显示名）；按需有 `assignee`、`changes`、`comment`。空键省略。`changes` 每项为 `field`（禅道字段名）+ `label`（中文，如 `resolvedBuild` / `上线版本`）+ `old` + `new`。例行变更（指派给、解决方案、状态、消耗等）不进 `changes`；`assigned` 的对象写在 `assignee`。`comment` 与描述同一套去样式 HTML。
_Avoid_: 历史 Markdown 字符串, 中文叙述头, 把指派写进 changes, field 与中文焊成一个字符串

**Opened Date**:
Bug 的创建时间。Bug List 用浏览 JSON、Bug View 用详情 JSON，都是完整时间（如 `2026-08-13 12:10:03`），不是列表 HTML 里省略年份的展示文本。
_Avoid_: 从 history 字符串解析日期, 列表 HTML 短日期

**Resolved Date**:
Bug 的解决时间。Bug List / Bug View JSON 同样用接口完整时间；未解决则为空。
_Avoid_: 从 history 字符串解析日期, 列表 HTML 短日期

**Display Name**:
禅道 `users` 对照得到的中文名，如 `周松`。Bug List / Bug View JSON 的 `openedBy`、`resolvedBy` 用显示名。筛选参数（`--opened-by`、`--resolved-by`）仍用账号。
_Avoid_: 用账号填 JSON 人员字段

**Resolved Build**:
Bug 解决时选定的上线版本。Bug View JSON 的 `resolvedBuild` 是 `builds` 对照后的展示名，不是版本 ID。
_Avoid_: build ID, 构建号

**Bug Stats**:
在与 Bug List 相同的筛选母集上，先提交搜索再读浏览 JSON，不刮列表页 HTML。主表：激活按当前指派人；已解决 / 关闭按解决者；合计是列加总（激活+已解决+关闭）。写出量从已解决+关闭推导。待验证单独一块（`pending`），按当前指派人、状态 `resolved`。人类表格投影同一套 JSON 字段。表头中文；JSON 字段名仍为英文。`resolved` 表示待验证，`solved` 表示已解决，`closed` 表示关闭，`total` 表示主表列加总。样本受 `-L/--limit` 约束，不保证全集。
_Avoid_: dashboard, 通用 group-by engine, 列表 HTML, 标题前缀分组

**Bug Report**:
与 Bug List / Bug Stats 同一套搜索样本，再按标题第一个 `【…】` 内文分组（不是禅道 module）。JSON `groupBy` 为 `titlePrefix`；`groups[].name` 无括号（如 `"系统测试"`）。分桶只看状态：`resolved` / `closed` / `other`。人类 markdown 必须从完整 `--json` 投影：组头把 `name` 包成 `【系统测试】`，条目用 `displayTitle`。默认 `-s resolved -s closed`、`-L 1000`（不含激活）。`--weekly` 为上周五～本周四。
_Avoid_: 把标题前缀当搜索条件, 用 module 分组, 人读另算一套, 硬编码解决者/产品, pending/willnotfix 当分桶

**Active / Resolved / Closed**:
禅道 Bug 状态在 CLI 中的规范值。`active` 为激活；`resolved` 为待验证（非结案）；`closed` 为关闭。
_Avoid_: Open/fixed/done（除非映射到上述三态）
