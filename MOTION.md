# 动效与材质 · 0.7.7

按用户要求，动画是主要体验：默认启用，只由应用的“界面动画”开关控制，不跟随 Windows 动画设置。旧版本升级恢复动画默认值；用户之后关闭动画的选择仍正常保存。

胶囊与展开卡片共用一个 RectangleGeometry 表面，中心与顶边保持固定。弹簧进度连续控制左右边界、宽度、高度和圆角；不再保留小胶囊并从下面揭开第二个卡片。
内容使用两个独立层交叉淡化，紧凑层退场与展开层进场由几何进度映射，无等待锁；两层之间的表面仍可以点击反向操作。文字交接使用轻微模糊，表面边缘保持清晰。

IslandTransition 的解析弹簧保留当前位置及单位/秒速度。默认 stiffness=420、damping=30、mass=.8，带柔和回弹；展开响应参数 360ms，收起 300ms，实际收敛由弹簧决定。连续反向操作不会从目标值重新开始。
所有几何与内容帧使用 CompositionTarget.Rendering；原生宿主保持固定布局。动画结束解除渲染回调、移除模糊；无播放时活动条停止，无循环后台动画。

材质仍只有 Default；旧系统材质迁移为 Default。关闭系统透明效果时实色，高对比度使用系统颜色。系统透明偏好不影响弹簧和播放动效。
原配置的 Motion.Gap 字段仅保留协议兼容，合并表面没有卡片间隔，设置页不再展示该参数。

```powershell
dotnet test LyricIsland.Tests -c Release
LyricIsland.exe --smoke-test native-smoke.json
LyricIsland.exe --animation-preview preview
LyricIsland.exe --render-preview preview
```

Windows 动画关闭的环境下仍已实际验证胶囊形变和播放指示条。数值与原生测试共同覆盖位置、速度接续与固定宿主。

默认固定位置的可见表面贴住工作区上沿；透明阴影预留区位于上沿之外，不再占据可见空白。自由悬浮保留原位置约束。
