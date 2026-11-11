use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot::frontend::Frontend;
use slot::input::DeviceInput;
use slot_gfx::{Compositor, FbdevSurface, Surface};
use slot_power::DevicePlatform;

/// Where BaseOS mounts the card slot has never been checked against a running device, so
/// `launch.sh` exports `SLOT_ROOT` and this is only what is left if it did not.
const CARD: &str = "/mnt/sdcard";

/// The panel is 60 Hz and EGL is asked to lock to it, but a driver that ignores the swap
/// interval would spin this loop as fast as the GPU can clear, so the frame is timed too.
const FRAME: Duration = Duration::from_micros(16_667);
/// 上电第二屏收尾的交叉淡出帧数。叠在**主循环本来就有的前几帧**上，
/// 所以不额外增加开机时间；24 帧 ≈ 0.4 s（60 Hz）。
const FADE_FRAMES: u32 = 24;

/// Append one line to `System/boot.err`, truncated once per boot by `run`. Best effort: a card
/// that is read only or half mounted is not a reason to fail a boot, and this is only ever for
/// a human reading the card afterwards. A device has no console, and a startup that dies
/// before the frontend exists otherwise leaves nothing behind at all.
fn note_startup(root: &std::path::Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("System/boot.err"))
    {
        let _ = writeln!(f, "{line}");
    }
}

