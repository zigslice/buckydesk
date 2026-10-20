# buckydesk

在 Windows 和 macOS 上提供 BuckyOS 的开箱即用运行环境。

BuckyDesk 只依赖 BuckyOS 的无界面内核。界面用 [gd5](https://github.com/zigslice/gd5) 写，由本机服务提供，浏览器打开 `http://127.0.0.1:1016`。

约定写在 [docs/非ui内核.md](docs/非ui内核.md)。浏览器里的工作区见 [docs/用户场景.md](docs/用户场景.md)。程序的命令壳见 [docs/命令体系.md](docs/命令体系.md)。内核版本如何钉住并长期跟随，写在 [docs/跟随构建.md](docs/跟随构建.md)。其余调研笔记在 [docs/](docs/README.md)。

本地打开页面：

```
cargo run -p buckydesk -- server
```

浏览器进入 `http://127.0.0.1:1016`，看到的是工作区。这一版的数据记在浏览器里。内核拉起和身份写入还是空命令，返回「尚未实现」。
