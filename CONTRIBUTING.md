# 贡献代码

项目使用 Vue 3、Tauri 2 和 Rust。目录说明见[源码导读](docs/源码导读.md)。

## 准备环境

Windows 开发需要 Node.js 22.12 或更新版本、Rust stable、.NET 8 SDK、Visual Studio C++ Build Tools 和 WebView2。使用 MSVC 工具链时，请在 Visual Studio Installer 中安装“使用 C++ 的桌面开发”与 Windows SDK。

```powershell
npm ci
npm run desktop:dev
```

只改页面时可用 `npm run dev` 预览。登录、文件读写和下载需要启动 Tauri 后端。

运行 `npm run desktop:build` 会把前端输出写入 `dist`，并在 `src-tauri/target/release` 生成启动器及自包含的 `msal-broker` 目录。分发时必须将完整目录与启动器、`WebView2Loader.dll` 一并保留。构建脚本固定使用项目内的 Rust 目录，并在正式构建前清理旧的启动器程序文件；Rust 依赖缓存会保留。

Windows 构建需要 Visual Studio C++ Build Tools 提供的 `link.exe`，或一套完整的 GNU Rust、MinGW 和 WebView2 静态链接文件。构建脚本会自动识别 PATH 中符合项目目录结构的本地 GNU 工具链；其他机器可以安装 Visual Studio C++ Build Tools，或配置 `XUMOB_BUILD_RUNTIME` 和 `XUMOB_WEBVIEW2_STATIC_DIR`。

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

正版登录应用 Client ID 与 CurseForge API Key 由发行者在构建时注入。可在本机创建根目录 `.env.release`，写入 `XUMOB_BUILD_MICROSOFT_CLIENT_ID=...` 和 `XUMOB_BUILD_CURSEFORGE_API_KEY=...`；构建脚本会将其传给 Rust 编译器，界面不会提供接口配置入口。`.env.release` 已被 Git 忽略，禁止提交到公开仓库。GitHub Actions 发布构建应使用仓库 Secrets 设置同名环境变量。公开客户端标识和嵌入式 API Key 都能从发布程序中提取，密钥只用于限制源码仓库泄露，不应视为服务端秘密。项目使用 GPL-3.0，详见 [LICENSE](LICENSE)。
