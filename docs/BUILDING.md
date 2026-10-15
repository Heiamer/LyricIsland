# 构建歌词岛 1.1

当前构建入口为 `build/native.ps1`，输出 Windows x64 的主程序和 GPUI 设置程序。

## 环境

- Windows x64、Git、Rust stable 的 `x86_64-pc-windows-gnu` 工具链。
- LLVM 的 `llvm-dlltool.exe`。
- Windows SDK 的 `fxc.exe`。
- C 编译器驱动；现有脚本使用调用 `zig cc` 的包装脚本。
- 打包时需要 Python 3、`cryptography`、Windows SDK 的打包与签名工具，以及 SQLite 原生库。

## 字体资源

源码已包含完整 Noto Sans SC 与 Noto Sans JP 变量字体，无需额外下载或安装到系统。请保留 `Assets/Fonts/` 与对应许可文件，打包时会校验完整字体的 SHA256。两套字体均使用 SIL Open Font License 1.1，见 `FONT-NOTICE.md`。

## 编译

在仓库根目录执行，替换下面的工具路径为本机实际路径：

```powershell
./build/native.ps1 `
  -DllTool 'C:/Tools/LLVM/bin/llvm-dlltool.exe' `
  -FxcPath 'C:/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/fxc.exe' `
  -ZigCc 'C:/Tools/zig-cc.cmd'
```

脚本先运行主程序与设置的单元测试，再构建 release 版本。依赖已缓存时可加 `-Offline`。输出位于 `Rust/target-native/release/`。

运行时需要完整 `Assets/Fonts/`、设置程序及 SQLite 原生库，不能只复制主程序。完整发行流程见 `build/package_native.py`；通知身份包由 `build/notification_identity.py` 创建。首次打包会需要生成自己的开发签名，并可能需要为脚本指定本机 SDK 路径。仓库不提供任何开发者私钥，构建不会自动安装证书或申请通知访问。

仓库只保留当前使用的代码与测试，构建入口为 `build/native.ps1`。
