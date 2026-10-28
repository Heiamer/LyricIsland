# 字体与许可

西文与数字使用 **Google Sans Flex**，中文界面使用 **Noto Sans SC**，日文行使用 **Noto Sans JP**。字体随软件和源码提供，无需安装到 Windows。Google Sans Flex 不含中日韩字形，中文与日文自动回退到 Noto 字体——与 Pixel / Android 的渲染栈一致。

三套字体均采用 **SIL Open Font License 1.1**，不适用项目代码的 MIT 许可。再分发时请保留版权及许可全文。

| 字体 | 官方来源 | 许可全文 |
| --- | --- | --- |
| Google Sans Flex | [Google Fonts](https://fonts.google.com/specimen/Google+Sans+Flex)（2025 年 11 月起以 SIL OFL 发布） | [GoogleSansFlex-OFL.txt](licenses/GoogleSansFlex-OFL.txt) |
| Noto Sans SC | [Google Fonts](https://github.com/google/fonts/tree/main/ofl/notosanssc) | [NotoSansSC-OFL.txt](licenses/NotoSansSC-OFL.txt) |
| Noto Sans JP | [Google Fonts](https://github.com/google/fonts/tree/main/ofl/notosansjp) | [NotoSansJP-OFL.txt](licenses/NotoSansJP-OFL.txt) |

字体文件位于 `Assets/Fonts/`。西文、数字与符号使用 Google Sans Flex（Regular / Medium / SemiBold / Bold 四个静态实例），中文使用 Noto Sans SC，包含假名的日文行统一使用 Noto Sans JP，避免一行内混用不同风格的中日文字形。韩文采用可用的系统字体。
