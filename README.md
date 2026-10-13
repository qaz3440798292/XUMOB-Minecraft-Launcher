# XUMOB Minecraft Launcher · Tauri + Vue 版 v0.5.0

保留 v0.2.0 的界面、游戏库与启动功能，桌面外壳改为 Tauri 2，界面改为 Vue 3。文件读取、Java 检测、版本解析、校验下载、原生库解压和游戏进程管理均在 Rust 中执行。发行版无需 Node.js，也没有 Node 常驻服务。

窗口失去焦点或空闲 15 秒后，通过官方 WebView2 `MemoryUsageTargetLevel` 接口降低缓存占用；用户操作时恢复正常模式，脚本及日志保持运行。继续使用原图形渲染，未采用会改变背景像素的软件渲染或低质量模式。接口行为见 [微软官方说明](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_19)。内存优化为尽力操作，效果取决于运行时和机器；旧运行时若不支持此接口，会正常继续运行。

v0.4.2 将品牌、窗口标题、Windows 文件属性及程序文件名统一为 **XUMOB Minecraft Launcher**，游戏启动信息也使用新名称。

## 使用

解压 Windows 便携版，双击 `XUMOB Minecraft Launcher.exe`。请将 `WebView2Loader.dll` 与 EXE 保存在同一文件夹。支持 Windows 10 / 11 x64；界面使用系统的 WebView2 运行时。若机器缺少运行时，请从 [微软官方页面](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) 安装 Evergreen Runtime。

首次使用点击“添加游戏目录”，选择已安装的原版或整合包目录，再选择游戏版本。支持添加多个目录，按目录和版本分别保存 Java、内存、目录模式、窗口尺寸及附加启动参数。不会绑定特定整合包。

离线用户名、Java 自动/手动选择、环境检查、可信基础依赖补全、模组搜索、实时中文日志、停止确认以及窗口操作均保留。启动流程支持标准版本继承、现代及旧版参数、原版/Fabric/Quilt/Forge/NeoForge/OptiFine/LiteLoader 元数据、长参数和旧版资源映射。

## Modrinth 下载中心

左侧“下载中心”支持搜索 Modrinth 的模组、资源包和光影，按相关程度、下载量或更新时间排序并翻页。选择游戏实例后，自动按 Minecraft 版本及 Fabric / Forge / NeoForge / Quilt 加载器筛选兼容版本。没有选择实例时可浏览，安装按钮会保持禁用。

选择项目与版本，点击“查看安装清单”，检查将安装的文件、必要前置和目标目录，再点击“确认下载并安装”。必要前置按 Modrinth 版本元数据递归补全；可选和内嵌依赖不会重复下载。资源包保存到实际游戏目录下的 `resourcepacks`，模组到 `mods`，光影到 `shaderpacks`。光影启用需要实例已有 Iris 或 OptiFine；资源包与光影在游戏中选择启用。

下载使用 Modrinth 官方 API 与 CDN，无需在启动器登录 Modrinth。流式下载后校验 SHA-512 和文件大小，全部文件校验完成后提交；取消或失败会清理临时文件，并撤回本次尚未完成的提交。游戏运行及其他准备任务期间禁止安装，下载期间不能切换实例或启动游戏。

已有相同文件会跳过；发现同名不同内容、同一项目的不同版本、声明的不兼容依赖或实例在预览后改变时，会停止并提示。不会自动替换已有整合包中的旧模组，请先通过模组文件夹处理冲突再安装。只能自动补全 Modrinth 已声明且可解析的必要前置；项目有额外要求时请查看“在 Modrinth 查看项目”。单文件最大 512 MB，每次安装总量最大 2 GB，最多解析 64 个项目。

