# 贡献代码

项目使用 Vue 3、Tauri 2 和 Rust。目录说明见[源码导读](docs/源码导读.md)。

## 准备环境

Windows 开发需要 Node.js 22.12 或更新版本、Rust stable、Visual Studio C++ Build Tools 和 WebView2。使用 MSVC 工具链时，请在 Visual Studio Installer 中安装“使用 C++ 的桌面开发”与 Windows SDK。

```powershell
npm ci
npm run desktop:dev
```

只改页面时可用 `npm run dev` 预览。登录、文件读写和下载需要启动 Tauri 后端。

GNU 单文件构建是可选配置。它需要 WebView2 静态库、匹配的 CRT shim，以及兼容的 WebView2 绑定；通过 `XUMOB_WEBVIEW2_STATIC_DIR` 指定库和 shim 所在目录。没有这些文件时，使用默认链接方式构建。

## 修改代码

前端文件使用仓库中的 Prettier 配置，Rust 使用 rustfmt。请运行 `npm run format` 和 `npm run format:rust` 格式化，再运行对应的 `format:check` 命令检查。

让名称说明数据含义，独立状态分别声明。页面负责显示和事件绑定，跨页面或较长的状态流程适合放入组合函数。对安全边界、兼容性和并发行为作简短注释即可。

下载和安装改动应保留预览、兼容性检查、取消清理与已有文件保护。未经用户确认，不要把真实凭据、个人游戏目录或账户数据放进提交。

## 检查修改

```powershell
npm run format:check
npm run format:rust:check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked --release --lib --features custom-protocol
```

改动需要在线服务时，除自动化检查外，请记录实际核对过的平台与游戏版本。提交说明写清行为变化和验证结果。

微软 Client ID 与 CurseForge API Key 通过本地环境或启动器设置配置。请勿提交凭据、登录令牌、`.env`、个人 `data` 目录或游戏文件。项目使用 GPL-3.0，详见 [LICENSE](LICENSE)。
