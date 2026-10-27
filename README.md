# buckydesk

在 Windows 和 macOS 上提供 BuckyOS 的开箱即用运行环境。

BuckyDesk 只依赖 BuckyOS 的无界面内核。界面用 [gd5](https://github.com/zigslice/gd5) 写，由本机服务提供。桌面自己打开 `http://127.0.0.1:1016`。没有公网地址的桌面向外连到云主机，手机用这台桌面的名字打开云主机，看到的仍是这台桌面。

约定写在 [docs/非ui内核.md](docs/非ui内核.md)。浏览器里的工作区见 [docs/用户场景.md](docs/用户场景.md)。程序的命令壳见 [docs/命令体系.md](docs/命令体系.md)。内核版本如何钉住并长期跟随，写在 [docs/跟随构建.md](docs/跟随构建.md)。其余调研笔记在 [docs/](docs/README.md)。

下面是一次实际跑通的命令序列。桌面和云主机各用一份状态目录，入口用 `127.0.0.1:18080`，桌面页面用 `127.0.0.1:18016`。Linux 上 80 和 1016 需要额外权限，绑不上时就用这两个高端口。换成两台机器时，云主机听 `0.0.0.0:80`，桌面听 `127.0.0.1:1016`，命令的其余部分一样。打印出来的那几句中文就是这次跑出来的。

这一版从源码编出 `bucky`。还不下载内核包，不写 `node_identity.json`，不启动 `node-daemon`。入口是 HTTP。域名记录由人写，程序不去改域名服务。

两台机器都先编出同一个程序：

```
git clone https://github.com/zigslice/buckydesk.git
cd buckydesk
git submodule update --init src/frontend/vendor/gd5
cargo build -p buckydesk
```

下面的 `bucky` 指 `target/debug/bucky`。状态写在这台机器的 `~/.buckydesk`。

## 桌面

在没有公网地址的那台电脑上。

```
bucky desk identity status
```

```
尚未写入本机身份。
```

写下名字、密码和钥匙文件夹。密码至少 8 位。钥匙文件夹只记下路径。

```
bucky desk identity activate --name 林 --password 12345678 --key-folder ~/bucky-key
bucky desk identity finish
bucky desk kernel status
```

```
正在准备你的工作区。
工作区已就绪。
内核还没有在这台电脑上运行。
```

打开本机页面。这个进程停在这里，继续提供 `http://127.0.0.1:1016`。

```
bucky desk open
```

```
http://127.0.0.1:1016
```

这次核对用的是 `bucky desk open --bind 127.0.0.1:18016 --no-open`，打印 `http://127.0.0.1:18016`。浏览器打开这个地址，看到的是这台电脑的工作区。

## 云主机

在有公网地址的那台机器上。它只做入口，上面不必再建一套工作区。

```
bucky cloud status
```

```
还没有桌面接到这个入口。
```

开始听。这个进程停在这里。

```
bucky cloud serve
```

```
http://0.0.0.0:80
桌面从这里向外连。
```

80 绑不上时：

```
bucky cloud serve --bind 0.0.0.0:8080
```

这次核对用的是 `--bind 127.0.0.1:18080`，打印：

```
http://127.0.0.1:18080
桌面从这里向外连。
```

用浏览器直接打开云主机的 IP，页面是：

```
这是云入口。从桌面的名字打开。
```

## 组网

假定云主机的公网地址是 `203.0.113.10`，这台桌面叫 `home.example`。在域名服务里把 `home.example` 的 A 记录写成 `203.0.113.10`。入口如果听在 8080，手机稍后打开的是 `http://home.example:8080/`，记录仍然指到这台云主机。

回到桌面，记下入口和名字。这次核对里的入口是 `http://127.0.0.1:18080`。

```
bucky desk entrance use --at http://203.0.113.10 --name home.example
bucky desk entrance status
```

```
这台桌面叫 home.example。它向外连 http://203.0.113.10。还要从这台桌面向外连。
这台桌面叫 home.example。它向外连 http://203.0.113.10。
```

再开一个进程，向外连上，并把访问转回本机页面。这个进程停在这里。

```
bucky desk entrance start
```

```
这台桌面叫 home.example。它向外连 http://203.0.113.10。
```

本机页面不在 1016 上时，加上 `--desk 127.0.0.1:18016`。

回到云主机再看一次：

```
bucky cloud status
```

```
1 台桌面接到了这个入口。1 台正连着。
```

到这一步，三处都还开着：桌面上的 `desk open`，桌面上的 `desk entrance start`，云主机上的 `cloud serve`。

## 手机打开

手机浏览器打开 `http://home.example/`。Host 是这台桌面的名字，云主机把这一次访问交给正连着的桌面，桌面再转到本机的 `127.0.0.1:1016`。页面标题是 BuckyDesk，脚本从 `/app/main.js` 和 `/gd5/` 一起过来。

工作区已经就绪时，手机上先看到登录。输入在桌面上设的密码，进入这台桌面的文件、消息和工作。这次核对里，登录的结果是「已进入工作区。」进入之后是「工作区已打开。」

没有这次登录就继续往里走，页面回到登录，摘要是「请重新进入。」

名字还没指到云主机时，可以在云主机上先看一句，确认桌面已经接上。把 `18080` 换成实际端口：

```
curl -s -H 'Host: home.example' http://127.0.0.1:18080/
```

正文里有 BuckyDesk。换成别的名字，正文是「这个地址不是接到入口的桌面。」

`entrance start` 停了，手机上是「这台桌面还没有连上。」`desk open` 停了，手机上是「这台桌面没有打开。」

自己有固定公网地址的桌面，只记下名字、不填 `--at`。状态是「这台桌面叫 home.example。公网入口在它自己身上。」它不经过这台云主机。这一版还不在那台桌面上对外听。