pub fn run() {
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    // BaseOS owns the USB gadget; this detached card-side helper restores the
    // existing MTP hand-off without replacing BaseOS MTP/storage services.
    slot::mtp::start(&root);
    // This boot's startup log, clean. `boot.log` is only written once the frontend exists, so
    // a failure before that would otherwise say nothing anywhere.
    let _ = std::fs::write(root.join("System/boot.err"), "");
    #[cfg(feature = "diag-no-boot-sfx")]
    note_startup(
        &root,
        "audio_diagnostic: Sfx::Boot suppressed; ALSA sink open path unchanged",
    );
    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("slot: {e}");
            note_startup(&root, &format!("surface: {e}"));
            return;
        }
    };
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            note_startup(&root, &format!("compositor: {e}"));
            return;
        }
    };
    // Anything the compositor wanted to say but could not print — today, why the screen
    // reflection was left off. Goes on the card, where a PC can read it.
    for line in compositor.warnings() {
        note_startup(&root, &format!("warn: {line}"));
    }
    // The card's panel mask, if it has one, and before the first frame is drawn. Same shape as
    // the typeface: a setting the card owns, read once, and a failure that leaves the built-in
    // table in place rather than taking the picture away.
    if let Some(mask) = slot::root::panel_mask(&root) {
        compositor.set_panel_mask(&mask);
    }
    let platform = DevicePlatform::new(root.clone());
    eprintln!("slot: {}", platform.report());
    platform.trace_boot();
    // The card's panel mask (and colour correction) is now owned by the app and pushed to the
    // compositor every frame from `frontend.render`, so it is not set here.
    //
    // ★ 2026-11-07 并行版：动画每一帧都要占着 EGL 主线程，而 `Frontend::boot` 那几秒
    //   全是扫卡 IO / 素材装填的纯 CPU 活，不碰 GL —— 把它整体挪到后台线程，动画就能盖住
    //   启动开销，而不是排在它后面白等 5 秒。GL 纹理上传（`upload_faces`）仍必须留主线程，
    //   所以放在 join 之后，与改前的相对顺序一致。
    //   PCM 设备独占：动画按帧喂 ring 需要它，Session 也需要它。先在**主线程**打开一次，
    //   再把同一个 Arc 交给后台，绝不让两边各开一个 sink。
    let sink = std::sync::Arc::new(slot::audio::Device::new());
    let audio_profile = slot::root::audio_profile(&root);
    if let Err(e) = sink.open(slot::audio::GBA_HZ, audio_profile) {
        eprintln!("slot: audio: {e}");
        note_startup(&root, &format!("audio: {e}"));
    }
    let ring = sink.ring();
    let boot_at = Instant::now();
    let boot_sink = std::sync::Arc::clone(&sink);
    let boot_thread = std::thread::Builder::new()
        .name("slot-boot".into())
        .spawn(move || {
            Frontend::boot_with_sink(Box::new(platform), boot_sink, audio_profile)
        });
    // 缺失/损坏/解码失败一律回退到原来的 Sfx::Boot + 货架首帧；动画失败不影响后台初始化。
    let opening = root.join("System/boot_opening.slotvid");
    let opening_stats = match slot::boot_opening::play(
        &mut compositor,
        &mut surface,
        ring,
        &opening,
    ) {
        Ok(s) => {
            note_startup(
                &root,
                &format!(
                    "boot_opening: frames={} fps={} {}x{} audio={} rate={} ring={} elapsed={}ms",
                    s.frames, s.fps, s.width, s.height, s.audio, s.audio_rate,
                    s.ring_rate, s.elapsed_ms
                ),
            );
            Some(s)
        }
        Err(e) => {
            note_startup(&root, &format!("boot_opening: skip ({e})"));
            None
        }
    };
    let anim_ms = boot_at.elapsed().as_millis();
    // 动画先放完而后台还没好时，画面停在末帧（已 swap 的帧缓冲不会被清），
    // 这段等待本来就是启动必需的，不产生额外时间。
    let mut frontend = match boot_thread {
        Ok(h) => match h.join() {
            Ok(f) => f,
            Err(_) => {
                note_startup(&root, "boot_parallel: boot thread panicked, rebuilding on main thread");
                Frontend::boot_with_sink(
                    Box::new(DevicePlatform::new(root.clone())),
                    std::sync::Arc::clone(&sink),
                    audio_profile,
                )
            }
        },
        Err(e) => {
            note_startup(&root, &format!("boot_parallel: spawn failed ({e}), serial fallback"));
            Frontend::boot_with_sink(
                Box::new(DevicePlatform::new(root.clone())),
                std::sync::Arc::clone(&sink),
                audio_profile,
            )
        }
    };
    let total_ms = boot_at.elapsed().as_millis();
    note_startup(
        &root,
        &format!(
            "boot_parallel: anim={}ms total={}ms extra_wait={}ms",
            anim_ms, total_ms, total_ms.saturating_sub(anim_ms)
        ),
    );
    frontend.upload_faces(&mut compositor);
    // ★ 2026-10-26：开机一声（继电器 + 涌入电流 + 电机起转，已压到 −12 dBFS）。放在第一帧之前、
    // 面都已经上传之后 —— 开机音不该跑在黑屏上，也不该和第一帧抢时间。
    let has_opening_audio = opening_stats.as_ref().is_some_and(|s| s.audio);
    if !has_opening_audio {
        frontend.play_boot_sound();
    }
    // ★ 2026-11-07：动画末帧交叉淡出到货架。用末帧 RGBA 另建一张独立纹理，
    //   生命周期只在主循环前 FADE_FRAMES 帧；播过完整带音轨动画才淡出，
    //   skip 回退（Sfx::Boot + 货架直出）保持原来的硬切，不引入新素材依赖。
    let mut fade: Option<(slot_gfx::TexId, u32)> = None;
    if let Some(s) = opening_stats.as_ref() {
        if s.audio && s.last_w > 0 && !s.last_rgba.is_empty() {
            let tex = compositor.create_texture(s.last_w, s.last_h, &s.last_rgba);
            fade = Some((tex, FADE_FRAMES));
        }
    }
    drop(opening_stats);
    let mut input = DeviceInput::open(&root);
    loop {
        let began = Instant::now();
        // 货架照常 render / advance，只是在合成 FBO 里最后盖一层渐隐的末帧。
        // smoothstep 让首尾都平缓，不出现"突然开始/突然结束"的台阶。
        if let Some((tex, left)) = fade {
            if left <= 1 {
                frontend.set_boot_fade(None, 0.0);
                compositor.release_texture(tex);
                fade = None;
            } else {
                let t = left as f32 / FADE_FRAMES as f32;
                frontend.set_boot_fade(Some(tex), t * t * (3.0 - 2.0 * t));
                fade = Some((tex, left - 1));
            }
        }
        frontend.render(&mut compositor, surface.window_size());
        if let Err(e) = surface.swap() {
            eprintln!("slot: {e}");
            return;
        }
        frontend.advance(&mut input);
        if frontend.restarting() {
            frontend.restart();
        }
        if frontend.powering_off() {
            frontend.poweroff();
            return;
        }
        if let Some(left) = FRAME.checked_sub(began.elapsed()) {
            std::thread::sleep(left);
        }
    }
}
