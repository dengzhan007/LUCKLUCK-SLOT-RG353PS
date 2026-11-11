# LUCKLUCK SLOT（66slot）

[English](README.md) · **简体中文**

<p align="center">
  <img src="assets/66slot-title.webp" width="820"
       alt="LUCKLUCK SLOT —— 运行在 BaseOS 上的十机种复古游戏前端，适配 Anbernic H700 系列掌机">
</p>

> **RG353PS / BaseOS 移植分支。** `port/rg353ps-640-native` 直接以真正的上游
> `66-mods-640` 源码树为基底（上游提交 `4a0b60189fb4e29e57e73de14d997260a2e9d3a1`），
> 保留原生 640×480 合成画布和整套 640 布局。针对 RG353PS，打包时仅替换设备图形后端：
> 使用 BaseOS 的 SDL2/KMSDRM + GLES2/Panfrost，不使用 H700 专用 EGL framebuffer 后端。
> BaseOS 继续负责启动器、manifest 和 USB gadget；卡侧 MTP 辅助脚本启动钩子予以保留。
> 测试 CI 只构建简体中文版，正式标签分别构建中英文 RG353PS 完整包。每个完整包都整合固定版本的
> AArch64/musl 模拟器核心、运行库和许可证/源码材料；核心来源为
> [核心包 v1.0.0](https://github.com/dengzhan007/LUCKLUCK-SLOT-RG353PS/releases/tag/rg353ps-cores-aarch64-musl-v1.0.0)，不会在前端发布时重新编译。
>
> **分支 `66-mods-640` —— 640×480 版。** `66-mods` 是 720×480 那棵树（RG SP、RG34XX、RG34XXSP
> 与其余 720 组机器）；本分支是同一套前端按 **640×480** 面板排的（RG35XXSP 与 640 组）。
> 两者不是差一个编译开关：货架排布、面板底板、卡座、遮罩孔径、桌面图、电源菜单全部按分辨率
> 各排一份，这棵树也不带 MTP responder。**故意保持两条分支，不要合并回一条。**

> **这是上游 `slot` 的修改版（fork），非官方版本。**
> 原作 © Brandon T. Kowalski，MIT 许可；修改由 [@RetroGame66](https://github.com/RetroGame66)（你的六六）完成。
> 上游仓库已于 2026-11 从 `BrandonKowalski/slot` 迁至 [slot-cfw/slot](https://github.com/slot-cfw/slot)，旧地址会重定向。
> 本仓库与上游作者无隶属关系、未获其背书。改动记录在 [`CHANGES.md`](CHANGES.md)，来源与许可见 [`NOTICE.md`](NOTICE.md)。
> 上游自带 README 原样保留在 [`README.upstream.md`](README.upstream.md)，但它描述的是 GBA-only 的上游版本，底座也不同。
> 英文版说明见 [`README.md`](README.md)。

LUCKLUCK SLOT（缩写 **66slot**）是运行在 **BaseOS** 上的多机种掌机前端（适用 RG SP 及其他 BaseOS 设备）。它最初只是 RG SP 上的 GBA 单机前端，现已覆盖 **10 个机种**。界面为简体中文（另有一份英文构建）。

## 项目更名来龙去脉

本项目从上游 `slot` 派生，中间经历了一次底座切换、一次功能扩张和一次定名：

1. **上游起源。** `slot`（“slot.”）是 Brandon T. Kowalski 为 Anbernic RG SP 写的**专属 GBA 单机**前端，MIT 许可，只跑 Game Boy Advance。2026-11，上游从 `BrandonKowalski/slot` 迁到 **`slot-cfw`** 组织，并扩展成以 Game Boy 为核心的定制固件；本 fork 仍基于下面那个 GBA 时期的提交。
2. **派生（fork）。** 本仓库现为 **`RetroGame66/LUCKLUCK-SLOT`** 的 **`66-mods`** 分支，基于上游 `4345cb8b`（2026-09-11），由「你的六六」维护。仓库名在 2026-11-09 之前是 `RetroGame66/slot`。
3. **底座切换。** 早期本 fork 部署在 **AGS-102**（Brandon Kowalski 基于 BaseOS 的 RG SP 分支）上。自 **2026-10-24** 起不再使用 AGS-102：其功能已并入上游 **BaseOS**，且 AGS-102 分支已停更，因此现在直接跑在 BaseOS 上。联机/网络功能移到了卡上脚本，使代码与底座无关。
4. **定名。** **2026-10-26** 项目展示名定为 **LUCKLUCK SLOT**，缩写 **66slot**；**2026-11-09** 仓库名跟着改成 `LUCKLUCK-SLOT`（全名进仓库，缩写留给包名前缀 `66slot-frontend-…` 和界面）。**代码里面没有跟着改**：crate 仍叫 `slot` / `slot-ui` / `slot-store`，卡上二进制仍是 `System/slot`，状态文件仍是 `slot.state`。这些是内部名，改了只会让已经在用的卡失效、对读者没有半点好处。
   一句话：**路径里的 “slot” 是代码名，不是旧名。**
5. **范围。** 从 GBA 单机扩展到 **10 个机种**（见下）。

一句话：它曾是“RG SP 上的 GBA 前端”，现在是运行在 BaseOS 上、覆盖 10 个机种的掌机前端，名叫 **LUCKLUCK SLOT（66slot）**。

## 关于本修改版

LUCKLUCK SLOT 把一台 BaseOS 掌机变成一整排卡带。相对上游，它多了这些：

- **一排十个机种** —— GBA、GB、GBC、FC、SFC、MD、PS1、PCE、街机、NEOGEO，各自从 `Games/<机种>/` 读出，各有自己的核心、帧尺寸与按键排布。
- **中文界面**，字体从卡上取，不内嵌。
- **沿机身上沿排开的字母索引** —— 汉字库没有 A–Z 序，所以索引横着排在顶上。金手指是每卡一份（`System/Cheats/<游戏名>.cht`）；按键改映射（`System/remap.txt`）与各机种的屏幕观感（`System/display.txt`）属于整张卡。
- **浅色 / 深色两套机身主题**，在货架上用组合键切；其下还有七档界面配色。
- **上游没有的按机种画面工具** —— 可选的面板遮罩 / 孔径档位、色彩校正、主机种的扫描线、按机种的屏幕观感，全部在机器上就能调，不用重编。
- **像素随身听** —— 已经从一块磁带屏长成一台卡座：四阶液晶上有两块面板、40 套色板、滚动歌词、十段均衡器，从菜单进、看完回它自己的碟片货架。
- **存档槽位化** —— 编号槽位 + 能翻到每一张卡的即时存档浏览器；金手指跟着卡走，不再是全局一个文件。
- **首字母搜索** —— 游戏货架按 `SELECT`+`Y` 打开全键盘，输入拼音首字母（如 `cjjq`）或游戏名片段即可实时筛选；`X` 恢复搜索前货架，`START` 应用，`MENU` 取消。
- **开机转场** —— 在你的开机画面与货架首帧之间播一段动画。
- **MTP 与 WiFi 直连** —— 插上线资源管理器里就能看见这张卡；同一网络下走 sftp，不用拔卡。
- **随机壁纸轮播**、**玩家自己的卡带美术**（整卡图 + 凹槽里的那张纸）、**开机画面切换**。
- **一份源码，两个构建**：中文（默认）与英文（`--features device,lang-en`）。界面字串在 `slot-ui/src/lang.rs` 的两张表里，改一处两边一起动。

## 支持的机种（共 10 种）

机种由 `Games/` 下的**文件夹**决定。把 ROM 放进 `Games/<机种>/` 即可；在一个机种文件夹内，只要扩展名不是已知的非 ROM（`.txt`、`.png`、`.sav` 等），都会被当作该机种的游戏。少数扩展名有歧义——`.md` 是 Markdown、`.ps1` 是 PowerShell 脚本——所以这些文件**必须放在正确的机种文件夹内**，不要散落在 `Games/` 根层。Neo Geo 与 Arcade 是 `.zip` 合集，按文件夹识别，不看扩展名。

`selected_core.ini` 只列**可切换**的核心（下表标「默认 / 可选」者）；GB 与 GBC 跑前端**内置的 Game Boy 内核**，不可在 `selected_core.ini` 里切换。

| 机种 | 名称 | ROM 目录 | 接受的 ROM 格式 | 核心 |
|---|---|---|---|---|
| GBA | Game Boy Advance | `Games/GBA/` | `.gba` | mGBA（默认）/ gpSP |
| GB | Game Boy | `Games/GB/` | `.gb` | 内置 Game Boy 内核 |
| GBC | Game Boy Color | `Games/GBC/` | `.gbc` | 内置 Game Boy Color 内核 |
| SFC | 超级任天堂（SNES） | `Games/SFC/`（也认 `SNES`） | `.smc`、`.sfc`、`.fig` | supafaust |
| FC | 红白机（NES） | `Games/FC/`（也认 `NES`） | `.nes`、`.fds` | fceumm |
| NEOGEO | Neo Geo | `Games/NEOGEO/` | `.zip`（按文件夹识别） | fbneo |
| ARCADE | 街机（CPS1 / CPS2 / CPS3、FBNeo） | `Games/ARCADE/` | `.zip`（按文件夹识别） | fbneo |
| MD | 世嘉 MD / Genesis | `Games/MD/`（也认 `GENESIS`、`MEGADRIVE`） | `.md`、`.gen`、`.smd`（文件夹内也认 `.bin`） | genesis_plus_gx |
| PS1 | 索尼 PlayStation | `Games/PS1/`（也认 `PSX`） | `.cue`、`.chd`、`.pbp`、`.iso`（文件夹内也认 `.bin`） | pcsx_rearmed |
| PCE | PC Engine / TurboGrafx-16 | `Games/PCE/`（也认 `TG16`、`TURBOGRAFX`、`PCENGINE`） | `.pce`（文件夹内也认 `.bin` / `.cue`） | pce_fast |

**NDS 不支持。**

---

## 下载

发布包在 [Releases 页](https://github.com/dengzhan007/LUCKLUCK-SLOT-RG353PS/releases)。本仓库针对 RG353PS 原生 **640×480** 面板，每次发布两个完整包：

| 包 | 语言 |
|---|---|
| `66slot-frontend-System-640480-V*-RG353PS.zip` | 简体中文 |
| `66slot-frontend-System-640480-V*-RG353PS-en.zip` | 英文界面 |

每个包均包含前端、BaseOS 适配文件、UI 资源、八个外置 AArch64/musl libretro 核心、所需运行库及对应许可证/源码材料。GB 和 GBC 使用前端内置核心。

`SHA256SUMS.txt` 提供两个完整包的校验值。覆盖已有卡之前，请备份存档以及自定义核心、字体和美术资源。发布说明会注明实际验证范围；CI 和动态加载检查不代表所有核心都已经完成真机游戏测试。

## 键位一览

`+` 表示同时按。最全、永远最新的那份在机器上：**MENU → 全键位一览**（快捷键卡），它和程序自己读的是同一张表。

### 全局（任何界面）

| 按键 | 作用 |
|---|---|
| `SELECT` + `上` / `下` | 亮度 |
| `SELECT` + `左` / `右` | 蓝光滤镜 |
| `L3` / `R3` | 亮度 / 蓝光 +1 档（有可按下摇杆的机型） |
| 摇杆 | 等同十字键（摇杆机型） |
| `音量 +` / `-` | 音量；两个一起按静音 |
| 轻点 `POWER` | 休眠 / 唤醒（下电前把状态落盘） |
| 长按 `POWER` | 电源菜单：重启 / 关机 |

### 货架上

| 按键 | 作用 |
|---|---|
| `左` / `右` | 浏览卡带 |
| `L` / `R` | 上一个 / 下一个字母 |
| 轻点 `A` | 从最近的即时存档继续 |
| 长按 `A` | 把这个游戏从头开始 |
| 轻点 `X` | 收藏 / 取消收藏这张卡 |
| 长按 `X` | 收藏夹 / 全部 |
| 轻点 `Y` | 切子文件夹 —— 在音乐那一侧就是磁带与游戏互切 |
| 长按 `Y` | 选机种货架（帘子落下，机种面板浮在帘上） |
| 长按 `B` | 直连：卡上的 WiFi 起来，屏上给出地址，电脑用 sftp 连过来 —— 不用拔卡 |
| `SELECT` + `Y` | 按拼音首字母或原名片段搜索游戏（仅游戏货架） |
| `START` | 为这张卡挑一个模拟器 |
| `MENU` | 主菜单（其下还有快捷键卡） |
| `SELECT` + `START` | 日间 / 夜间主题 |
| `SELECT` + `R1` | 界面配色（7 档） |
| `SELECT` + `音量 ±` | 音频延迟档位 |

**首字母搜索用法：** 四层键盘 `abc / ABC / 123 / #+=`，`L1`/`R1` 换层，方向键移动，`A` 输入，`B` 退格。`X` 恢复搜索前货架，`START` 应用筛选，`MENU` 取消。匹配拼音首字母串或游戏名原文片段；多音字按内置拼音表中的常用读音匹配。

### 游戏内 · 所有机种通用

| 按键 | 作用 |
|---|---|
| 轻点 `MENU` | 游戏菜单 |
| 槽位表里 `左` / `右` | 换格 |
| 槽位表里轻点 `X` | 清这一格 |
| `菜单 → 重开本局` | 拔卡再插，不接存档 |
| `SELECT` + `MENU` | 同一个游戏菜单 |
| 长按 `MENU` | 存档、弹出卡带、回货架 |
| 双击 `MENU` | 即时存档浏览器（每张卡、每个槽） |
| `SELECT` + `L1` / `R1` | 读档 / 存档 |
| `SELECT` + `A` | 金手指（再按一次全开全关） |
| 长按 `L2` / `R2` | 倒带 / 快进 —— **PS1 以外**的所有机种 |
| 双击 `R2` | 锁定快进 |
| 双击 `L2` | 半速慢放 |
| `音量 +` / `-` | 音量；两个一起静音 |

**PS1** 的肩键是真正的游戏按键，所以它的传输键是 `SELECT` + `L2` / `R2`。

### 游戏内 · 按机种的屏幕观感

只在游戏内有效，选完立刻写进 `System/display.txt`，重启也在。`SELECT` 是延迟判定的（最多等 600ms 看有没有第二个键），所以组合键不会漏进游戏，单独按 `SELECT` 也照样在窗口过后传给游戏。

| 机种 | `SELECT` + `X` | `SELECT` + `Y` |
|---|---|---|
| GBA | 孔径档位（7 档：关、LCD3× 25 / 50 / 75 / 100%、扫描线 50 / 100%） | 色板 18 档；**长按**开浏览器 |
| GB | 遮罩艺术（美术环） | 色板 343 套；**长按**开浏览器 |
| GBC | 遮罩艺术（网格叠在上面） | 色板 18 档；**长按**开浏览器 |
| FC / SFC / MD / PS1 / PCE / NEOGEO / 街机 | 扫描线 / 遮罩档位 | 色板 18 档；**长按**开浏览器 |

**孔径表只给 GBA** —— 其余机种根本用不了点阵遮罩，所以它们的 `X` 调的是扫描线环。`Y` 在所有机种上都是色板。

合盖会写一份续玩快照（这一份总会写），并让屏幕变暗；之后是否关机、等多久，在 **设置 → 息屏后关机** 里设（其中一档只关屏不下电）。**放音乐时超时关机被屏蔽**，唤醒回到播放器而不是货架。

## 设置（参数调整页，共 10 项）

主菜单 → 参数调整页。10 行、每行两个；方向键移动光标，`A` 改值或打开。

1. **日期与时间** — 打开时钟设置
2. **刷新游戏库** — 重新扫描卡上 ROM（无需重启）
3. **加速时声音** — 快进 / 回溯时是否播放声音（开 / 关）
4. **息屏后关机** — 屏幕熄灭后多少分钟自动关机（其中一档 = 仅息屏不关机）
5. **快进倍率** — 快进档位
6. **回溯倍率** — 回溯档位
7. **插卡续玩** — 插卡 / 开机且卡槽有卡时自动读 `resume.state` 续玩
8. **拔卡续存** — 拔卡时把当前画面存入 `resume.state`（续玩快照）
9. **开机画面** — 从卡上 `Bootlogos/` 里挑选
10. **无线网络** — 打开 Wi-Fi 配置

---

## 像素随身听（音乐播放器）

主菜单 → 像素随身听。一张碟在卡座上播：碟在自己的窗口后面，曲名在货架上顶窗，走带键在插卡口那条带上 —— 卡座的走带键本来就在那。卡座本身画在一块四阶复古液晶上，按整数倍放大，格子才是方的。

**两块面板，第二块要第一块垫底。** 常态那块是曲名 + 频谱；另一块是同一首歌的滚动歌词。卡里的音乐没带 `.lrc` 就没有第二块可看 —— 键照样能按，按下去还是第一块。轻点 `MENU` 开碟片信息页（标签、第几首、时长）。键位见上面那张表。

它读什么：`Music/` 下按专辑分文件夹，`Music/Covers/` 放封面，`Music/Lyrics/` 放 `.lrc` 歌词，`Music/Fonts/` 放面板的汉字字体。解码器按容器选而不是按扩展名选 —— 卡里的音乐是手机或商店留下的什么都有。

## 壁纸

把 PNG 放进 `Wallpapers/`；每次开机随机抽一张铺在货架背后（重启一次换一张）。没有 `Wallpapers/` 文件夹时，前端回退到内置默认壁纸（霓虹隧道 + 跟随主题色板的呼吸光）。

---

## 卡内目录结构

```
BIOS/             可选 gba_bios.bin（或核心自带的高层 BIOS）。
Games/<机种>/    ROM，每个机种一个子文件夹（GBA/GB/GBC/SFC/FC/NEOGEO/ARCADE/MD/PS1/PCE）。
Labels/<机种>/   <游戏名>.png —— 贴进这张卡凹槽里的那张纸。
Saves/<机种>/    <游戏名>.sav / .srm 电池存档（平铺的 Saves/<游戏名>.sav 也认）。
States/<核心>/   <游戏名>/，每卡的存档槽位，按**生成它的那个核心**归档 ——
                 mgba / gpsp / fceumm / supafaust / genesis_plus_gx / pcsx_rearmed / pce_fast / fbneo。
System/          二进制、各核心，以及下面的可选配置。
Wallpapers/      .png，每次开机随机抽一张，铺在货架背后。
Bootlogos/       开机画面；在 设置 → 开机画面 里切换。
Cartridge/<机种>/ 整卡贴图 <游戏名>.png；缺省回退到 System/Carts/66SLOT_CART_<机种>.png。
Music/Fonts/     界面字体 —— 以及卡座面板自己那张面。
Music/Covers/    专辑封面。
Music/Lyrics/    歌词（.lrc）。
```

`System/` 里除二进制和各核心外都是可选配置：

```
theme.txt          housing / recess / opening / edge —— 机身配色。
selected_core.ini  <游戏名> = <核心>；默认 mGBA；两个 GBA 核心都随包在 System/ 里。
labels.txt         strip_tags on/off —— 游戏名保留还是去掉 [中]/[日]/[英] 版本标签。
display.txt        "mask cc_gba cc_gb cc_gbc" —— 按机种的屏幕观感。
audio.txt          stable / balanced / strict —— 音频延迟档位。
mask.txt / cc.txt  卡自带面板遮罩 / 色彩校正矩阵。
remap.txt          物理按键改映射，每行一个 "<物理键> = <游戏键>"。
Cheats/<游戏名>.cht 金手指文件，用 RetroArch 的 .cht 格式（cheatN_enable 决定哪条默认开；
                   # 开头是注释）。
wifi.conf          这张卡自己的 WiFi 凭据，由「设置 → 无线网络」写入。
slot.state         浅色/深色等开关 —— 由前端写入。
bootlogo.txt       当前用的是 Bootlogos/ 里的哪一张。
bootlogo-original.bmp  第一次动分区之前留下的原图，写坏了还能从卡上救回来。
library.cache      ROM 扫描缓存，按文件夹 mtime 记。删掉就是下一次开机全量重扫。
overlay.txt / shelf.txt  最后使用的 Game Boy 遮罩 / 最后所在的机种货架。
Carts/carts.tsv    按机种的壳色与卡带标志；二进制里的表退居兜底。
Carts/66SLOT_CART_<机种>.png  某个机种的内建卡带图。
boot_opening.slotvid  开机图与货架之间的那段动画。
mtp-responder + mtp-auto.sh  让这张卡在资源管理器里作为一个设备出现的那一对。
```

> **卡带美术有两个位置，它们不是二选一。** `Cartridge/<机种>/<游戏名>.png` 是**整张卡**；`Labels/<机种>/<游戏名>.png` 是**凹槽里那张纸** —— 等比裁满贴进去（不压扁），没有这个文件就留一块纯色。`Labels/` 在 2026-10-28 被摘掉，**2026-11-03 又加回来了**：用它的玩家说去掉之后改起来很不方便。（`labels.txt` 是上面那个 strip_tags 开关，与此无关，仍在使用。）

**PS1 可以让碟自己带 BIOS。** 汉化 / 改版的 PS1 游戏经常要配一版特定 BIOS，约定是放在 rom 旁边、
**与 rom 同名** —— `Ridge.cue` 配 `Ridge.bios`。它只对那一张碟生效，不会自己变成一张卡。

`System/theme.txt` 示例：

```
housing #24242a
recess  #1a1a1e
opening #050508
edge    #4d4d57
```

`System/selected_core.ini` 示例（gpSP 带硬件串口联机，mGBA 的 libretro 构建不带）：

```
Emerald = gpsp
```

---

## 安装

LUCKLUCK SLOT 运行在 **BaseOS** 上。BaseOS **同时支持单卡（TF1）与双卡（TF1 + TF2）** 两种布局——前端放在设备所采用的那种布局上即可；若存在第二张卡（TF2），它优先于第一张卡（TF1）。

1. 把适用于你设备的**最新 BaseOS 镜像**烧录到 SD 卡（参见 BaseOS 的安装说明）。
2. **插入机器、开机一次，让首次启动完成**——BaseOS 会把数据分区扩展到整张卡。随后会显示“无前端”提示（这是正常的）。**首次开机前不要往卡里拷任何东西**——第 2 步是“建分区”，提前放进去的文件会被清空。
3. 关机、拔卡、插到电脑上。会出现一个占满整张卡的卷（标为 `BASEOS`）。
4. **把 [下载](#下载) 里的前端包解压到卡根。** 根层是 `System/`；`Games/`、`Labels/`、`Saves/`、`Cartridge/`、`Wallpapers/`、`Bootlogos/`、`Music/` 等目录**随发布包自带、已经建好**，不要自己手动新建。
5. 把卡插回、开机，前端就启动了。

跑起来之后，**在货架上长按 `B`**，同一网络下的电脑就能用 sftp 连上这张卡，不用拔。插 USB 线时卡还会应 MTP，资源管理器里直接显示成一个设备。

（开发者：用 `python _tools/build_slot.py` 构建，用 `python _tools/deploy_bin.py` 经 adb 部署——它会把二进制推到 `System/slot`、`chmod`、清缓存并重启。或手动：`adb push System/slot /mnt/sdcard/System/slot && adb shell chmod 0755 /mnt/sdcard/System/slot && adb shell sync && adb reboot`。）

---

## 开机画面与开机片

开机静态画面与开机片是两个独立资源。静态画面从设置页的 `Bootlogos/` 中选择；可选开机片放在 `System/boot_opening.slotvid`，在静态画面之后、货架首帧之前播放。

`.slotvid` v1 文件由 64 字节头、每帧 16 字节索引，以及逐帧 JPEG 和对应 PCM 音频片段组成。RG353PS 使用原生 **640×480** 尺寸。开机片是可选的，删除该文件即可跳过动画。

## SLOTVID STUDIO（开机片制作工具）

[从上游 Release 下载 `slotvid-studio.zip`](https://github.com/RetroGame66/LUCKLUCK-SLOT/releases/download/v2026.11.09/slotvid-studio.zip)（Windows，单文件自包含工具）。它把普通视频转换为 `boot_opening.slotvid`，是独立工具，不包含在 RG353PS 前端 ZIP 中；不需要单独安装 Python、ffmpeg，也不需要联网。

为 RG353PS 选择 **640×480**，再设置 cover / contain / stretch、24 或 30 fps、是否保留声音及片段时长。建议约 **5 秒**，因为动画会与前端启动工作重叠；时间过长会增加开机等待。生成后把 `boot_opening.slotvid` 复制到卡上的 `System/` 目录。

## 构建（开发者）

设备构建是针对 aarch64 Linux 目标的 cargo 调用：

```
cargo build --release --target aarch64-unknown-linux-gnu \
  -p slot --no-default-features --features device
```

`--no-default-features --features device` 是必需的：默认特性会引入无法交叉编译的主机后端。本 fork 通过一个小转发器用 **zig**（`zig cc`）链接，因为 rustc 无法直接调用名为 `linker` 的 `cc` 风格驱动。辅助脚本 `_tools/build_slot.py` 封装了这一步，产出中文（默认）或英文（`SLOT_FEATURES=device,lang-en`）二进制到 `target-device/.../release/slot`。

构建后有用的脚本：`check_glibc.py`（二进制需要的最高 `GLIBC_*` 版本——BaseOS 是 glibc 用户态，因此用 2.35 级别的 toolchain 编译以保证可移植）和 `verify_slot.py`（确认是 aarch64 ELF 且带中文文案，而非旧英文）。

---

## 许可与致谢

本 fork 采用 **MIT**。上游 [`LICENSE`](LICENSE) 顶部的版权声明原样保留；修改部分在其下方。完整来源 / 基线 / 署名见 [`NOTICE.md`](NOTICE.md)。

模拟核心为 [mGBA](https://mgba.io)（endrift）、[gpSP](https://github.com/libretro/gpsp)（Exophase）、fceumm、supafaust、genesis_plus_gx、pcsx_rearmed、pce_fast 与 FBNeo —— 全部经 libretro 调用。前端用 `dlopen` 加载核心，从不链接或修改任何一个；各核心保留自己的许可，发布包的 `System/licenses/` 附带这些许可文本与它们要求的对应源码。设备启动 **BaseOS**（[pvaibhav/BaseOS](https://github.com/pvaibhav/BaseOS)）。界面字体为 **Plix Bold**（普力士，cathree3，SIL OFL），基于 IBM Plex Sans；卡座面板字体为 **Ark Pixel**（SIL OFL）。面板遮罩源自 LCD3x（公有领域，Gigaherz，libretro）。正文字体为 Open Sans（SIL OFL）与 Nerd Fonts（MIT）。

---

## AI 使用声明

上游前端由 Claude 编写、由其作者审阅。本 fork 的改动由 AI 助手起草，再由 [@RetroGame66](https://github.com/RetroGame66) 在设备上进行审阅与实测（包括只在真机上才显现的部分）。**本文件的不少文字也是机器生成的**——这一点与上原有别，特此说明。

这是个个人 fork。Issues 可能无人回复；`slot` 本身的问题请去上游。上游原版 README 见 [`README.upstream.md`](README.upstream.md)。

---

## 上游

凡未在上文列出的内容，都是 **Brandon T. Kowalski** 的工作；没有他就没有这个 fork。他的 README（含发布流程、硬件说明与致谢）原样保留在 [`README.upstream.md`](README.upstream.md)。关于 `slot` 本身，请见 <https://github.com/slot-cfw/slot> —— 上游已于 2026-11 从 `BrandonKowalski/slot` 迁到那里，旧地址会重定向。
