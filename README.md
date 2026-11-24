# buckydesk

在 Windows 和 macOS 上提供 BuckyOS 的开箱即用运行环境。

BuckyDesk 只依赖 BuckyOS 的无界面内核。界面用 [gd5](https://github.com/zigslice/gd5) 写，由本机服务提供，浏览器打开 `http://127.0.0.1:1016`。

约定写在 [docs/非ui内核.md](docs/非ui内核.md)。命令行和页面共用的命令壳见 [docs/命令体系.md](docs/命令体系.md)。内核版本如何钉住并长期跟随，写在 [docs/跟随构建.md](docs/跟随构建.md)。其余调研笔记在 [docs/](docs/README.md)。

## 快速开始

启动本地页面：

```bash
cargo run -p buckydesk -- server
```

浏览器会自动打开 `http://127.0.0.1:1016`。

## 命令

```bash
# 查看版本和 BUCKYOS_ROOT 路径
buckydesk version

# 查看内核状态
buckydesk kernel status

# 激活本机身份（首次使用）
buckydesk identity activate --name "我的名字" --password "my-password"

# 启动内核
buckydesk kernel start

# 停止内核
buckydesk kernel stop

# 查看产品配置
buckydesk product info
```

## 产品模式

BuckyDesk 默认使用 Lite 模式：
- 无容器依赖
- 单 Agent 运行限制
- 支持静态网页应用
- 预置脚本应用原生运行

## 目录结构

- `src/backend/` - Rust 命令壳和 HTTP 服务
- `src/frontend/` - gd5 前端页面
- `docs/` - 设计文档

## 环境变量

- `BUCKYOS_ROOT` - BuckyOS 运行根目录
  - macOS 默认: `/opt/buckyos`
  - Windows 默认: `%APPDATA%\buckyos`
  - Linux 默认: `/opt/buckyos`
- `BUCKYDESK_FRONTEND` - 前端页面目录
- `BUCKYDESK_GD5` - gd5 源码目录
