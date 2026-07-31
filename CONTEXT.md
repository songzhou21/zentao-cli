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
在一个 Site 和 Product 作用域内，按禅道可验证筛选条件查询得到的 Bug 集合。
_Avoid_: Search result, report
