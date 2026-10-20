# WebSDK、CLI 与桌面壳

BuckyDesk 会同时碰到三个「buckyos」入口。它们版本应该对齐，但职责不同。

| 入口 | 位置 | 谁使用 |
| --- | --- | --- |
| npm 包 `buckyos` | `buckyos-websdk`，快照版本 0.7.133，MIT | App 开发者的浏览器/Node 代码，以及 `npx buckyos` |
| 系统 Tool | `$BUCKYOS_ROOT/bin/buckyos`，由安装器放入 | 已安装系统上的命令。内部用捆绑的 Deno 跑同一套 CLI 源码 |
| Rust API | buckyos 仓库 `src/kernel/buckyos-api` | 内核和 frame 服务 |

系统 Tool 和 `npx buckyos` 不互相调用、不互相覆盖、也不会自更新。全局 `npm install -g buckyos` 不是支持的安装方式。PATH 上的 `buckyos` 应指向系统安装器放进去的那个。

## SDK 的三种运行时

websdk `SDK.md` 按「这台机器上有没有可信的 cyfs-gateway」区分能力。

| 运行时 | 典型调用方 | 登录 | 访问系统服务 |
| --- | --- | --- | --- |
| `node` | AppService / AppClient | 用启动时的 session token 向 verify-hub 换双 token 并自己续期 | 通常是 `127.0.0.1:3180/kapi/<service>` |
| `browser` | 普通网页 | 当前窗口跳到 `sys.<zone>/sso/login`，成功后读 `getAccountInfo()` | 必须走 Zone 主机名和 HTTPS |
| `app_runtime` | 装在 BuckyOS Web 容器里的页面 | 能直接 `getAccountInfo()`。主动 logout 之后，再登录走钱包 SSO | 有本机网关时走 3180，否则走 Zone 主机名 |

桌面壳把激活页或系统页放进自己的 WebView 时，页面是否算 `app_runtime`，取决于壳有没有按 websdk 的约定注入运行时标记和 `window.BuckyApi`。普通系统浏览器打开 `http://127.0.0.1:3182` 就是网页路线。

SDK 的通用调用顺序是：初始化、登录、按服务名解析端点、发起 HTTP。端点解析依赖网关和 `service_info`，所以壳在 Zone 还没 boot 完成时调用业务 API 会失败。激活阶段只能使用激活服务或 `provision` 这种不依赖 Zone 的 API。

## CLI 里和桌面相关的命令

CLI 源码在 websdk `cli/`。`doc/modules/README.md` 写于 2026-09-17，并声明其中多数模块是目标契约，不能当成现行手册。下面只列已经注册、和「把系统跑起来」有关的部分。

| 模块 | 现状 | 对 BuckyDesk 的意义 |
| --- | --- | --- |
| `provision` | 已实现 `status` / `check` / `activate` | 本机首次身份。安装器可以调用同一套 Node/Deno API，而不必复制 `active_server.rs` 的写盘格式。 |
| `system` | `status` 一类读取存在；更新仍是占位 | 激活之后观察运行状态可以借用；升级事务不能交给这个命令。 |
| `pikg` | 本地 `init/build/pack/info/doctor` 不要求 Zone 已运行；`app install` 要求已接受当前身份的 Zone | 这是应用开发者工具。首版桌面不必把它暴露给终端用户。 |
| `app` | 安装计划、安装、状态 | 依赖已运行 Zone 和权限。属于开箱之后的应用管理。 |
| 用户、消息、文件、备份等 | 设计修订稿，完成度按服务/SDK/CLI/验收四层分别计算 | 桌面不要用 CLI 在本地编排来填补还没落地的服务。 |

`provision` 的提交顺序和文件名是安装器必须遵守的契约，摘录如下。`etc/node_identity.json` 最后写，它是 `node-daemon` 判断「已经激活」的开关。

- `security/ood1.<domain>/authentication.private.pem`
- `local/identity/ood1.<domain>/did.json`
- `local/identity/ood1.<domain>/device_doc.jwt`
- `local/identity/ood1.<domain>/device_mini_doc.jwt`
- `etc/.zone.json`
- `etc/zone_document.jwt`
- `etc/zone_dns_records.json`
- `etc/start_config.json`（含管理员密码哈希，不含私钥）
- `etc/node_gateway_params.json`
- `etc/node_identity.json`

密码编码是 `base64(SHA-256(password + ownerName + ".buckyos"))`，与网页激活的 `hashPassword` 无 nonce 分支一致。

