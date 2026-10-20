# BuckyOS 架构

本章只保留理解桌面运行环境所需要的模型。细节以 buckyos 仓库 `doc/arch/` 为准，尤其是 `01_overview.md` 到 `11_env_contract.md`。那套文档写明：和代码冲突时以代码为准。

当前源码目标是 **Beta2.2 / 0.7.0**。README 写的正式发布时间是 2026-10-15。这是一次破坏性版本：系统配置、身份、应用打包和 Agent 配置都变了，不保证兼容更早的 beta。

## Zone 是运行单位

BuckyOS 把用户自己的设备收成一个 **Zone**。Zone 是逻辑上的个人云，里面可以有多台设备。

| 概念 | 含义 |
| --- | --- |
| Zone | 用户拥有的逻辑集群。身份、用户、应用安装、权限都以 Zone 为边界。 |
| OOD | Zone 的核心节点，承载 `system-config` 等关键能力。可以是 1 台，设计上也允许 2n+1 台。 |
| Node | 可运行应用或系统服务的设备。桌面电脑在单机场景里同时是 Node 和唯一 OOD。 |
| ZoneGateway | Zone 对外的 HTTP/HTTPS 入口，通常由 OOD 承担。 |
| NodeGateway | 每台机器上的 `cyfs-gateway`，默认入口 `http://127.0.0.1:3180`。 |

桌面单机是这个模型的特例，不是另一套系统。本机仍要有 Owner、Zone 文档、设备文档和一次 boot。差别在于：只有一个 OOD，不跑 CYFS FUSE，外部访问可以先不做。

## 控制环

最短可用闭环是五步：

1. **激活。** 设备写下身份和启动参数。网页路径在 3182；离线路径由 `buckyos provision` 直接写文件。
2. **三个关键进程。** `cyfs-gateway`（网络）、`system-config`（状态，端口 3200）、`node-daemon`（本机执行）。
3. **首次 boot。** `boot/config` 尚不存在时，`node-daemon` 运行 `scheduler --boot`，根据 `BUCKYOS_ZONE_DOC` 写入初始 KV。
4. **调度循环。** scheduler 从 KV 推导本机该跑什么、服务怎么被找到、RBAC 和网关配置，再写回 KV。
5. **节点收敛。** `node-daemon` 只读本机的 `nodes/<node_id>/config`，安装、启动、停止、升级，并上报实际端口。

三个角色不要和常见的「一个主进程加插件」混在一起：

- `system-config` 是控制面数据库，不只是配置文件。用户、设备、服务、权限、调度结果都在里面。
- scheduler 是确定性推导器。它同时产出实例放置、`service_info`、RBAC 和网关暴露。
- `node-daemon` 是本机收敛器。它不临时向控制面询问「现在该做什么」，只把本机拉到 `node_config`。

客户端找服务看 `service_info`，不记死端口。实例从提示端口开始尝试绑定，成功后上报真实端口。家庭和桌面环境里端口经常被占用，这是故意的；调试时也必须先看 `service_info`。

## 代码怎么分层

`src/readme.md` 给出的依赖方向是：

`dapp → frame services → kernel services → kernel modules → components`

| 层 | 目录 | 桌面上的形态 |
| --- | --- | --- |
| Kernel | `src/kernel/` | 本机进程，Zone 起来之前就能工作。包括 `node_daemon`、`sys_config_service`、`verify_hub`、`scheduler`、`task_manager`、`kmsg`、`kevent`、`workflow`。 |
| Frame | `src/frame/` | 可缺失的系统功能。当前仓库里控制面板、repo、文件、消息、OpenDAN、AICC、HomeStation 都有实现，构建进 `bin/`。其中 App/Agent/HostScript 另走容器。 |
| Apps | `src/apps/` | 用户态应用。Jarvis 是版本化的 `.pikg`。一般在容器中运行，没有 root。 |
| Web Desktop | `src/frame/desktop/` | 控制面板的 React UI，安装到 `bin/control-panel/web/`。 |
| 激活向导 | `src/kernel/node_active/` | 唯一源码。`rootfs/bin/node-active/` 是打进镜像的构建产物。 |

`src/bucky_project.yaml` 里和桌面直接相关的安装映射包括：`node_daemon`、`system_config`、`verify_hub`、`scheduler`、`control_panel`、`repo_service`、`nfs_server`、`msg_center`、`opendan`、`aicc`、`homestation`、`desktop`（进 `bin/control-panel/web/`），以及系统自带的 `buckyos` CLI（`bin/buckyos` 和 `libexec/buckyos-tool/`）。

Beta2.2 README 点名的用户可见能力：Web 桌面与控制面板、FileBrowser、MessageHub、OpenDAN/Jarvis、AI Compute Center、Workflow 与 Task Manager、`.pikg` 应用安装。同时写明尚未完成的部分：Workflow 运行态仍主要在内存；scheduler 里的 Function Instance 替换未完成；外部消息通道目前实现了 Telegram。

## 网络入口

`cyfs-gateway` 是独立仓库，打包时作为 `buckyos` 组件的依赖装进来。桌面上最重要的是本机入口：

```text
浏览器 / CLI / SDK
  → http://127.0.0.1:3180/kapi/<service>
    → NodeGateway
      → 本机服务，或经 rtcp 到别的节点
```