接口实现参考 [Modrinth 官方文档](https://docs.modrinth.com/api/)。

## 微软正版登录

点击右上角账户或首页“修改”，选择“微软正版”，再点击“登录微软账户”。复制界面中的短代码，点击“打开微软登录页”，在系统浏览器中完成微软登录并输入代码。授权完成后，启动器自动读取 Minecraft Java 版角色并切换为正版模式。

正版启动使用服务返回的角色名、UUID、访问令牌和 Xbox 身份，离线用户名单独保存。登录过期时自动刷新，并在启动前验证 Java 版授权及角色资料；失败会提示重新登录，不会自动改用离线身份。可以切换回离线账户，或退出并清除本启动器保存的登录。

默认 Client ID 为本次提供的 `530c101a-f756-496e-8099-6addb6ed06ed`。授权页显示的应用名称由该 ID 的微软注册信息决定。在账户窗口“登录应用配置”中可更改 ID；配置变更会移除旧登录。应用需支持微软个人账户、启用公共客户端流程并获准访问 Minecraft 服务。无需 Client Secret。

v0.4.1 修正登录错误判断：不再把所有 Minecraft 登录 HTTP 403 都归为应用许可问题。仅当服务明确返回 `Invalid app registration` 时提示核对应用注册与访问许可；其他拒绝显示“无法确定”及诊断类型。错误文字可选中复制，只包含状态、固定错误分类和有效 UUID 格式的请求编号，不包含服务原文或令牌。

成功获取微软登录代码并不证明此 Client ID 已获得 Minecraft 访问许可。若明确出现 `Invalid app registration`，请由该应用的所有者核对注册信息并通过 [Minecraft 应用审核入口](https://aka.ms/mce-reviewappid) 申请或确认 Java 服务权限。未获得许可时，修改客户端无法代替服务端审核。

认证流程使用 [微软设备授权协议](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)，随后验证 Xbox Live、XSTS 和 Minecraft。用户密码始终只输入微软页面。

`data/account.json` 保存账户类型和公开角色资料；`data/account-session.bin` 使用当前 Windows 用户的 DPAPI 加密保存刷新令牌和 Minecraft 凭据。复制到其他 Windows 用户或机器后需重新登录。访问令牌不会通过界面接口返回，也不会写入长参数文件；Java 游戏进程仍按标准 Minecraft 协议通过进程参数接收令牌。请不要分享自己的 data 文件夹。

本次已真实验证 Client ID 可以申请设备授权代码。完整认证链、刷新、取消及正版参数已由模拟响应回归测试覆盖；未代替用户登录个人账户，真实角色登录及正版服务器连接需要用户完成微软授权后验证。

## 迁移设置

关闭旧启动器，把旧版程序旁的整个 `data` 文件夹复制到新版程序旁，保留原目录列表、离线用户名和各版本设置。游戏文件仍使用你添加的原路径，无需再次复制整合包。不要覆盖新版中已有且需要保留的设置；迁移前可备份两个 data 文件夹。

运行中日志、配置和缓存写入选定游戏目录。结束游戏前先保存进度，停止按钮仍有确认提示。

## 开发

Windows 常规构建需要 Node.js 22.12 或更新版本、Rust stable MSVC 工具链、Visual Studio C++ Build Tools 与 WebView2。安装方式见 [Tauri 官方环境说明](https://v2.tauri.app/start/prerequisites/#windows)。

```powershell
npm ci
npm run desktop:dev
```

发布构建与核心测试：

```powershell
npm run desktop:build
cargo test --manifest-path src-tauri/Cargo.toml --locked --release --lib --features custom-protocol
```

Tauri CLI 发布构建会自动开启 `custom-protocol` 并嵌入 `dist`。直接使用 Cargo 发布时，应先 `npm run build`，再 `cargo build --manifest-path src-tauri/Cargo.toml --locked --release --features custom-protocol --bin blockfield-launcher`。直接 Cargo 开发不带该 feature，使用本地 Vite 开发地址。

- `src/App.vue`：保留原 HTML 结构、图标、文案与布局。
- `src/useApp.js`：Vue 状态、控件和有上限的批量日志更新。
- `src/useLauncher.js`：有限的 Tauri 原生接口。
- `src-tauri/src/core.rs`：启动核心。
- `src-tauri/src/auth.rs`：微软设备授权、Xbox/Minecraft 认证、刷新及 Windows 加密凭据。
- `src/DownloadCenter.vue`：Modrinth 搜索、版本选择、安装清单与下载进度。
- `src-tauri/src/modrinth.rs`：Modrinth 元数据、前置解析、冲突检查和校验安装。
- `src-tauri/src/main.rs`：窗口、对话框和后台命令。
- `src-tauri/src/tests.rs`：核心回归测试。
- `core-probe`：开发用核心测试工具，便携版不附带。

设置、日志与 WebView2 缓存默认位于程序旁的 `data`；测试可通过 `BLOCKFIELD_DATA_ROOT` 指定独立目录。只有主动设置 `BLOCKFIELD_DEBUG_PORT` 才开启本地测试调试端口，正常启动不会开启。

## 使用范围

启动符合标准版本配置的多个已安装游戏实例，支持微软正版登录及 Modrinth 模组、资源包、光影下载。未新增在线安装 Minecraft 或加载器、CurseForge/Modrinth 整合包清单安装、MultiMC 专用清单转换或加载器安装器处理器。程序不附带 Minecraft、Java 或整合包文件。
