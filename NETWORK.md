# 直连网络策略 · 0.7.8

按用户要求，所有由本应用发起的网络请求只走直连。没有代理回退，没有端口配置。不会改 Windows 代理、DNS、注册表或环境变量；VPN / TUN 的操作系统路由不由应用控制。

## 实现

- TaskbarLyrics.Core.DirectNetwork 为封面和网易云歌曲信息创建 HttpClientHandler，显式设置 UseProxy=false、Proxy=null。
- Lyricify.Lyrics.Helper 0.2.0 中，ClearProxy 创建普通 HttpClient，仍继承 Windows / 环境代理；DisableProxy 明确将 handler.UseProxy 设为 false。已通过已安装版本的 IL 检查确认。
- LyricifyTask 初始化和网络异常重试均调用 DisableProxy，因此网易云、QQ、酷狗、LrcLib 歌词及歌曲时长不会在重试时回到系统代理。
- 本次不改歌词匹配规则、缓存和播放器识别。

## 验证

```powershell
LyricIsland.exe --network-test direct-network.json
```

测试在一个独立进程中将 HttpClient.DefaultProxy 设成不可连接的 127.0.0.1:1，再访问固定公开样本的网易云歌曲信息、实际 TrackArtworkResolver 封面下载和 NeteaseLyricSource 歌词。所有请求使用生产客户端；退出前恢复该进程内属性，不修改 Windows 配置，不读取或控制当前播放。

该测试证明三条网易云请求路径不会依赖默认代理；不能保证所有歌曲、地区或其他供应商接口一直可用。其他供应商共用同一个明确禁用代理的歌词客户端。
