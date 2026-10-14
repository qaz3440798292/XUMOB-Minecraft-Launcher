# Minecraft 登录被拒绝：排查方法

HTTP 403 表示请求被拒绝，单凭状态码无法判断原因。早期版本把所有 Minecraft 登录 403 都解释成应用权限问题；当前版本会按服务响应区分原因。

## 看诊断信息

在账户窗口复制诊断文字：

- `Invalid app registration`：Minecraft 服务明确指出应用注册无效。请应用所有者检查 Client ID，并核对 Minecraft 服务访问许可。
- `未明确说明`：服务返回了 JSON，但没有说明应用注册无效。单凭这条信息无法确认是否缺少许可。
- `非 JSON 响应`：返回内容格式不符合预期，原因仍未确定。

诊断文字包含认证阶段、HTTP 状态、固定错误类别和有效格式的请求编号，不包含服务原文、密码、登录代码或令牌。

## 下一步

1. 更新到最新版，并重新完成一次微软登录。
2. 若明确收到 `Invalid app registration`，由应用所有者通过 [Minecraft 应用审核入口](https://aka.ms/mce-reviewappid) 查询或申请许可。也可参考微软的 [Java Edition Game Service API 审核说明](https://help.minecraft.net/hc/en-us/articles/16254801392141)。
3. 若分类不明确，保留诊断编号并检查网络、账户状态和应用注册。无需发送账户数据目录或任何令牌。

启动器不包含公开的默认 Client ID。微软设备授权代码申请成功，也不能证明 Minecraft 服务已批准该应用。应用配置和数据保存方式见[正版登录说明](正版登录说明.md)。

Minecraft 登录后还需完成 Xbox Live、XSTS、游戏许可和 Java 版角色检查。某一步失败时，诊断文字会指出失败阶段。