系统发行物的固定布局：

```text
$BUCKYOS_ROOT/bin/buckyos
$BUCKYOS_ROOT/libexec/buckyos-tool/
  cli/  dist/  package.json  LICENSE  distribution.json  sbom.cdx.json
  runtime/deno[.exe]
```

构建系统在打 rootfs 时校验 tarball、manifest 和 Deno 摘要。安装器更新这一目录时应整包替换，保留回滚，而不是让 Tool 自己升级。

## BuckyOSApp

仓库：[buckyos/BuckyOSApp](https://github.com/buckyos/BuckyOSApp)。Tauri 2 + React + TypeScript，包名 `buckyosapp`，版本 0.7.0，依赖 npm `buckyos` ^0.7.120。打包脚本在同级目录执行 `pnpm run tauri build`，产物进入安装包的 `BuckyOSApp` 组件。

当前版本需求（以该仓库 `doc/当前版本需求文档-中文.md` 指向的现行文档和代码边界为准）把应用定义成两件事：

1. **身份宿主。** 创建或导入助记词 DID，绑定 SN 用户名，切换、备份、删除身份。创建顺序是先确认助记词，再注册 SN，成功后才把 DID 写入本地。导入要求 SN 上已经能按公钥查到用户名。
2. **Runtime 宿主。** 用 iframe 或独立 WebView 打开页面，并注入 `window.BuckyApi`。

已开放的宿主方法：

| 方法 | 作用 |
| --- | --- |
| `getPublicKey` | 当前激活 DID 的公钥 |
| `getCurrentUser` | DID、用户名、公钥、SN 用户名、OwnerDocument |
| `signJsonWithActiveDid` | 用户输入密码后对 JSON 对象数组签名，可附带 `pwd_hash` |
| `resolve_did` | 经宿主的 name-client 解析 DID 文档 |

`node_active` 的钱包路线只消费这组桥。桥的实现在 BuckyOSApp，不在 buckyos 仓库。buckyos 的激活说明特别写了：不要把这个桥理解成浏览器插件。

明确不做的事：

- 不安装、升级、卸载 Zone 内的 App，应用列表也不能点击启动。
- 不做 OOD 进程的启动、保活和激活完成后的整套本地配置写回。
- 不做依赖激活码、中继、二维码或账号云端发现的远程激活。

它已经做的设备发现有两条：按本机 IPv4 网段扫描，只保留返回了 `active_url` 的设备；或者让用户手动输入远程 OOD 根地址，请求 `{地址}/device`，默认端口 3182，再打开返回的激活页。已绑定 OOD 时可以展示状态，并用签名 token 调用 `zone.unbind_config` 解绑。桌面端用独立 WebView 打开这些页面。这些动作都假设另一端已经有处于激活模式的 `node-daemon`。

应用列表来自 `BUCKYOS_ROOT/bin/applist.json`（开发模式可回退内置列表）。同一套产品要求在 macOS、Windows、Android、iOS 上功能等价；移动端把外部页面开在当前 WebView 内，而不是独立窗口。

BuckyOSApp 仓库里的 `doc/目录结构.md` 还描述了 `$BUCKYOS_ROOT/local/bin`、`app-daemon` 和「不安装 buckyos-service」的原生应用设想。需求大纲把「这是不是本项目的正式需求」列成待确认问题。实现范围以当前 Tauri 需求为准：那些目录还不是现成的桌面运行时。

## 桌面壳和 Web 桌面怎么配合

这一节记录上游两个客户端的关系。BuckyDesk 不走这条路径，页面约定见 [非 UI 内核](非ui内核.md)。

一次完整的本机使用会经过三层 UI：

```text
BuckyDesk 窗口 / BuckyOSApp
  ├─ 尚未激活：本机向导，或打开 3182
  └─ 已激活：打开 control-panel 的 Web Desktop
       └─ Web Desktop 再打开文件、消息、AI Center、Jarvis
```

Web Desktop 的工程在 buckyos `src/frame/desktop`。它假定 Zone 已经能提供登录和 kapi。Mock 模式只用于界面开发。

BuckyDesk 如果复用 BuckyOSApp 的窗口，就同时接过它的 SN 优先身份流程。如果首版采用本地 `provision`，首次启动不应再强制走「助记词 → 邀请码 → SN 绑定 → 扫描局域网」。那种流程是「这台电脑去激活另一台设备，或给自己的 Zone 注册一个可公开的名字」。两种流程可以并存，但默认路径只能有一条。