浏览器里的 Web 应用则走 Zone 主机名：`https://<app-host>/kapi/<service>`。这条路径依赖 ZoneGateway 和登录 cookie。单机桌面如果只在本机使用，可以优先走 3180；一旦页面被放到普通浏览器标签里，就会碰到网关和 HTTPS。

SN（在 `cyfs-gateway` 里）负责账号、设备在线信息和可选的流量中转。BNS 负责可公开解析的身份文档，写路径是链上交易，不是登录 token。桌面若只服务本机，这两条都不必出现在第一次启动里。

## 身份与权限

`verify-hub` 签发和校验 session token。RBAC 策略在 `system-config` 的 `system/rbac/*`，传播有缓存，架构文档记录最长约 10 秒。

访问控制看四元组：`appid`（来自 HTTP Host）、`userid`、`resource`（来自 path）、`OP`（来自 HTTP method）。改权限后立刻刷新页面，可能仍看到旧结果。

App 进程的身份不从包名猜。`node-daemon` 必须注入 `BUCKYOS_APP_DID`、`BUCKYOS_APP_ID`、`BUCKYOS_APP_INSTANCE_ID`、`BUCKYOS_OWNER_USER_ID`、`BUCKYOS_DATA_DIR`、`BUCKYOS_APP_TOKEN`。SDK 启动时会检查它们彼此一致。

## 两套「层」叠在了一起

源码目录表达的是进程和权限。另外还有一组客户端，用 Web 或原生窗口去调用这些进程。两边都使用「应用」「桌面」「Web」，所以会看成同一套分层。

### 进程分层：谁可以缺

依赖方向是 `dapp → frame → kernel`。下层不依赖上层。

| 层 | 缺了以后 | 对开箱运行 |
| --- | --- | --- |
| Kernel + `cyfs-gateway` | Zone 无法 boot，也没有本机入口 | 必需。至少是 `node-daemon`、`system-config`、scheduler、`verify-hub`、NodeGateway |
| Frame 服务 | 少一项系统能力。`src/readme.md` 把 frame 定义成可缺失 | 按首屏需要挑选。boot 模板现在会写入 `control-panel` 和 `repo-service` 的 spec，所以默认安装会拉起它们；这是当前发行物的选择，不是控制环本身的前提 |
| App（`.pikg`，含 Jarvis） | Zone 仍在，只是没有那个用户工作负载 | 可后装。要验收「能对话」时再预装 Jarvis，并因此带上容器运行时 |

目录和启动重要性也不重合。`control-panel` 的源码在 `src/frame/`，boot 路径却把它写成初始服务。看目录会以为它可选，看启动会以为它是内核。

### 客户端：都在调用 Zone，互相不是上下层

| 客户端 | 实际位置 | 开箱时 |
| --- | --- | --- |
| Web 桌面 | `src/frame/desktop` 的静态页，装进 `bin/control-panel/web/`，由 `control-panel` 提供 | 当前唯一完整的人机界面。它是控制面板的前端，不是独立的一层操作系统 |
| 激活向导 | `src/kernel/node_active` 的静态页，嵌在 `node-daemon` 里，端口 3182 | 只服务「尚未写身份」的那段时间。本地 `provision` 写完身份后可以不打开它 |
| `buckyos` CLI / WebSDK | 系统镜像里的 `bin/buckyos`，源码在 websdk | 运维和 App 开发的调用面。浏览器里的 Web 桌面通过 SDK 访问 kapi。BuckyDesk 不必再内嵌一份 SDK |
| BuckyOSApp | 另一个仓库的 Tauri 程序，安装包里的可选组件 | 单机开箱可以不要。它管的是壳内助记词，以及去打开另一台设备的激活页 |

Web 在这里是实现技术，出现了三次：内核里的激活页、frame 里的系统桌面、Tauri 壳里的 WebView。三次都是页面，不是三个 Web 层。

BuckyOSApp 和 Web 桌面是并列的两个客户端。安装器把它们收成「Desktop App」和「Desktop Service」，名字都带 Desktop，于是像是上下层。服务负责把 Zone 跑起来；App 负责在 Zone 之外保管一把钥匙并打开页面。只安装 App 时，本机没有 Zone。

## 用户打开桌面时实际看到的

系统桌面是控制面板里的 Web 应用，不是操作系统的窗口管理器，也不是 BuckyOSApp 的主界面。

- 开发时：`src/frame/desktop` 用 Vite，端口 5174，默认 Mock（`VITE_CP_USE_MOCK=true`）。Mock 能看界面，不能证明身份或服务链路。
- 运行时：页面由已启动的 `control-panel` 提供。真实 Zone 要关掉 Mock，并让页面能访问网关。
- BuckyOSApp 当前的主界面是 DID、SN 绑定，以及发现并打开另一台设备的激活页。应用列表只展示，点击不启动。它是身份宿主，不是这套 Web 桌面。

所以桌面产品至少有两个 UI 层：一个负责本机身份和窗口，一个是 Zone 起来之后的系统桌面。BuckyDesk 需要决定用户安装后第一眼看到哪一层，以及什么时候从身份层进入系统桌面。
