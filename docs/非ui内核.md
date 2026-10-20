# 非 UI 内核

BuckyDesk 只依赖 BuckyOS 的无界面内核。界面全部自己做：本机起一个 HTTP 服务，用 [gd5](https://github.com/zigslice/gd5) 写页面。`buckydesk server` 监听成功后，用浏览器打开 `http://127.0.0.1:1016`。

上游的 Web 桌面、激活向导页面、控制面板前端、BuckyOSApp、websdk 的浏览器界面，都不再使用。

## 依赖的进程

这些进程没有自己的产品页面。BuckyDesk 负责安装、拉起和保活，页面通过本机 HTTP API 调用它们。

| 进程 | 仓库 | 作用 | 本机端口 |
| --- | --- | --- | --- |
| `cyfs-gateway` | `buckyos/cyfs-gateway` | 网络内核。NodeGateway 把 `/kapi/<service>` 转到对应服务 | 3180 |
| `node-daemon` | `buckyos` `src/kernel/node_daemon` | 读 `node_config`，在本机安装、启动、停止服务 | 3181、3183（kevent，仅本机） |
| `system-config` | `src/kernel/sys_config_service` | Zone 的状态库 | 3200 |
| `verify-hub` | `src/kernel/verify_hub` | 登录和 session token | 3300 |
| `task-manager` | `src/kernel/task_manager` | 任务 | 3380 |
| `scheduler` | `src/kernel/scheduler` | 从状态库推导本机该跑什么 | 3400 |
| `kmsg` | `src/kernel/kmsg` | 消息队列 | 4030 |
| `workflow` | `src/kernel/workflow` | 工作流服务 | 4070 |

调用一律走 `http://127.0.0.1:3180/kapi/<service>`。3180 只给本机。

`node-daemon` 二进制里带有激活向导的静态页（源码在 `src/kernel/node_active`，端口 3182）。那是界面，不是内核。BuckyDesk 不打开这个页面。身份用文件协议写进 `$BUCKYOS_ROOT/etc`（与 websdk `provision` 落盘的那组文件相同），然后启动 `node-daemon`。

`kevent`、`klog`、`buckyos-api` 是这些进程内部的库，不是单独的产品。

## 自己重做的部分

下面这些都不进入 BuckyDesk 的依赖。需要相应能力时，在 1016 的页面里重新做，底层只调用上一节的 API。

| 上游东西 | 所在位置 |
| --- | --- |
| 系统桌面和控制面板页面 | `src/frame/desktop`，装到 `bin/control-panel/web/` |
| 控制面板服务 | `src/frame/control_panel`。它的页面是上游 UI 的宿主 |
| 激活向导页面 | `src/kernel/node_active`，由 3182 提供 |
| 桌面壳 | 仓库 `BuckyOSApp` |
| 浏览器 SDK 和各应用页面 | `buckyos-websdk`，以及 `src/frame/`、`src/apps/` 里的界面 |
| Frame 服务和 `.pikg` 应用 | `repo-service`、`nfs-server`、`msg-center`、`opendan`、`aicc`、`homestation`、`aiworkspace`、`smb-service`、Jarvis |

上游默认 boot 仍可能拉起 `control-panel`。那是它自己的页面进程。BuckyDesk 不把浏览器指到那里。

## 本机界面

`buckydesk server` 只绑定 `127.0.0.1:1016`，提供静态 HTML。页面用 gd5 直接写，不经过打包器：

- 引擎：`src/engine/`（`Component`、`ComponentRender`）
- 组件：`src/ui/`（Button、Input、Modal 等）
- 浏览器以 ESM 从同一个源加载这些文件，所以必须由这个 HTTP 服务提供，不能用 `file://`

服务开始监听之后，打开系统浏览器访问 `http://127.0.0.1:1016`。1016 是 BuckyDesk 自己的端口，不替代 3180，也不占用 BuckyOS 的系统端口。

页面只谈本机内核：读 `system-config`、向 `verify-hub` 登录、看 `node-daemon` 是否活着。文件、消息、Agent 这些产品功能以后仍写在这个站点里，不嵌上游页面。

这些二进制不在本仓库编译。版本钉在上游已经发布的载荷上，做法见 [跟随构建](跟随构建.md)。
