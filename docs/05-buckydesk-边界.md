# BuckyDesk 的边界

BuckyDesk 的目标：用户在 Windows 或 macOS 上安装一个应用，安装结束时本机已经是一台可登录的单 OOD Zone，窗口里是 BuckyOS 的系统桌面。用户不需要安装 Rust、Deno、Docker 文档或 SN 账号。

BuckyOS 继续拥有 Zone 的语义。BuckyDesk 拥有「这台个人电脑如何把 Zone 跑起来」。

## 分界

| 属于 BuckyOS / 上游 | 属于 BuckyDesk |
| --- | --- |
| `system-config`、scheduler、`node-daemon`、frame 服务、Web Desktop、`.pikg` | 安装根、安装事务、开机与崩溃监管、日志与诊断入口 |
| 激活文件格式、`provision` 写盘协议、网页激活 RPC | 选择默认激活路径，保证与 3182 互斥，激活进程退出后重新拉起 |
| AppLoader、OCI 镜像、WSLC/Docker 适配 | 把选定的容器运行时安装到可用，并把同一用户、同一权限的运行时交给 `node-daemon` |
| `cyfs-gateway` 的路由、rtcp、SN/BNS | 是否在首启打开公网端口；本机防火墙例外只覆盖真正需要的端口 |
| RBAC、verify-hub、应用权限 | 第一个管理员密码的收集和保存方式 |
| BuckyOSApp 的 DID 库和 `BuckyApi` | 默认安装不包含这个壳。需要壳内签名或去激活别的设备时再作为附加组件 |
| 服务升级的 pkg ready 语义 | 桌面安装包的下载、签名、覆盖安装和回滚 |

不要在 BuckyDesk 里重写一份调度器或一份控制面板。窗口里应加载上游构建好的 `bin/control-panel/web/`。身份文件应通过 `provision` API 或 `node-daemon` 的激活服务写入，避免桌面端私自拼 JWT。

## 建议的运行结构

```text
用户安装 BuckyDesk
  → 创建专用 BUCKYOS_ROOT（与源码 start.py 的默认根分开）
  → 解包已签名的 rootfs + cyfs-gateway
  → 若预装了容器应用，再准备容器运行时
  → 本机首次激活（默认：单 OOD、无 SN）
  → 把 Owner 私钥备份到用户指定的位置
  → 启动并监管 node-daemon
  → 等待 boot/config 与本机网关就绪
  → 打开 Web Desktop
```

监管进程是产品的一部分。它需要知道三种退出：

- 激活成功后的主动退出：身份文件已出现，应不带 `--enable_active` 重启。
- 崩溃或被系统杀掉：按退避策略重启，并保留日志。
- 用户选择退出：停止整组进程，包括名称以 `buckyos-app-` 开头的容器。`stop.py` 会留下命名卷，桌面的「退出」和「重置数据」要分成两个动作。

开机启动用操作系统的常规机制（launchd、Windows 服务或计划任务）。实现时要和上游安装脚本已有的 hook 对齐，避免两个保活源同时拉起 `node-daemon`。

## 运行根

首版建议使用安装器独占的目录，并通过环境变量传给所有子进程。

- macOS 可以继续用 `/opt/buckyos`，但要处理权限，并在检测到该目录已由源码开发占用时停止安装，而不是覆盖。
- Windows 不要同时使用 `%APPDATA%\buckyos` 和 `C:\BuckyOS`。选定其一后，写进服务环境和卸载程序。
- 开发开关是独立的 `BUCKYOS_ROOT`。文档和安装器都应告诉开发者：桌面产品和 `uv run src/start.py` 不能指向同一根。

数据策略沿用上游：

- 覆盖升级替换 `bin/` 和 Tool 分发，不删除 `data/home`、`data/srv`、`storage`。
- 卸载默认保留上述三类，并在界面里写明路径。
- 「重置应用」可以以后再做。上游软重置的执行细节还是待定，桌面不要先发明一套不兼容的清理规则。

和宿主文档目录的链接是增强项。首版即使用户文件只住在 `data/home` 里，也应能完成文件应用的验收。Windows 符号链接失败不应阻断安装。

## 首次启动

默认体验建议是：

1. 询问显示名、管理员密码，以及 Owner 私钥备份目录。
2. 使用本地 `provision` 语义生成单 OOD 身份。域名由产品生成一个稳定的本机名字，不要求用户拥有 DNS。
3. 密码规则与上游一致：至少 8 位，哈希为 `base64(SHA-256(password + ownerName + ".buckyos"))`。
4. 激活期间不要启动带 `--enable_active` 的 `node-daemon`。
5. 写盘完成后启动服务，轮询本机 3180 和控制面板，直到可打开桌面或明确失败。
6. 失败时展示 `provision status` 的状态（`not_installed`、`unactivated`、`partial`、`configured`、`invalid`）和 `node-daemon` 日志位置。`partial` 不能靠再点一次激活覆盖，上游会拒绝。

在把「本机专用域名」做成默认值之前，要用当前 `node-daemon` 验证：只存在 `etc/.zone.json`、没有公网 DNS 时，`scheduler --boot` 能写入 `boot/config`，Web 桌面能完成本机登录。provision 文档支持这个方向，但是启动侧的读取路径要以代码为准。

SN、自有域名、局域网里激活另一台设备，放在设置或「高级」里。它们复用网页向导和 BuckyOSApp，不作为安装后的第一屏。

