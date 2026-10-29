# 点屿 M3E 弹簧系统

全部界面过渡采用共享解析弹簧。Spatial 用于位置、尺寸、形状，Effects 用于颜色、透明度且阻尼比为 1。默认 Expressive，也支持 Standard；两种方案各有 Fast、Default、Slow 三档。

参数来源为 AndroidX Material3 的 ExpressiveMotionTokens / StandardMotionTokens v0_14_0（Apache-2.0）：https://github.com/androidx/androidx/tree/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens

物理求解由本项目实现，保留打断时的位置和速度，按真实经过时间解析采样；停止后精确落在目标并停止请求动画帧。减少动态效果时位置直接就位。计时、音乐播放进度和拖动数值使用真实数据；循环中的音符、声波和旋转使用相位，状态切换包络使用弹簧。
