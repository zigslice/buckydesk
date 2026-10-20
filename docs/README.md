# BuckyDesk 调研文档

这份文档回答一个问题：要在 Windows 和 macOS 上提供 BuckyOS 的开箱即用环境，BuckyDesk 该理解上游的哪些事实，以及该自己拥有哪一段产品。

快照日期：2026-10-20。依据是当天 `main` 上的公开仓库，不是设计稿优先于代码。上游文档和代码冲突时，以代码为准；本文在冲突处会标出来源。

| 文档 | 用途 |
| --- | --- |
| [非 UI 内核](非ui内核.md) | 现行决定。BuckyDesk 只依赖无界面内核，界面用 gd5 在 `127.0.0.1:1016` 自己做。 |
| [命令体系](命令体系.md) | 第一版的命令壳。命令行和 1016 页面共用一棵树。 |
| [01-结论](01-结论.md) | 调研结论。界面方案以《非 UI 内核》为准。 |
| [02-buckyos-架构](02-buckyos-架构.md) | Zone 和控制环。进程分层和 Web/桌面客户端是两套东西，以及开箱时谁必需。 |
| [03-运行安装与激活](03-运行安装与激活.md) | 目录、端口、容器、三条激活路径、现有安装包。 |
| [04-websdk-与桌面壳](04-websdk-与桌面壳.md) | TypeScript SDK、`buckyos` CLI、BuckyOSApp 宿主桥。 |
| [05-buckydesk-边界](05-buckydesk-边界.md) | 建议 BuckyDesk 拥有的子系统，以及还没在上游定下来的选择。 |

相关上游仓库：

| 仓库 | 在桌面链路里的角色 |
| --- | --- |
| [buckyos/buckyos](https://github.com/buckyos/buckyos) | Zone 运行时。当前版本 Beta2.2 / 0.7.0。 |
| [buckyos/buckyos-websdk](https://github.com/buckyos/buckyos-websdk) | npm 包 `buckyos`（快照版本 0.7.133）：浏览器/Node SDK 与 CLI。 |
| [buckyos/cyfs-gateway](https://github.com/buckyos/cyfs-gateway) | 网络内核。NodeGateway、ZoneGateway、SN/BNS 客户端都在这里，不在 buckyos 仓库内。 |
| [buckyos/BuckyOSApp](https://github.com/buckyos/BuckyOSApp) | Tauri 身份宿主。注入 `window.BuckyApi`，打开设备激活页。版本 0.7.0。 |
| [buckyos/buckyos-pack-installer](https://github.com/buckyos/buckyos-pack-installer) | macOS pkg、Windows NSIS、deb/rpm 的本地打包脚本。buckyos 以 `src/publish` 子模块引用它。 |
| [buckyos/buckyos-devkit](https://github.com/buckyos/buckyos-devkit) | 开发者构建与安装工具。终端用户不应接触。 |
