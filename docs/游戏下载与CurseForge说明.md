# 游戏安装与 CurseForge

下载中心提供 Minecraft 版本、整合包、模组、资源包和光影。内容项目可以从 Modrinth 或 CurseForge 搜索。选择项目后可查看介绍和历史文件。

## 安装 Minecraft

打开“下载中心 → 游戏版本”，搜索并选择 Minecraft 版本，填写实例名称和安装目录，然后选择原版、Fabric、Forge 或 NeoForge。Fabric API 可与 Fabric 一起安装；OptiFine 可独立安装，或搭配与之匹配的 Forge 版本。

确认配置后会先显示安装计划。再次确认后才开始下载。完成的实例会出现在游戏库中，模组、配置和存档与其他实例分开。

安装前需准备符合 Minecraft 版本要求的 64 位 Java。启动器不会自动下载 Java。实例名称重名时需要另选名称，原实例不会被覆盖。

Minecraft 文件从 Mojang 官方清单校验；加载器由各自发布方提供。Forge 与 NeoForge 安装器会在无界面进程中运行，并检查退出状态和安装产物。

OptiFine 安装文件通过 BMCLAPI 获取。若镜像重定向到其他 HTTPS 节点，启动器会核对 BMCLAPI 返回的 SHA-1。这个校验来自镜像，不是 OptiFine 作者的签名。OptiFine 本身不随启动器分发。

## 安装整合包

在“整合包”中选择项目和文件版本，设置新的实例名称并确认安装。支持 Modrinth `.mrpack` 和 CurseForge 标准 Minecraft 整合包清单。整合包会使用作者指定的游戏、加载器和依赖版本，不会自动升级其中的模组。

客户端必需文件会自动下载；服务端专用文件会跳过。可选择是否安装作者标记为可选的客户端文件。安装时先应用通用 overrides，再应用 client-overrides；server-overrides 不会写入客户端。

归档提取会检查路径穿越、Windows 非法路径、符号链接和重复文件。单个文件最大 512 MB，最多处理 10,000 项，下载和解压内容分别有容量上限。

若 CurseForge 作者不允许第三方下载，启动器会提示打开项目页。它不会尝试绕过该限制。未完成的安装不会加入游戏库。

## 配置 CurseForge

官方发行版在构建时配置 CurseForge API Key。自行构建时，可在“下载中心 → CurseForge → 接口设置”填写自己的 Key，或使用 `XUMOB_BUILD_CURSEFORGE_API_KEY` 构建环境变量。运行环境变量 `XUMOB_CURSEFORGE_API_KEY` 也可用于本机配置。

本机输入的 Key 使用当前 Windows 用户的 DPAPI 加密，存放在 `curseforge-key.bin`。界面和普通日志只显示配置状态，不返回 Key。启动器只向 CurseForge API 和其 ForgeCDN 文件域发送该 Key；启动的 Java 进程不会继承 Key 环境变量。

发行程序中的 API Key 仍可被技术手段提取。本机加密保护的是本地配置文件，不会让客户端内的密钥成为不可提取的秘密。需要集中控制访问时，应由维护者运行自己的服务端代理。

## 接口资料

[CurseForge API](https://docs.curseforge.com/rest-api/) · [Modrinth 整合包格式](https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack) · [Fabric Meta](https://github.com/FabricMC/fabric-meta) · [Forge Installer](https://github.com/MinecraftForge/Installer) · [BMCLAPI](https://bmclapidoc.bangbang93.com/)
