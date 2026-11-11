# LUCKLUCK SLOT — RG353PS 640×480 · v2026.11.09

Release tag: `v2026.11.09`  
Target: Anbernic RG353PS / RK3566 / native 640×480 / BaseOS 1.2.8.

## 简体中文

这是基于上游 LUCKLUCK SLOT `v2026.11.09` 的 RG353PS / RK3566 原生 640×480 移植版。保留上游 640×480 布局，并针对 BaseOS 与 RG353PS 替换设备显示、输入和启动适配层。本次正式包同时包含前端和已验证 ABI 的模拟器核心，不再要求用户另行下载核心包。

### 本次 RG353PS 修复（版本号保持不变）

- 修复 Wi-Fi 密码软键盘中小写字母显示成大写的问题。键盘字符现在按原始大小写绘制，不再复用状态栏的大写字形渲染。
- 已在 RG353PS 真机验证：大小写显示和文本输入均正常。
- 关闭默认启动提示音 `Sfx::Boot`，避免前端进入时再播放额外的默认提示音。
- `System/boot_opening.slotvid` 中的自定义音轨仍正常播放；已在 RG353PS 真机验证。游戏音效、音乐播放器及 ALSA/PCM 初始化不受影响。

### 上游 v2026.11.09 的主要内容

- 十个机种：GBA、GB、GBC、FC、SFC、MD、PS1、PCE、街机和 NEOGEO。
- 像素随身听、设置界面、存档槽位与即时存档浏览器、金手指和按键重映射。
- 按机种的显示遮罩、扫描线与色彩校正，以及主题、壁纸、卡带美术和开机画面。
- 修复跨机种同名游戏可能启动到错误机种的问题。
- 修复音乐播放器在启动后无法进入的问题。
- 大于 48 MiB 的 PS1 镜像改为路径流式加载，避免整张镜像读入内存。

完整上游改动见 [上游 v2026.11.09 发布说明](https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/tag/v2026.11.09) 与本仓库 `CHANGES.md`。

### RG353PS 移植内容

- **原生 640×480**：基于上游 `66-mods-640` 源码树，保留 640×480 逻辑画布和对应布局，不将 720×480 版本缩放适配。
- **显示后端**：将 H700 专用 EGL framebuffer 路径替换为 BaseOS SDL2/KMSDRM + GLES2，运行于 RK3566 Panfrost。
- **BaseOS 前端适配器**：集成 `manifest.conf`、启动脚本和 BaseOS 的音量/电源所有权约定；保留 MTP 辅助钩子。
- **输入适配**：应用 RG353PS 实测按键映射和摇杆轴参数，避免沿用 H700 的输入编号。
- **网络与电源协作**：集成 RG353PS 的 Wi-Fi 脚本兼容修正，并与 BaseOS 的无线模块初始化、休眠/唤醒流程配合。
- **启动性能**：将约 2.5 MB 的 HUD 符号字体解析移至启动后台线程，避免首次绘制图标时在 GL 线程承担解析开销。
- **模拟器核心随包提供**：整合 RG353PS AArch64/musl 核心包 `rg353ps-cores-aarch64-musl-v1.0.0`，包括 mGBA、gpSP、FCEUmm、Supafaust、FBNeo、Genesis Plus GX、PCSX ReARMed 和 PCE Fast，以及所需运行库、许可证和对应源码材料。核心包按 SHA-256 固定复用，不在前端发布时重新编译。

### 下载与安装

本仓库只发布 RG353PS 原生 640×480 包：

- `66slot-frontend-System-640480-V20261109-RG353PS.zip`：简体中文。
- `66slot-frontend-System-640480-V20261109-RG353PS-en.zip`：英文界面。
- `SHA256SUMS.txt`：两个完整包的 SHA-256 校验值。

解压后将包内目录合并到 BaseOS 数据分区根目录。包内包含 `System/`、`Music/`、`Overlay/`、`Bootlogos/`、BaseOS 前端适配器文件和模拟器核心。更新前备份存档及自定义的核心、字体、卡带图和其他资源；本包中的同名核心文件会被更新。

### 验证状态

- CI 已验证前端 AArch64/musl 构建、包结构、核心 ELF 架构及核心包文件完整性。
- 模拟器核心来自独立核心包 v1.0.0；其原始 CI 检查覆盖 AArch64 ELF、动态依赖解析、`dlopen(RTLD_NOW)` 和 libretro API 版本。
- **CI 成功不等于十个机种均已完成真机游戏验证。** 不同核心的 ROM 兼容性、BIOS、性能和实际游戏体验仍需按机种测试。
- 此版本为 RG353PS 移植发布，不代表上游项目的官方 RG353PS 支持。

