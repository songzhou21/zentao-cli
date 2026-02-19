# zentao-cli

禅道 CLI 工具，当前以 Chrome（macOS）中的登录会话 Cookie 为认证来源。

## 功能

- 每次从 Chrome 读取 zentao Cookie（`za/zentaosid/zp`）
- 输出 Cookie 明细（含到期时间格式化）
- 可选校验 Cookie 是否有效（根路径重定向规则）
- 支持管理 Chrome Profile 并保存到 `config.json`

## 使用示例

```bash
# 1) 列出并选择 Chrome profile，保存到配置
zentao chrome profile

# 2) 读取 Cookie（默认使用配置中的 chrome_profile）
zentao cookie --url http://shendao.sharexm.cn/zentao

# 3) 临时覆盖 profile
zentao cookie --url http://shendao.sharexm.cn/zentao \
  --profile "/Users/you/Library/Application Support/Google/Chrome/Profile 1"

# 4) 读取后执行校验
zentao cookie --url http://shendao.sharexm.cn/zentao --verify
```

## 配置说明

- 配置文件路径：`~/.zentao/config.json`
- 字段：
  - `url`（可被 `--url` 覆盖）
  - `api_version`
  - `chrome_profile`（由 `zentao chrome profile` 写入）
- Cookie 不会持久化到配置文件

## 输出说明

`zentao cookie` 会输出：
- 高亮过期时间（黄色）
- 明细字段：`name/value/domain/path/secure/httpOnly`
- 指定 `--verify` 时校验结果：成功绿色高亮，失败红色高亮（并返回非 0）

其中过期时间为格式化后的 UTC 时间；会话 Cookie 显示为 `session`。