Owner 私钥只出现在用户选定的备份位置。不要把网页向导里「Review 页明文展示私钥」搬进默认流程。

## 容器

首版验收包含至少一项容器工作负载。建议就是预装的 Jarvis 或等价的 Agent 应用能被拉起。

- macOS：安装流程检测 Docker，并给出可执行的安装步骤。检测要用与 `node-daemon` 相同的用户。远程 Docker 引擎不算本机可用。
- Windows：优先检测 WSLC，其次 Docker。把 `doc/windows_wslc.md` 的前置条件收进安装器：Windows 11、虚拟化、`wsl --install --no-distribution`、`wslc run hello-world` 这一级的检查。不要把这篇文档交给用户。
- 选定之后固定 `BUCKYOS_CONTAINER_RUNTIME`，并保证镜像导入、`node-daemon` 和停止动作使用同一 Windows 用户和同一权限级别。WSLC 的镜像对会话和权限敏感，交互用户拉的镜像在 LocalSystem 服务里可能看不见。

若检测失败，安装可以完成，但主界面应停留在「运行环境未就绪」，而不是打开一个缺少 Agent 的桌面并假装成功。

## 窗口

默认窗口只加载 `bin/control-panel/web/`。这个页面已经是系统的人机界面，BuckyDesk 负责把它打开，并负责窗口下面的进程是否活着。

BuckyOSApp 留在默认安装之外。它解决的是另一件事：钥匙放在壳里、以及发现别的设备。把监管做进那个壳，会把「本机 Zone」和「去激活远程设备」合成一个首启流程。

薄窗口至少要有：启动中、本机激活进度、桌面、服务未就绪、查看日志、退出。托盘可以后加。

## 端口与网络

首版本机模式只保证 loopback 上的 3180 以及服务实际监听的本机端口。80、443、2980 不作为安装成功的条件。

3182 只在用户主动选择网页激活时打开，完成后关闭。不要让一台已经 provision 过的机器再对外提供激活页。

检测到端口占用时，指出占用进程并停止安装或停止启动。不要静默换一组端口，因为网关、SDK 和大量服务配置都以这些端口为契约。

## 升级

消费上游的发布产物：按平台的 rootfs、`cyfs-gateway`、固定版本的 `buckyos` Tool tarball 和 Deno。升级前停止监管，替换 `bin/` 与 `libexec/buckyos-tool/`，保留数据目录，再启动。

不要在桌面端从 git 构建，也不要在用户机器上执行 `devenv.py`。Tool 没有自更新通道，桌面更新程序是它唯一的替换者。

跨 beta 的数据兼容由 BuckyOS 版本说明约束。Beta2.2 已经声明不保证兼容更早 beta。安装器在大版本升级前应要求备份 `data/`、`storage/` 和 Owner 密钥，而不是假设覆盖安装总是安全。

## 建议的验收

在一台干净的 Windows 和一台干净的 macOS 上，使用新的系统用户：

1. 安装包不要求预先安装 Rust、Node、pnpm、Deno 或源码仓库。
2. 安装结束时存在独立的 `BUCKYOS_ROOT`，且没有改写开发者已有的另一套根。
3. 用户只输入显示名、密码和备份位置，即可到达登录后的 Web 桌面。
4. 重启电脑后，无需再执行命令，桌面仍能打开。
5. 预装的容器应用（Jarvis 或当时的等价物）能启动并完成一次对话或等价的健康检查。
6. 覆盖安装后，`data/home` 里预先写入的文件还在。
7. 卸载后进程和容器停止，用户数据目录按界面承诺保留。
8. 激活中途失败再打开应用，不会生成第二套身份；界面能说明目录处于 `partial` 以及下一步。

上游已经承认 Windows 上「安装 + 激活 + Jarvis」尚未端到端验证。这条验收就是 BuckyDesk 相对上游桌面包的增量。

## 尚未替产品做的决定

这些决定会改变首版范围，调研阶段先记下，不在文档里假装已经选定。

1. **默认身份。** 本地 `did:web` 名字，还是仍然要求 SN 用户名。前者才能做到不联网安装；后者才能立刻使用 `*.web3.buckyos.ai` 和 BuckyOSApp 的现有创建流程。
2. **窗口形态。** 系统浏览器打开本机控制面板，还是 BuckyDesk 自带一个只加载该页面的窗口。两种都不包含 BuckyOSApp。
3. **Windows 服务账户。** 当前用户进程能看见用户自己的 WSLC/Docker，但开机时用户未登录则服务不可用。LocalSystem 能开机启动，却可能看不见用户会话里的容器镜像。首版可以规定「登录后启动」。
4. **容器运行时的分发方式。** 检测并引导安装官方 Docker/WSL，或由 BuckyDesk 捆绑一个固定版本。捆绑要单独处理许可和体积。
5. **与上游 Desktop 安装包的关系。** 现有 NSIS/pkg 已经会安装 BuckyOSApp 和服务。BuckyDesk 可以取代这条桌面分发，或成为它的首启与监管层。两条线同时给用户安装，会抢端口和运行根。
6. **多用户。** 上游允许一个 Zone 里有多个用户，但桌面电脑通常映射到操作系统的一个登录用户。首版应按「操作系统用户 = Zone 的第一个管理员」处理，不在安装器里做一套平行的多用户向导。
