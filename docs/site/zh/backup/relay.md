# 自建中继

本页介绍如何运行 Lockra 中继，让设备通过你掌控的服务器同步。

<StatusTag status="available" /> 自 0.8.1 版起提供。

中继为同步空间保存加密文件，并在某台设备写入时通知正在等待的其他设备（[经 Lockra 中继同步](/zh/backup/sync#经-lockra-中继同步)）。中继无法打开这些文件：加密它们的密钥从不离开设备。自建中继后，除你之外没有人能看到设备何时、从哪里同步，同步空间也不依赖 Lockra 的服务器。

## 准备工作

- 一台设备能够访问的 Linux 服务器，x64 或 ARM64 均可，小型虚拟机即可满足需要。
- 一个域名，以及中继前端的 HTTPS：Lockra 只通过 `https://` 连接中继。中继本身使用普通 HTTP，因此需要由前端的反向代理或负载均衡器处理 HTTPS。下文的 Caddy 可以自动申请免费证书。

## 使用 Docker 和 Caddy

仓库中的 `deploy/relay/compose.yaml` 会构建中继，并在它前面运行 Caddy；Caddy 会从 Let's Encrypt 为你的域名申请证书。

1. 把域名解析到服务器，并开放 80 和 443 端口。
2. 在服务器上，从仓库根目录启动两者：

   ```sh
   git clone --depth 1 https://github.com/sunerpy/lockra && cd lockra
   RELAY_HOST=relay.example.com docker compose -f deploy/relay/compose.yaml up -d
   curl https://relay.example.com/healthz   # ok
   ```

同步空间保存在 Docker 卷 `relay-data` 中。

## 使用发布包

每个版本都提供 Linux 版中继：`lockra-relay_<版本>_linux_x64.tar.gz` 和 `lockra-relay_<版本>_linux_arm64.tar.gz`，均列在该版本的 `SHA256SUMS` 中。每个包都包含不依赖其他库的程序，以及它的 systemd 服务。

1. 下载与服务器对应的包，校验后安装程序：

   ```sh
   # 最新版本的版本号：取自 GitHub 把 /releases/latest 跳转到的地址
   latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' https://github.com/sunerpy/lockra/releases/latest)
   version=${latest##*/v} arch=x64
   base="https://github.com/sunerpy/lockra/releases/download/v$version"
   curl -fL -O "$base/lockra-relay_${version}_linux_${arch}.tar.gz" -O "$base/SHA256SUMS"
   sha256sum --check --ignore-missing SHA256SUMS
   tar -xzf "lockra-relay_${version}_linux_${arch}.tar.gz"
   sudo install -m 0755 lockra-relay /usr/local/bin/lockra-relay
   ```

2. 安装并启动服务。服务以无特权用户运行中继，把同步空间保存在 `/var/lib/lockra-relay`，并在 `127.0.0.1:8090` 上等待同一台服务器上的代理：

   ```sh
   sudo cp lockra-relay.service /etc/systemd/system/
   sudo systemctl enable --now lockra-relay
   ```

3. 在中继前端配置 HTTPS。在同一台服务器上使用 Caddy 时，下面的 `Caddyfile` 会申请证书并转发请求：

   ```text
   relay.example.com {
   	reverse_proxy 127.0.0.1:8090 {
   		header_up X-Forwarded-For {remote_host}
   	}
   }
   ```

安装了 `gh` 时，还可以用 `gh attestation verify <安装包> --repo sunerpy/lockra` 确认安装包由发布流程从 Lockra 的源代码构建。

## 在负载均衡器之后

由云服务商的负载均衡器处理 HTTPS 时，请让中继监听网络地址，并指明负载均衡器的地址，使请求限制按每台设备的地址计数，而不是按负载均衡器的地址：

```sh
lockra-relay --bind 0.0.0.0:8090 --trust-proxy 10.0.0.0/16 --data /var/lib/lockra-relay
```

8090 端口只允许负载均衡器访问，健康检查使用 `/healthz`。在服务中，这些设置对应 `LOCKRA_RELAY_BIND` 和 `LOCKRA_RELAY_TRUST_PROXY` 两行。

## 连接你的设备

在第一台设备上，打开「设置 › 同步」，选择「开始同步」，再选择「Lockra 中继」，并在「中继地址」中填写中继的地址，例如 `https://relay.example.com`。邀请码和配对链接中包含这个地址，因此其他设备扫描二维码或粘贴配对链接即可加入。已经在其他存储上同步的同步空间，可以在每台设备上通过「修改存储设置」迁移到你的中继。

## 设置

默认设置适合家庭或小型团队：每台设备的文件最大 4 MiB，每个同步空间最多 64 台设备、32 MiB，总计 4 GiB，每个网络地址每分钟 120 次请求，设备最后一次访问同步空间 400 天后删除该空间。`lockra-relay --help` 会列出全部设置，每项设置也可以用环境变量指定；[中继的文档](https://github.com/sunerpy/lockra/blob/main/docs/relay.md)（英文）逐项说明了它们。

### 关闭新空间注册

只想让已经使用的同步空间访问中继时，请同时设置 `LOCKRA_RELAY_REGISTRATION=closed` 和
`LOCKRA_RELAY_ALLOWED_SPACES`。后者的每一项依次是同步空间目录的 UUID、`=` 和该目录
`access` 文件里的小写 SHA-256；多项以英文逗号分隔：

```sh
space=123e4567-e89b-42d3-a456-426614174000
access=$(sudo cat "/var/lib/lockra-relay/lockra-relay-v1/spaces/$space/access")
export LOCKRA_RELAY_REGISTRATION=closed
export LOCKRA_RELAY_ALLOWED_SPACES="$space=$access"
```

其他同步空间的请求都会被拒绝。未列出的目录仍留在磁盘上，但不会对外提供；允许的空间即使目录丢失，
也只有能提供匹配令牌的原设备可以重新建立。允许列表缺失、格式错误，或允许目录中的指纹不一致时，
中继会在开始监听前退出，避免误服务或覆盖文件。这里限制的是同步空间，不是其中的单台设备：同一空间
中的设备仍共用一个访问令牌。

### 用注册码登记新的同步空间

<StatusTag status="available" /> 自 0.15.0 版起提供。

只想让你同意的同步空间使用中继，又不想每次都复制文件、重启中继时，请设置 `LOCKRA_RELAY_REGISTRATION=code` 并重启一次中继。此后新的同步空间都需要注册码，注册码在服务器上生成：

```sh
sudo lockra-relay new-code --data /var/lib/lockra-relay --note "Anna 的手机"
# 使用 Docker 时：docker compose -f deploy/relay/compose.yaml exec relay lockra-relay new-code --note "Anna 的手机"
```

命令只显示一次注册码，例如 `ABCD-EFGH-IJKL-MNOP-QRST-UVWX`。把它发给要建立同步空间的人，由对方在第一台设备上填入中继地址下方的「注册码（可选）」（[经 Lockra 中继同步](/zh/backup/sync#经-lockra-中继同步)）。一个注册码只能登记一个同步空间，7 天后失效（用 `--expires 12h` 或 `--expires 30d` 修改，最长 365 天）。加入这个同步空间的设备不需要注册码。

| 命令                         | 作用                                                                     |
| ---------------------------- | ------------------------------------------------------------------------ |
| `lockra-relay codes`         | 列出尚未使用的注册码。                                                   |
| `lockra-relay drop-code ID`  | 作废一个注册码，ID 见 `codes` 的输出。                                   |
| `lockra-relay spaces`        | 列出同步空间和它们最后一次使用的时间，以及磁盘上中继没有提供的同步空间。 |
| `lockra-relay admit 空间ID`  | 不用注册码，直接提供磁盘上的某个同步空间，例如以前的同步空间。           |
| `lockra-relay revoke 空间ID` | 停止提供某个同步空间，它的文件仍留在磁盘上。                             |

每个命令都和 `new-code` 一样使用 `--data`（使用 Docker 时，通过 `docker compose … exec relay` 运行）。命令通过中继数据文件夹中的一个套接字与运行中的中继通信，只有中继的用户和 root 能打开它，不经过网络。中继只保存注册码的哈希：拿到数据文件夹也得不到注册码。原来开放注册的中继改用注册码后，已有的同步空间在你 `admit` 之前不再提供；`LOCKRA_RELAY_ALLOWED_SPACES` 中列出的同步空间不需要注册码，照常提供。

## 中继保存的内容

- **只有加密的文件。** 每个同步空间一个文件夹，其中是每台设备的加密文件，以及同步空间中设备用来证明身份的那个值的指纹。账号、设备名称、恢复密钥和主密码都不会到达中继。
- **日志不含地址。** 中继记录启动和停止、每小时一次的同步空间数、字节数和请求数统计，以及磁盘故障；不记录任何网络地址、同步空间或请求。代理记录哪些内容，取决于你对它的配置。
- **备份与升级。** 备份数据文件夹后，即使磁盘损坏，同步空间也能继续使用；其中的文件与设备加密时一样无法读取。没有备份时，设备仍保留所有账号，并会在下次同步时重新写入同步空间。升级时，安装新的程序并重启服务即可。
