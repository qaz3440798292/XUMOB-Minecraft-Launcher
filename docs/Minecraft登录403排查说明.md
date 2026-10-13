# XUMOB Minecraft Launcher v0.5.0 · Minecraft 登录拒绝排查

旧版提示“Minecraft 服务拒绝了此应用。请确认 Client ID 已获准访问 Minecraft 服务。”只能说明 Minecraft 登录接口返回 HTTP 403。旧版未区分响应内容，因此不能单凭该提示确定应用没有许可。

## 本次修正

保留界面布局与认证协议，修正错误分类。登录失败时显示可选中复制的诊断文字：

- `Invalid app registration`：服务明确拒绝应用注册，需要应用所有者核对 Client ID 和 Minecraft 服务访问许可。
- `未明确说明`：服务返回 JSON，但未明确表示应用注册无效，仍需进一步排查。
- `非 JSON 响应`：收到的内容不是预期 JSON，不能据此断定应用未获准。

诊断只显示认证阶段、HTTP 状态、固定分类及有效 UUID 格式的请求编号，不显示原始响应、登录代码、密码或令牌。

## 重试

1. 解压 v0.5.0，关闭旧版。在“账户 → 微软正版 → 登录应用配置”中确认 Client ID 是你要使用的注册应用。
2. 重新完成微软登录。若仍失败，复制账户窗口底部的诊断文字，用于继续定位。不要发送自己的 data 文件夹或账户凭据。
3. 如果明确显示 `Invalid app registration`，由应用所有者通过 [Minecraft 应用审核入口](https://aka.ms/mce-reviewappid) 申请或确认权限；官方帮助主题为 [Java Edition Game Service API Review or Application Process](https://help.minecraft.net/hc/en-us/articles/16254801392141)。已有许可但仍被拒绝时，也应由所有者向该审核渠道核实注册状态。

当前默认 ID 仍为用户提供的 `530c101a-f756-496e-8099-6addb6ed06ed`。已验证它能获取微软设备授权代码，但这不能证明其 Minecraft 服务许可状态。无法仅从公开 GUID 查证应用所有权或批准状态。

## 验证范围

认证请求与 [HMCL 的认证实现](https://github.com/HMCL-dev/HMCL/blob/main/HMCLCore/src/main/java/org/jackhuang/hmcl/auth/microsoft/MicrosoftService.java) 核对，未发现 Minecraft 登录地址或 identityToken 格式差异。本次增加明确应用错误、未知 403、非 JSON 403 和响应隐私的回归验证。真实个人账户的最终登录结果仍需在微软页面完成授权后验证。