---

## English

This is the RG353PS / RK3566 native 640×480 port based on upstream LUCKLUCK SLOT `v2026.11.09`. It retains the upstream 640×480 layout and replaces the H700-specific device display, input, and startup integration with the BaseOS/RG353PS adapter. The complete release packages include both the frontend and the ABI-validated emulator core pack.

### RG353PS fix included in this release refresh

- Fixed the Wi-Fi password keyboard rendering lowercase letters as uppercase. Keyboard glyphs now preserve their original case instead of reusing the uppercase status-bar renderer.
- Verified on RG353PS hardware: uppercase/lowercase display and text input both work.
- Disabled the default `Sfx::Boot` startup chime so the frontend no longer plays the extra fallback sound on entry.
- Custom audio embedded in `System/boot_opening.slotvid` continues to play; this was verified on RG353PS hardware. Game SFX, the music player, and ALSA/PCM initialization are unchanged.

### Upstream v2026.11.09 highlights

- Ten systems: GBA, GB, GBC, FC, SFC, MD, PS1, PCE, Arcade, and Neo Geo.
- Music player, settings UI, numbered save slots and save-state browser, cheats, and button remapping.
- Per-system display masks, scanlines and colour correction, plus themes, wallpapers, cartridge art, and boot-logo selection.
- Fixes cross-system duplicate game names resolving to the wrong machine.
- Fixes the music player failing to open after startup.
- Streams PS1 images larger than 48 MiB by path instead of loading the entire image into RAM.

See the [upstream v2026.11.09 release notes](https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/tag/v2026.11.09) and this fork's `CHANGES.md` for the complete upstream feature history.

### RG353PS port changes

- **Native 640×480:** based on upstream `66-mods-640`, retaining the 640×480 logical canvas and layout rather than scaling down the 720×480 build.
- **Display backend:** replaces the H700 EGL framebuffer path with BaseOS SDL2/KMSDRM + GLES2 on RK3566 Panfrost.
- **BaseOS frontend adapter:** includes `manifest.conf`, the launcher, BaseOS volume/power ownership markers, and the MTP helper hook.
- **Input adaptation:** applies RG353PS measured button mappings and joystick axis parameters instead of H700 input codes.
- **Network and power integration:** carries RG353PS Wi-Fi helper compatibility fixes and coordinates with BaseOS wireless initialization and suspend/resume.
- **Startup performance:** parses the roughly 2.5 MB HUD symbol font on a background worker, avoiding that work on the GL thread during the first icon upload.
- **Bundled emulator cores:** includes the pinned RG353PS AArch64/musl pack `rg353ps-cores-aarch64-musl-v1.0.0`: mGBA, gpSP, FCEUmm, Supafaust, FBNeo, Genesis Plus GX, PCSX ReARMed, and PCE Fast, plus runtime libraries, licence notices, and corresponding source materials. The core pack is reused by SHA-256 rather than rebuilt as part of the frontend release.

### Downloads and installation

This repository publishes the RG353PS native 640×480 packages only:

- `66slot-frontend-System-640480-V20261109-RG353PS.zip`: Simplified Chinese.
- `66slot-frontend-System-640480-V20261109-RG353PS-en.zip`: English UI.
- `SHA256SUMS.txt`: SHA-256 checksums for both complete packages.

Merge the extracted directories into the root of the BaseOS data partition. Packages include `System/`, `Music/`, `Overlay/`, `Bootlogos/`, the BaseOS frontend adapter, and emulator cores. Back up saves and customised cores, fonts, cartridge art, and other assets before updating. Matching core files in the package are replaced.

### Validation status

- CI verifies the AArch64/musl frontend build, package structure, core ELF architecture, and completeness of the pinned core pack.
- The cores come from the separate v1.0.0 pack, whose CI checks AArch64 ELF format, dynamic dependency resolution, `dlopen(RTLD_NOW)`, and libretro API version.
- **A successful CI build does not mean every system has completed gameplay validation on hardware.** ROM compatibility, BIOS requirements, performance, and gameplay still need testing per system.
- This is an RG353PS port release, not an official upstream claim of RG353PS support.
