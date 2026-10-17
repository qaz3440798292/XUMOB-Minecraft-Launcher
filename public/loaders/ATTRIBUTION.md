# 加载器 Logo 来源与许可

本目录的图标只用于「游戏库」页面区分客户端的加载器类型（原版用草方块），全部是随包分发的本地资源；
启动器运行时**不联网**加载任何图标。识别逻辑见 `src/loaderIcons.js`。

各 Logo 与其名称的商标、著作权归各自权利人所有。这里按「标识对应加载器」的用途引用，
不代表与这些项目之间存在隶属、赞助或背书关系。

| 文件           | 标识           | 来源                                                                                              | 许可与改动说明                                                                                                                                                                                                 |
| -------------- | -------------- | ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `vanilla.svg`  | 原版 Minecraft | 项目自绘矢量（无外部素材）                                                                        | 原版没有加载器 Logo，这里用自绘的像素风草方块表示。Minecraft 是 Mojang Studios / Microsoft 的商标，本文件是原创绘制、不含任何官方素材。                                                                        |
| `fabric.png`   | Fabric         | <https://fabricmc.net/assets/favicon.png>（官方站点 64×64 方形标识）                              | FabricMC 项目按 Apache-2.0 发布（<https://github.com/FabricMC/fabric-loader>）。Logo 为 FabricMC 商标。原图未改动，只把调色板 PNG 转为 RGBA。                                                                  |
| `forge.svg`    | MinecraftForge | <https://files.minecraftforge.net/static/images/safari-pinned-tab.svg>（官方站点 mask-icon 矢量） | MinecraftForge 项目按 LGPL-2.1 发布（<https://github.com/MinecraftForge/MinecraftForge>）。Logo 为 MinecraftForge 商标。只把填充色改为 `#eef2e9` 并按内容边界收紧 `viewBox`。                                  |
| `neoforge.png` | NeoForge       | <https://neoforged.net/img/content/branding/banner.png>（官方 branding 横幅里的狐狸标识）         | NeoForged 项目按 LGPL-2.1 发布（<https://github.com/neoforged>）。Logo 为 NeoForged 商标。用边界洪水填充去掉横幅的灰色底纹，保留原始配色与像素细节。                                                           |
| `optifine.png` | OptiFine       | <https://optifine.net/images/of16r.png>（官方站点 16×16 标识，标识本身就是「OF」字标）            | OptiFine 是 sp614x 的闭源项目（<https://optifine.net/>），未声明可再分发许可。此处按标识用途引用官方 16×16 字标，仅把深红底色改为透明。如需完全规避风险，删除本文件即可，OptiFine 客户端会自动回退到方块图标。 |
| `quilt.svg`    | Quilt          | <https://quiltmc.org/assets/img/logo.svg>（官方站点矢量标识）                                     | QuiltMC 项目按 Apache-2.0 发布（<https://github.com/QuiltMC/quilt-loader>）。Logo 为 QuiltMC 商标。只按内容边界收紧 `viewBox`。                                                                                |

## 更新图标

替换同名文件即可，前端按 `src/loaderIcons.js` 里的路径读取，无需改代码。
把新来源补进上表；如果来源不许再分发，请不要放进本目录。
