---
name: zentao
description: 通过禅道 Bug 详情页 URL 抓取 Bug 详情并输出 Markdown；也支持按指派者/解决者/所属模块/Bug 状态/解决日期范围搜索 Bug。用于查看 bug 详情、分析 bug 上下文、编写 git commit message，以及整理周报/月报。适用于“提取禅道 bug”“根据 bug 链接抓详情”“导出 bug 描述”“分析某个 bug”“根据 bug 信息组织提交说明”“按条件搜索 bug”“按时间范围统计已解决问题”等请求。
---

# Zentao Bug Fetch & Search

## 目标

核心用途：

- 输入 `bug 详情 URL`
- 抓取该 bug 详情
- 输出结构化 Markdown（终端输出或写文件）
- 作为 bug 分析与 `git commit message` 编写参考
- 按条件搜索 bug（适合周报/月报素材收集）

## 输入规范

支持两类输入：

1. Bug 详情 URL
- 示例：`http://shendao.sharexm.cn/zentao/bug-view-51214.html`

2. 搜索条件
- `assigned_to`（指派给）
- `resolved_by`（解决者）
- `module`（所属模块 ID；iOS 的 module 值是 `1099`）
- `bug_status`（Bug 状态，如 `active`/`resolved`/`closed`）
- `group`（分组维度：`module` 或 `assigned-to`）
- `resolved_date_from`（解决日期起始，含）
- `resolved_date_to`（解决日期截止，含）
- `page_size`（每页条数，映射为 Cookie `pagerBugBrowse`，默认 `20`）

`zentao bug show` 仅支持传完整的 `BUG_URL`，不再支持纯数字 ID，避免跳转后的站点不一致导致找不到 cookie。

## 执行步骤

1. 确定站点地址
- 默认从 `BUG_URL` 推导站点地址，用于 cookie 匹配
- 若显式传了 `--url`，则以 `--url` 覆盖

2. 抓取 bug 详情
- 命令：

```bash
zentao bug show <BUG_URL>
```
- 该命令默认直接输出 Markdown 到 stdout

3. 输出
- 默认输出到终端（即上一步 stdout 的 Markdown）
- 需要落盘时使用：

```bash
zentao bug show <BUG_URL> --out ./bug-<id>.md
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
zentao image download --url "<image-url>" --output-dir "/tmp/bug52106"
zentao image download --url "<image-url>" -o "/tmp/bug52106"
```

可批量下载（推荐，直接消费 `zentao bug show` 的 stdout Markdown）：

```bash
zentao bug show <BUG_URL> \
  | grep -Eo '!\[[^]]*\]\((https?://[^)]+)\)' \
  | sed -E 's/.*\((https?:\/\/[^)]+)\).*/\1/' \
  | while read -r url; do
      zentao image download --url "$url"
    done
```

4.4 失败处理
- 若访问 `sharexm.cn` 失败，直接申请放开该域名网络权限（含 DNS 解析与 HTTP 访问）后重试

4.5 ZIP 附件日志处理（仅 fix bug / 排查 bug 场景）
- 当 `zentao bug show <BUG_URL>` 输出中的 `## 附件` 出现 `.zip` 链接，且任务目标是 `fix bug`、`排查 bug`、`分析日志` 时，必须下载并分析 ZIP 日志
- 典型样例：
  - `[21d3aabf_212885_20260323102807.zip](https://resource.sharexm.com.cn/im/log/iOS/202603/23/21d3aabf_212885_20260323102807.zip)`
  - `[21d3aabf_livekit_212885_20260323_102807.zip](https://resource.sharexm.com.cn/im/log/iOS/202603/23/21d3aabf_livekit_212885_20260323_102807.zip)`
- 必须使用 **local shell** 下载，不要在 hosted/container shell 中处理
- 下载后统一解压到 `/tmp` 目录下的独立子目录，再分析日志内容
- 建议流程：先从 `## 附件` 提取 ZIP URL，下载到 `/tmp`，再解压到同名目录，例如：

```bash
curl -L "<zip-url>" -o "/tmp/<zip-name>.zip"
unzip -o "/tmp/<zip-name>.zip" -d "/tmp/<zip-name>"
```

- 分析时优先关注崩溃前后时序、错误关键字、网络请求失败、音视频/LiveKit 相关状态流转、用户操作链路

4.6 场景约束
- `排查 bug`：必须下载图片并查看；若 `## 附件` 中存在 ZIP 日志，也必须解压到 `/tmp` 后结合日志一起分析；不能只基于文字描述
- `git commit`：下载图片和 ZIP 日志都是可选动作；若文字信息充分，可不下载
- 排查时结论需结合截图中的 UI 状态、按钮文案、抓包字段、日志时序

## 输出格式约束

输出内容必须包含：

- `# Bug #<id> <标题>`
- `## 描述`

并遵循：

- 描述中的图片地址补全为绝对 URL
- 空图片 alt 自动命名为 `img#<n>`
- 有附件时输出独立的 `## 附件` section

## 错误处理

- `bug URL` 缺失或解析失败：报参数错误
- 页面跳登录：报 cookie 失效
- 页面为空：报“页面内容为空”
- 缺标题/缺描述：报解析错误

## 常用示例

```bash
# 通过 bug 详情 URL 抓取（自动提取 ID）
zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html 

# 通过 bug 详情 URL 抓取并写文件
zentao bug show http://shendao.sharexm.cn/zentao/bug-view-51214.html --out ./bug-51214.md
```

## 搜索（周报/月报）

用于“按人 + 按时间范围”汇总已处理问题，优先使用 `zentao search`。

1. 基础搜索

```bash
# 搜索指派给某人的 Bug
zentao search --assigned-to zhousong

# 指定每页条数（默认 20）
zentao search --assigned-to zhousong --page-size 1000

# 按解决者 + 解决日期范围搜索
zentao search --resolved-by zhousong \
  --resolved-date-from 2025-11-01 --resolved-date-to 2025-11-30

# 按所属模块 + Bug 状态搜索
# iOS 模块示例：module=1099
zentao search --module 1099 --bug-status active

# 按标题模块分组展示
zentao search --group module

# 按指派对象分组，并输出 JSON
zentao search --group assigned-to --json
```

2. 周报场景（建议输出 JSON 再二次加工）

```bash
# 示例：统计某人一周内解决的问题
zentao search --resolved-by zhousong \
  --resolved-date-from 2025-11-17 --resolved-date-to 2025-11-23 \
  --json
```

3. 月报场景

```bash
# 示例：统计某人当月解决的问题
zentao search --resolved-by zhousong \
  --resolved-date-from 2025-11-01 --resolved-date-to 2025-11-30 \
  --json
```

4. 使用约束与建议

- 周报/月报优先用搜索结果做事实清单，再按需要补充 `zentao bug show <BUG_URL>` 的详情描述
- 时间范围建议使用自然周/自然月，避免跨周期重复统计
- 若用于自动化脚本，优先 `--json` 以减少文本解析误差
- 搜索默认会带 `pagerBugBrowse=20`；如需更多结果可显式传 `--page-size`（如 `1000`）
- 使用 `--group` 时，`--json` 与非 `--json` 的排序规则一致：先按分组内最新创建时间倒序分组，再按组内 bug 创建时间倒序
