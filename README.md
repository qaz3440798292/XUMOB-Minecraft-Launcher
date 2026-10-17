# XUMOB Minecraft Launcher 0.7.0

XUMOB 是一款 Windows Minecraft 启动器，使用 Tauri 和 Vue 构建。它可以管理多个游戏目录与版本、启动本地实例、安装 Minecraft 和加载器，并从 Modrinth 或 CurseForge 浏览与安装内容。

## 使用启动器

下载并解压 Windows 便携版，运行 `XUMOB Minecraft Launcher.exe`。请保留随包提供的 `msal-broker` 文件夹和 `WebView2Loader.dll`。首次打开时添加游戏目录；以后选择的目录、版本和启动设置会自动保存。

玩家可以使用离线用户名，也可以通过 Windows 系统账户窗口或设备代码登录微软账户。Minecraft 下载页可创建独立实例，选择原版、Fabric、Forge 或 NeoForge，并按需安装 OptiFine 或 Fabric API。下载中心提供游戏版本、整合包、模组、资源包和光影。

微软登录和 CurseForge 接口配置方法见[正版登录说明](docs/正版登录说明.md)和[游戏下载与 CurseForge 说明](docs/游戏下载与CurseForge说明.md)。Modrinth 的内容筛选和安装步骤见[下载中心说明](docs/Modrinth下载说明.md)。

## 运行要求

- Windows 10 或 11，64 位。
- Windows WebView2 运行时。缺少时请安装[微软 Evergreen Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)。
- 安装并启动 Minecraft 需要符合游戏版本要求的 64 位 Java。启动器目前不下载 Java。

模组、资源包、光影和整合包来自对应内容平台。程序不附带 Minecraft、Java 或内容文件。

## 开发

首次参与开发可先读[源码导读](docs/源码导读.md)和[贡献指南](CONTRIBUTING.md)。

```powershell
npm ci
npm run desktop:dev
```

构建便携版需要安装 Rust、.NET 8 SDK、Visual Studio C++ Build Tools 和 WebView2 SDK，详见 [Tauri 的 Windows 环境要求](https://v2.tauri.app/start/prerequisites/#windows)。.NET SDK 用于构建随启动器发布的 MSAL 登录组件；玩家无需另行安装 .NET 运行时。

```powershell
npm run desktop:build
```

构建输出保存在项目目录：前端文件位于 `dist`，启动器及其 MSAL 登录组件位于 `src-tauri/target`。请在分发时把 `XUMOB Minecraft Launcher.exe`、完整的 `msal-broker` 文件夹与 `WebView2Loader.dll` 一并保留。桌面开发和正式构建命令都会把 Rust 输出固定到这个目录；正式构建前会删除旧的启动器程序文件，依赖编译缓存会保留。

格式检查：

```powershell
npm run format:check
npm run format:rust:check
```

核心测试：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked --release --lib --features custom-protocol
```

使用 GNU 工具链构建单文件 WebView2 程序时，需要自备兼容的静态库和 CRT shim，并将目录设置到 `XUMOB_WEBVIEW2_STATIC_DIR`。普通构建使用 Tauri 依赖提供的默认链接方式。

## 源码

前端位于 `src`，Tauri 与 Rust 代码位于 `src-tauri`。账户、游戏下载和平台接口说明见 `docs`。开发用核心探针不会打入便携版。

## 许可证

本项目使用 GPL-3.0，详见 [LICENSE](LICENSE)。

## 第三方资源

CurseForge 文件地址为空时补全官方 CDN 地址、切换 CDN 节点的处理思路，参考了龙腾猫跃的 [PCL 官方源码 ResourceVersion.vb](https://github.com/Meloong-Git/PCL/blob/main/Plain%20Craft%20Launcher%202/Modules/Resource/ResourceVersion.vb) 中相关的小段实现。本启动器独立实现下载与校验流程。

游戏库里的加载器 Logo（Fabric、MinecraftForge、NeoForge、OptiFine、Quilt）为各自权利人的商标，
来源与许可见 [public/loaders/ATTRIBUTION.md](public/loaders/ATTRIBUTION.md)。
