# Modrinth 下载中心

打开“下载中心”，选择模组、资源包或光影。项目卡片显示平台图标；点击项目可查看介绍、作者发布的图片和链接，以及可用的文件版本。

## 搜索和选择版本

可按 Minecraft 版本、加载器和类别筛选，也可按相关度、下载量或更新时间排序。“一键匹配当前实例”会把当前游戏版本和加载器带入筛选。需要浏览测试内容时，可以开启快照与预览版本。

项目介绍页提供文件历史、游戏版本和加载器筛选。先在游戏库选择目标实例，再选兼容的文件版本。没有选中实例或版本不兼容时，安装按钮会说明原因并保持禁用。

## 安装

1. 选择目标实例。
2. 在项目页选游戏版本、加载器和文件版本。
3. 点击“下载此版本”，查看目标目录和必要前置。
4. 点击“确认下载并安装”。

必要前置按 Modrinth 版本清单递归处理。文件会校验 SHA-512 和大小，校验完成后再安装。同一文件会跳过；已有不同内容、同一项目已有其他版本或依赖不兼容时，安装会停止。取消或失败会清理临时文件。游戏运行时不能安装模组。

模组放入实例的 `mods`，资源包放入 `resourcepacks`，光影放入 `shaderpacks`。模组需要 Fabric、Forge、NeoForge 或 Quilt 实例；资源包可用于原版。使用光影需要相应实例已安装 Iris 或 OptiFine。

单个文件最大 512 MB；一次安装最多 2 GB、64 个项目。项目说明中的手动配置要求仍需按作者指引完成。

启动器通过 Modrinth 官方 API 和 CDN 下载，不需要 Modrinth 账户或 API Key。接口资料：[API](https://docs.modrinth.com/api/) · [搜索筛选](https://docs.modrinth.com/api/operations/searchprojects/) · [项目版本](https://docs.modrinth.com/api/operations/getprojectversions/)。

## 数据位置

游戏目录、选中的版本和设置保存在 `%LOCALAPPDATA%\XUMOB Minecraft Launcher\data`。换启动器目录后设置仍可沿用。账户凭据使用当前 Windows 用户的加密存储；不要分享或提交个人 `data` 目录。
