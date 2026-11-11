# LUCKLUCK SLOT — RG353PS native 640×480 · v2026.11.11

## 简体中文

这是基于上游 LUCKLUCK SLOT `v2026.11.11`（源码提交 `25e39ec7131ac05fc5187181c4de77d75bedad7c`）的 RG353PS / RK3566 原生 640×480 移植版。正式中文和英文包由同一源码标签分别构建，完整包含前端及固定版本的 RG353PS AArch64/musl 模拟器核心。

### 上游 v2026.11.11 更新

- **游戏首字母搜索：** 在游戏货架按 `SELECT + Y` 打开四层键盘，支持拼音首字母和游戏名原文片段实时筛选。方向键移动、`A` 输入、`B` 退格、`L1/R1` 换层；`X` 恢复全部、`START` 应用、`MENU` 取消。只作用于游戏货架。
- **修复 B 键短按误触发 Wi-Fi 直连：** 松开 B 时清除长按状态；长按 B 的 Wi-Fi 直连保留。
- **修复键盘大小写显示：** Wi-Fi 密码键帽按原始字符绘制，保留小写和大写字形。
- 保留十机种支持（GBA、GB、GBC、FC、SFC、MD、PS1、PCE、街机、NEOGEO）、像素随身听、设置、存档槽位、即时存档浏览器、金手指和按键重映射等功能。

### RG353PS 移植内容

- **原生 640×480：** 基于上游 640 源码树，不通过缩放适配 720×480 版本。
- **显示：** BaseOS SDL2/KMSDRM + GLES2，适配 RK3566 Panfrost。
- **底座集成：** BaseOS 前端适配器、`manifest.conf`、启动脚本、MTP 辅助钩子和音量/电源所有权约定。
- **输入与系统：** RG353PS 实测按键映射、四轴摇杆参数、Wi-Fi 和休眠/唤醒适配。
- **启动性能：** HUD 字体解析移至后台线程。
- **模拟器核心：** 完整包包含固定版本 AArch64/musl 核心、所需运行库、许可证和对应源码材料。
- **启动提示音：** 正式包关闭前端默认 `Sfx::Boot`。此设置不关闭 ALSA、游戏音效和音乐；BaseOS 开机画面期间的功放上电噪声属于另一问题，本版本不宣称已修复。

### 数据资源与安装

- 中文完整包：`LUCKLUCK-SLOT-RG353PS-640x480-rg353ps-v2026.11.11-zh.zip`
- 英文完整包：`LUCKLUCK-SLOT-RG353PS-640x480-rg353ps-v2026.11.11-en.zip`
- 每个 ZIP 均附带同名 `.sha256` 文件。

为保证构建可复现，当前静态美术、字体、音乐、遮罩、开机图和网络脚本资源仍使用 SHA-256 固定的上游 v2026.11.09 数据包；前端可执行文件由 v2026.11.11 对应的 RG353PS 源码编译，不复制上游设备二进制。

解压后将目录合并到 BaseOS 数据分区根目录。升级前备份存档和自定义核心、字体、卡带美术及其他资源。包内同名文件会被覆盖。

### 验证状态

- CI 验证中英文 AArch64/musl 构建、完整包结构及 SHA-256 校验文件。
- 上游 v2026.11.11 移植提交及其后续关闭前端默认启动提示音的中文精简测试包均已成功构建；正式双语完整包以本次发布 CI 结果为准。
- **CI 成功不代表所有机种和游戏均已完成真机验证。** ROM 兼容性、BIOS、性能和实际游戏体验仍需逐项测试。
- BaseOS 开机画面阶段功放上电噪声仍作为独立硬件问题处理，本前端版本不作已解决声明。
- 本项目为 RG353PS 社区移植，不代表上游官方支持 RG353PS。

上游发布：<https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/tag/v2026.11.11>

---

## English

This is the native 640×480 RG353PS / RK3566 port based on upstream LUCKLUCK SLOT `v2026.11.11` (source commit `25e39ec7131ac05fc5187181c4de77d75bedad7c`). The Chinese and English full packages are built from the same tagged source and include the frontend plus the pinned RG353PS AArch64/musl emulator core pack.

### Upstream v2026.11.11 changes

- **Shelf search by initials:** press `SELECT + Y` on the game shelf to open the four-layer keyboard. Search by pinyin initials or a literal title substring. D-pad moves, `A` types, `B` erases and `L1/R1` changes layers; `X` restores all games, `START` applies and `MENU` cancels. The disc/music shelf is unchanged.
- **Short B press fix:** releasing B clears the long-press tracker, preventing a tap from opening Wi-Fi direct connect. Holding B still starts direct connect.
- **Keyboard case rendering:** Wi-Fi password keycaps are rendered from the original character, preserving lowercase and uppercase glyphs.
- Retains ten systems (GBA, GB, GBC, FC, SFC, MD, PS1, PCE, Arcade and Neo Geo), the music player, settings, save slots, save-state browser, cheats and button remapping.

### RG353PS port changes

- **Native 640×480:** based on the upstream 640 source tree, not a scaled 720×480 build.
- **Display:** BaseOS SDL2/KMSDRM + GLES2 on RK3566 Panfrost.
- **BaseOS integration:** frontend adapter, `manifest.conf`, launcher, MTP helper hook, and volume/power ownership contract.
- **Input and system integration:** RG353PS-tested button mappings, four-axis joystick parameters, Wi-Fi and suspend/resume support.
- **Startup performance:** HUD font parsing is moved to a background worker.
- **Emulator cores:** full packages include the pinned AArch64/musl cores, runtime libraries, licences and corresponding source materials.
- **Startup cue:** the release build disables the frontend's default `Sfx::Boot` cue only. ALSA, game effects and music remain enabled. The amplifier power-on noise during the BaseOS boot screen is a separate issue and is not claimed as fixed by this release.

### Data assets and installation

- Chinese full package: `LUCKLUCK-SLOT-RG353PS-640x480-rg353ps-v2026.11.11-zh.zip`
- English full package: `LUCKLUCK-SLOT-RG353PS-640x480-rg353ps-v2026.11.11-en.zip`
- Each ZIP has a matching `.sha256` sidecar.

For reproducible packaging, static artwork, fonts, music, overlays, boot logos and the network helper remain pinned to the SHA-256-verified upstream v2026.11.09 data archive. The frontend executable is compiled from the RG353PS source synced to v2026.11.11; no upstream device binary is copied.

Merge extracted directories into the BaseOS data-partition root. Back up saves and customised cores, fonts, cartridge art and other assets before updating. Matching files in the package are replaced.

### Validation status

- CI verifies the Chinese and English AArch64/musl builds, full package structure and SHA-256 sidecars.
- Chinese core-free test packages built successfully from the v2026.11.11 port commit and its boot-cue diagnostic follow-up; the formal bilingual full packages are subject to this release CI run.
- **A successful CI build does not mean every system and game has completed hardware gameplay validation.** ROM compatibility, BIOS requirements, performance and gameplay still require per-system testing.
- The BaseOS boot-screen amplifier transient remains a separate hardware issue; this frontend release does not claim to resolve it.
- This is a community RG353PS port, not an official upstream support claim.

Upstream release: <https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/tag/v2026.11.11>
