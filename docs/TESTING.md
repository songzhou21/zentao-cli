# TESTING

本文说明 `zentao-cli` 的测试策略、目录约定和执行方式。

## 目标

- 优先保证回归稳定性
- 以 Unit Test 为主，少量集成测试兜底
- `zentao bug show` 通过固化 HTML fixture 做解析回归测试
- 避免依赖真实禅道、真实 Chrome 环境

## 测试分层

### 1. Unit Test（默认必须跑）

覆盖模块：

- `internal/config`
- `internal/api`
- `internal/bug`
- `internal/cli`（纯函数/参数与分支逻辑）
- `internal/browser`（仅纯函数和稳定逻辑）

重点：

- 表驱动测试（table-driven）
- 错误路径与边界条件
- 核心业务规则断言

### 2. 集成测试（可选，非黑盒进程）

- 不启动 CLI 子进程
- 通过拆分后的函数和依赖注入测试 `bug show` 业务流程
- 使用 fake 依赖模拟：
  - cookie 加载
  - bug html 获取
  - 输出写入

## `zentao bug show` 测试策略

采用“固化 HTML + 核心字段断言”。

### fixture 目录约定

`internal/bug/testdata/bug_html/`

建议样例：

- `bug_51214_full.html`：正常页面，含标题/描述/图片
- `bug_missing_title.html`：缺标题
- `bug_missing_desc.html`：缺描述

### 断言范围（核心字段）

- 标题提取正确（含选择器优先级）
- 描述成功转换 Markdown
- 相对图片地址补全为绝对地址
- 空 `alt` 自动命名：`img#<n>`
- 附件链接会被提取并追加到描述末尾（`attachment#1`、`attachment#2`）
- `RenderMarkdown` 头部与描述 section 格式正确

### 不做的断言

- 不做整段 Markdown 全量逐字符对比（降低脆弱性）

## browser 模块测试边界

`internal/browser` 先做最小必要单测，不做重型系统集成测试（Chrome DB/钥匙串）。

建议覆盖：

- `pkcs7Unpad`
- `hostMatches`
- `normalizePath`
- `chromeExpiresUTCToUnix`
- `chooseBestByPath`

## 运行方式

### 默认测试

```bash
go test ./...
```

### 覆盖率

```bash
go test -cover ./...
```

可选按包查看：

```bash
go test -cover ./internal/bug ./internal/api ./internal/config ./internal/cli ./internal/browser
```

## 覆盖率目标（首轮）

- `internal/api` >= 90%
- `internal/bug` >= 85%
- 全仓 >= 70%

## fixture 更新规范

采用：**手动更新 + 评审**

规则：

1. 页面结构变化时，人工抓取并替换 fixture
2. PR 中必须说明变更原因
3. Review 重点看：
- HTML 结构关键差异
- 断言是否仍覆盖核心规则
4. 禁止无说明批量刷新 fixture

## 编写规范

- 测试函数命名：`TestXxx`
- 统一使用表驱动：
  - `cases := []struct{ ... }{...}`
  - `t.Run(case.name, func(t *testing.T){ ... })`
- 每个 test case 需要有简洁注释（放在 case 定义处、`t.Run` 内第一行或函数前均可），无需固定措辞
- 错误断言需包含上下文信息
- 优先小而明确的断言，避免单测过度耦合实现细节
