use std::path::{Path, PathBuf};

use slot_input::{Action, Gestures, Millis, RawEvent};
use slot_retro::Rumble;
use slot_store::{Core, System};
use slot_ui::{FfState, Toast};

use crate::app::{App, Phase};
use crate::audio::{Device, Profile, Ring, Sfx, GBA_HZ};
use crate::core::open_core;
use crate::emu::{CoreState, EmuHandle, Speed};
use crate::frames::FrameRef;
use crate::input::Pad;
use crate::persist;

/// Everything the frontend is that is not a window: the app, the core behind it, and the
/// gesture layer between the two. The binary owns the GL and hands raw events in.
pub struct Session {
    root: PathBuf,
    app: App,
    emu: Option<EmuHandle>,
    /// Opened once and outliving every cart. The cart clicks home while it is still on its
    /// way in, which is exactly when there is no core to own a sink.
    ///
    /// **Shared with the deck rather than owned by the shelf alone** (2026-10-07): the device's
    /// PCM is exclusive, so a second `open_sink` in the music player was always going to fail.
    /// See `audio::Device`.
    sink: std::sync::Arc<Device>,
    /// The profile the sink was opened for. Applied when the next cart is inserted.
    audio: Profile,
    gestures: Gestures,
    pad: Pad,
    rewinding: bool,
    fast: bool,
    /// ★ 2026-10-20：慢放是**锁存**的（L2 双击开、再按一次关），所以它跟 `fast` 一样是一个
    ///   状态而不是一次动作 —— 松开 L2 不关它，只有下一次按 L2 才关。
    slow: bool,
    /// What the motor was last set to. On the device that setting is a write to hardware and
    /// the core asks for the same value most frames.
    motor: u16,
    /// The keymap the pad last rebuilt from, so `sync_keymap` only touches the pad when the
    /// card actually changed it — a rebuild is a reallocation, and one per frame would be a
    /// reallocation every frame.
    keymap_last: Vec<(String, String)>,
    turbo_last: u8,
}

impl Session {
    pub fn boot(root: PathBuf) -> Self {
        let sink = std::sync::Arc::new(Device::new());
        let audio = crate::root::audio_profile(&root);
        // A frontend for one console knows the rate before it knows the cart. A device that
        // refuses it still opens, and the worker resamples to whatever it did take.
        if let Err(e) = sink.open(GBA_HZ, audio) {
            eprintln!("slot: audio: {e}");
        }
        Self::boot_with_sink(root, sink, audio)
    }

    /// 2026-11-07 并行启动入口：PCM 设备是独占的，上电第二屏在动画开播前就已经把 sink
    /// 打开并按帧喂 ring，所以这里只把**同一个**句柄接进 Session，绝不能开第二个。
    /// 除音频设备的所有权来自调用方外，行为与 `boot` 逐句一致。
    pub fn boot_with_sink(
        root: PathBuf,
        sink: std::sync::Arc<Device>,
        audio: Profile,
    ) -> Self {
        let mut app = App::boot(&root);
        // The pad starts on the card's own keymap rather than the identity: a card someone
        // has already tuned its six keys keeps that tuning from the first frame, and an empty
        // table reads back as the identity anyway (see `Remap::from_keymap`).
        let (km, thz) = app.keymap_snapshot();
        let km_map: std::collections::BTreeMap<String, String> =
            km.into_iter().collect();
        let pad_remap =
            crate::input::Remap::from_keymap(app.current_machine(), &km_map, thz);
        // Timed, because a device with no console turns every boot-time change into a matter
        // of opinion otherwise: the log is the only place the numbers exist.
        let _ = std::fs::write(root.join("System/boot.log"), "");
        let scan_at = std::time::Instant::now();
        // The deck plays through the shelf's own device. Handed over before anything can open
        // the music row, so there is no window in which it would open a second one.
        app.attach_audio(std::sync::Arc::clone(&sink));
        let scan_ms = scan_at.elapsed().as_millis();
        let session = Session {
            app,
            root,
            emu: None,
            sink,
            audio,
            gestures: Gestures::new(),
            pad: Pad::with_remap(pad_remap),
            rewinding: false,
            fast: false,
            slow: false,
            motor: 0,
            keymap_last: km_map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            turbo_last: thz,
        };
        session.boot_note(&format!("library scan + shelf {}", scan_ms));
        session
    }

    /// Mixed in over whatever the game is already playing, so it lands with the thing on
    /// screen rather than a buffer behind it.
    pub fn play_sfx(&mut self, sfx: Sfx) {
        let ring = self.sink.ring();
        let rate = ring.sample_rate();
        if rate == 0 {
            return;
        }
        let mut samples = sfx.render(rate);
        // The core's audio is levelled by the worker on its way to the ring. A clip mixed in
        // here never passes that, so the same level has to be applied on this path or the
        // slot stays loud under a game turned all the way down.
        crate::audio::volume::apply(&mut samples, self.app.output_volume());
        ring.mix(&samples);
    }

    pub fn audio_queued(&self) -> usize {
        self.sink.ring().queued_frames()
    }

    /// What the device is about to play, for the tests that need to hear what was queued
    /// rather than only how much of it there is.
    pub fn audio_ring(&self) -> std::sync::Arc<Ring> {
        self.sink.ring()
    }

    /// Straight to the motor, skipping the phase. Only `sync_rumble` and a caller standing
    /// in for a cart that buzzes have any business here.
    pub fn rumble(&mut self, strength: u16) {
        if strength == self.motor {
            return;
        }
        self.motor = strength;
        self.app.set_rumble(strength);
    }

    /// The core's end of the motor, or nothing when the slot is empty.
    pub fn core_rumble(&self) -> Option<&Rumble> {
        self.emu.as_ref().map(EmuHandle::rumble)
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    /// The card's root, for the walks that belong to the frontend rather than to the app.
    ///
    /// `App` never holds this: everything it reads off the card it was handed at boot, and the
    /// things that arrive later — a cheat list, the overlays, and now the card's music — are read
    /// here and handed over, so that reading a directory is always somebody's explicit choice
    /// rather than a side effect of constructing a value.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The emulator thread's own handle, reachable only from here — `App` never touches the
    /// transport or the core itself (see its `link` field's doc comment). `None` before a
    /// cart has spawned one. Exists for whoever ends up wiring a link indicator, and for
    /// tests proving `act`'s own bridge to `EmuHandle::end_link` actually reaches the
    /// emulator thread rather than only `App`'s bookkeeping.
    pub fn emu(&self) -> Option<&EmuHandle> {
        self.emu.as_ref()
    }

    pub fn frame(&self) -> Option<FrameRef> {
        self.emu.as_ref().and_then(|e| e.latest_frame())
    }

    /// A cart in the slot is a core running, so this is also "is there a game layer".
    pub fn has_core(&self) -> bool {
        self.emu.is_some()
    }

    /// Whether the game layer may be drawn at all. Gate the draw on this, never on
    /// `has_core`: a handle exists before its worker has produced anything.
    /// Diagnostic: how many frames have been taken out of the handoff buffer. Only the
    /// render path may take one, so this must equal the number the renderer received.
    pub fn frame_ready(&self) -> bool {
        self.emu.as_ref().is_some_and(EmuHandle::frame_ready)
    }

    /// Frames the current core has produced. Zero while it is loaded but paused, which is
    /// what the insert relies on.
    pub fn frames_published(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::published_count)
    }

    /// What the worker last reported reading `speed` as, or `None` with no core to ask. Unlike
    /// `sync_speed`'s write, this is a fact about the worker's own last pass through its loop —
    /// see `EmuHandle::observed_speed` for why that is a stronger thing to wait on than a frame
    /// count holding still.
    pub fn observed_speed(&self) -> Option<Speed> {
        self.emu.as_ref().map(EmuHandle::observed_speed)
    }

    pub fn frames_taken(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::frames_taken)
    }

    /// Gated on the screen being up as well as on a published frame. A core starts loading
    /// on the way into the slot and publishes long before the cart is home, so without this
    /// the game is playing behind the cart for most of the animation. The app owns the answer
    /// because it owns the draw list the game layer is an item in.
    pub fn game_visible(&self) -> bool {
        self.app.game_visible()
    }

    /// Called every frame whether or not anything was pressed: the gesture windows expire on
    /// the tick, not on an event.
    pub fn feed(&mut self, events: impl IntoIterator<Item = RawEvent>, now: Millis) {
        // The turbo rows tick on the frame clock, not the edge clock: a held key must pulse
        // its on/off half on a press-less frame, so this runs every frame, before the mask the
        // core is polled with is built.
        self.pad.step(now);
        // ★ 2026-10-21：`SELECT+START` 在**有卡的时候是游戏的**（不少游戏自己的暂停/菜单键），
        //   在货架上是切日/夜主题。手势层看不见屏，而这条和弦又是在它那里被吞掉的 —— 吞了就
        //   再也递不到核心，所以只能每帧把"仓里有没有卡"告诉它一次。和上面的 `pad.step` 一样
        //   是每帧都跑的活，不挂在按键边沿上。
        self.gestures.set_start_to_game(self.emu.is_some());
        // ★★ 2026-10-25：同上，但多问一句"是不是 PS1"。PlayStation 的 `L2`/`R2` 是真游戏
        //   按键，所以这一局里裸按的肩键归核心，倒带/快进/慢放退到 `SELECT+L2` / `SELECT+R2`。
        //   其余六个机种这一位恒假，行为一字未改。**两处一起置**：手势层决定按下去发什么，
        //   `Pad` 决定 `mask()` 报不报那两个位，漏一个就会出现"按了没反应"。
        let ps1 = self.emu.is_some() && self.app.current_machine() == System::Ps1;
        self.gestures.set_l2r2_to_game(ps1);
        // ★★ 2026-11-02（六六）：卡座相位（`Phase::Listening`）时裸 `A` 分短按/长按——
        //   短按播放/暂停、长按停止。手势层看不到屏，所以每帧告诉它一次"现在是不是卡座"，
        //   和上面 `set_l2r2_to_game` 同一个形状。合盖（`Doze`）时这一位是假，A 照旧是游戏的，
        //   所以 lid 底下没有"长按 A 误触发停止"这回事。
        self.gestures.set_a_to_deck(self.app.is_listening());
        // ★★ 2026-10-27：PS1 的 L2/R2 现在走 `remap` 表（八行），不再需要这条独立通道。
        // 关掉它，否则裸按 L2/R2 会被重复报位、且改键成 NONE 也压不下去。
        self.pad.set_shoulders(false);
        let mut actions = Vec::new();
        for ev in events {
            actions.extend(self.gestures.feed(ev, now));
        }
        actions.extend(self.gestures.tick(now));
        for action in actions {
            self.act(action);
        }
        if let Some(emu) = &self.emu {
            emu.set_input(self.pad.mask());
        }
    }

    /// `App` only ever holds a session's own bookkeeping (see `App::link`'s doc comment) —
    /// the transport and the core it feeds live on the emulator thread, reachable only
    /// through `EmuHandle`. Watching the edge here, around whatever `f` does to `App`, is
    /// what closes that gap for every action that can end a session, without either side
    /// having to know the other exists.
    ///
    /// The one place this is called from used to be inline inside `act`, wrapping only
    /// `App::apply` — which covered every button but missed a critical battery reading,
    /// which reaches `App` through `update`/`timers` instead, with no `Action` and no
    /// `apply` call anywhere on its path. `App::begin_power_off` now ends a live session
    /// itself (see its own doc comment) the same way `doze` already did, but that ending
    /// still needed a way to reach the emulator thread — the whole reason this moved out of
    /// `act` and became the one thing both of `Session`'s own entry points into `App`
    /// (`act`'s `apply`, `update`'s `update`) route every call through. A session can now
    /// only ever end from inside `App` on a path this already watches; there is no longer a
    /// way to add a sixth route that skips it.
    fn bridge_link(&mut self, f: impl FnOnce(&mut App)) {
        let had_link = self.app.link_active();
        f(&mut self.app);
        // ★ 2026-10-26：联机建立 / 断开各一声（对讲机式的接入与切出，−12 dBFS）。
        // 放在这里是因为这两行就是链路状态唯一的真相源（上面那段注释说的）。
        if !had_link && self.app.link_active() {
            self.play_sfx(Sfx::LinkUp);
        }
        if had_link && !self.app.link_active() {
            self.play_sfx(Sfx::LinkDown);
            if let Some(emu) = &self.emu {
                // Tells the core the session is over (`RetroCore::stop_link`, if it offered
                // a `stop` to hear it through) and drops the transport, which is what
                // actually closes the wire — see `Cmd::EndLink` in `emu.rs`.
                emu.end_link();
            }
        }
    }

    fn act(&mut self, action: Action) {
        if trace() {
            eprintln!("slot: {action:?} in {:?}", self.app.phase());
        }
        // A system-shelf swap blanks the buttons *while the shutter is moving* (descending
        // before it has covered the screen, or rolling back up): nothing reaches `App` and the
        // pad is not updated, so the movement plays out untouched. The full-shutter pause that
        // waits for the user to point at a machine is **not** locked — its keys must get through.
        // The volume and mute side of `act` is behind this too, which is the point — a level
        // change mid-movement is a key that landed.
        if self.app.input_locked() {
            return;
        }
        match action {
            Action::RewindStart => self.rewinding = true,
            Action::RewindStop => self.rewinding = false,
            Action::FfStart => self.fast = true,
            Action::FfStop => self.fast = false,
            // 慢放的两条边，跟快进那两条一样只是记一个状态：真正把速度给不给下去是
            // `sync_speed` 的事，屏上那一句提示是 `App` 的事（联机中被拒时它会屏抖而不是提示）。
            Action::SlowStart => self.slow = true,
            Action::SlowStop => self.slow = false,
            Action::CheatToggle => {
                // SELECT+A opens the cheat table (the browsable, per-code list) rather than only
                // flipping a master flag. Pressed again while the table is up it flips every code
                // at once — the old master toggle, now reachable from inside.
                if self.app.cheat_menu().is_some() {
                    self.app.toggle_all_cheats();
                    self.push_cheats();
                    self.app.toast_now(if self.app.cheats_on {
                        Toast::CheatsOn
                    } else {
                        Toast::CheatsOff
                    });
                } else {
                    self.app.open_cheat_menu();
                }
                return;
            }
            _ => {}
        }
        // A button a menu used is not the game's, on either edge of it: the press that opens
        // one and the press that dismisses it both belong to the menu. Read on both sides of
        // the `apply`, because either of those two presses is the one that moves the answer.
        let menu = self.overlaid();
        self.bridge_link(|app| app.apply(action));
        // A row toggled inside the cheat table set a flag the table itself cannot act on, because
        // it has no reach to the emulator. Push the list now that `apply` has run.
        if self.app.take_cheats_dirty() {
            self.push_cheats();
        }
        // After apply: the level the sink wants is the one the action just produced.
        if matches!(
            action,
            Action::VolumeUp | Action::VolumeDown | Action::MuteToggle
        ) {
            if let Some(emu) = &self.emu {
                emu.set_volume(self.app.output_volume());
            }
        }
        // A press reaches the game pad only when a core is actually live and no menu has it.
        // The shelf (and the menu / shortcuts / settings / clock phases) carry no core, yet
        // `overlaid()` does not list them — so without the `!self.emu.is_some()` guard a key
        // pressed on the shelf would be written into `pad.down` and leak into the next game.
        // FC paid for that first: its stock map binds `X`/`Y` to the turbo rows (连发 A/B), so
        // a `Y` still held from the shelf's "hold to swap machine" gesture arrived in-game as a
        // stuck turbo that only a second press cleared. No core running ⇒ clear, never apply.
        if menu || self.overlaid() || !self.emu.is_some() {
            self.pad.clear();
        } else {
            self.pad.apply(action);
        }
        // On the action rather than on the next frame: an eject or a doze may be the last
        // thing this process does, and a motor left running outlives it.
        self.sync_rumble();
    }

    pub fn update(&mut self, dt: f32) {
        self.bridge_link(|app| app.update(dt));
        // The wire a link that just came up runs over. `App` holds a session's own
        // bookkeeping and never a transport (see `App::link`), so this is the hop that
        // carries one to the emulator thread — the mirror of `bridge_link`'s own hop for the
        // ending. Ahead of `sync_speed` below, so the frame the overlay closes on is already
        // a frame the game is running again.
        if let Some((client_id, transport)) = self.app.take_link_transport() {
            match &self.emu {
                Some(emu) => emu.begin_link(client_id, transport),
                // No core to carry it. Dropping the transport closes the socket, which is
                // the only honest thing to do with a session that has nowhere to run.
                None => eprintln!("slot: link: a transport arrived with no core to run it"),
            }
        }
        if let Some(sfx) = self.app.take_sfx() {
            self.play_sfx(sfx);
        }
        self.sync_core();
        // ★ 2026-10-21：**重开本局** —— 把核心拨回开机那一刻（`retro_reset`）。放在
        //   `sync_core` 之后：这一帧刚生出来的句柄才接得住这条命令。命令走队列，核心在自己的
        //   线程里复位，所以这里立刻返回，相位一帧都不动 —— 玩家看到的就是游戏从开机重来。
        if self.app.take_restart_core() {
            match &self.emu {
                Some(emu) => emu.reset(),
                None => eprintln!("slot: restart: no core to reset"),
            }
        }
        // After the core sync: a handle spawned or dropped this frame has published nothing
        // the renderer may show.
        self.app
            .set_game_ready(self.emu.as_ref().is_some_and(EmuHandle::has_published));
        self.sync_speed();
        self.sync_rewind_hud();
        self.sync_ff_hud();
        self.sync_rumble();
        self.sync_audio_profile();
        self.sync_keymap();
    }

    /// The core writes its motor from the emulator thread and this is the one place that
    /// reaches the hardware with it. The phase has the last word: a cart on its way out, a
    /// paused switcher and a doze all stop the motor whatever the core last asked for.
    /// The shelf is the only screen that can change the audio profile, and the only one where
    /// applying it is free: the device buffer is fixed when the PCM is opened, so a change
    /// means reopening the hardware. Guarded on `playing()` as well, so that even a stray
    /// chord cannot reopen the device under a running game.
    fn sync_audio_profile(&mut self) {
        let wanted = self.app.audio_profile();
        if wanted == self.audio || self.playing() {
            return;
        }
        self.audio = wanted;
        if let Err(e) = self.sink.open(GBA_HZ, wanted) {
            eprintln!("slot: audio: {e}");
        }
        eprintln!("slot: audio profile {}", wanted.as_str());
    }

    /// Append a line to `System/boot.log`, which `boot` truncates at every start. The device
    /// has no console, so a boot being tuned has to leave its timings where a PC can read
    /// them — and a card is the one thing every one of these has.
    /// How many carts the shelf is holding. Read only to label the face timing, so the count
    /// and the duration can be read off the log together.
    pub fn cart_count(&self) -> usize {
        self.app.carts().len()
    }

    pub fn boot_note(&self, line: &str) {
        use std::io::Write;
        let path = self.root.join("System/boot.log");
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    fn sync_rumble(&mut self) {
        let want = match &self.emu {
            Some(emu) if self.playing() => emu.rumble().strength(),
            _ => 0,
        };
        self.rumble(want);
    }

    /// Rebuild the pad's remap when the card's keymap changed under it. The menu is where a
    /// mapping gets saved, and the pad is the only thing the running game reads — so without
    /// this a saved key would not reach a cart that was already in the slot, and the first
    /// game after the change would run on the binding it started with. **Only when it
    /// changed**: a rebuild is a reallocation, and doing it every frame would be a
    /// reallocation every frame, which is not what the two gears over there do.
    fn sync_keymap(&mut self) {
        let (km, thz) = self.app.keymap_snapshot();
        let cur: Vec<(String, String)> = km;
        if cur == self.keymap_last && thz == self.turbo_last {
            return;
        }
        let km_map: std::collections::BTreeMap<String, String> = cur.iter().cloned().collect();
        self.pad
            .set_remap(crate::input::Remap::from_keymap(self.app.current_machine(), &km_map, thz));
        self.keymap_last = cur;
        self.turbo_last = thz;
    }

    /// The badge tracks the speed the game is actually running at, so a cart on its way in, a
    /// paused switcher, or a live link session refusing the hold all take it down even with
    /// R2 still latched — a badge that kept showing Held or Latched over a session withholding
    /// the speed would be telling the player their input landed when it did not, the exact
    /// lie `sync_rewind_hud`'s own `actually_rewinding` guards against for the rewind bar.
    fn sync_ff_hud(&mut self) {
        let ff = match (self.actually_fast_forwarding(), self.gestures.ff_latched()) {
            (false, _) => FfState::Off,
            (true, false) => FfState::Held,
            (true, true) => FfState::Latched,
        };
        self.app.set_ff(ff);
    }

    /// The bar belongs to the hold, so it is pushed every frame it lasts and taken down the
    /// moment L2 stops being a rewind, whether that was the button, the phase, or a live link
    /// session refusing it — a bar held up over a rewind that never actually happens would be
    /// showing the player a lie about their own input.
    fn sync_rewind_hud(&mut self) {
        let fill = self
            .actually_rewinding()
            .then(|| self.emu.as_ref().map(EmuHandle::rewind_fill))
            .flatten();
        match fill {
            Some(fill) => self.app.show_rewind(fill),
            None => self.app.hide_rewind(),
        }
    }

    /// L2 held, over a game actually in charge of the device, with nothing forbidding it.
    /// Shared between `sync_speed` (which acts on it) and `sync_rewind_hud` (which shows it),
    /// so the two can never drift into disagreeing about whether a rewind is really underway.
    fn actually_rewinding(&self) -> bool {
        self.rewinding && self.playing() && self.app.may_rewind()
    }

    /// R2's counterpart to `actually_rewinding`, for the identical reason: shared between
    /// `sync_speed` (which acts on it) and `sync_ff_hud` (which shows it), so the badge and
    /// the speed the core is actually run at can never drift into disagreeing about whether
    /// fast forward is really underway.
    fn actually_fast_forwarding(&self) -> bool {
        self.fast && self.playing() && self.app.may_fast_forward()
    }

    /// ★ 2026-10-20：L2 的慢放真的跑起来的三个条件 —— 锁存着、游戏在跑、联机没禁它。
    ///   跟上面两条同一副骨架，理由也同一条：`sync_speed` 给下去的速度和屏上那句提示不许
    ///   各说各话。
    fn actually_slowing(&self) -> bool {
        self.slow && self.playing() && self.app.may_slow()
    }

    fn inserting(&self) -> bool {
        matches!(self.app.phase(), Phase::Inserting { .. })
    }

    fn showing_polaroids(&self) -> bool {
        matches!(self.app.phase(), Phase::Polaroids { .. })
    }

    /// The screens whose buttons belong to them rather than to the game underneath. Both
    /// pause the core as well (`held`, and `sync_speed`'s own `showing_polaroids`), which is
    /// what keeps a press landing here from being seen — but a pause is not a mask, and a
    /// press taken while paused whose release arrives after it is a button the game finds
    /// already down. This is what stops either edge reaching the pad at all.
    fn overlaid(&self) -> bool {
        self.showing_polaroids()
            || self.app.game_menu_open()
            || self.app.palette_browser().is_some()
            || self.app.keymap_edit().is_some()
    }

    /// Whether the game is live and in charge of the device. Not the phase alone: the power
    /// menu and the shutdown screen are overlays rather than phases — deliberately, so
    /// cancelling returns to whatever was underneath — and the phase stays `Playing` under
    /// both. Reading only the phase left the core running flat out, and the motor buzzing,
    /// behind a screen that had already replaced the game.
    fn playing(&self) -> bool {
        matches!(self.app.phase(), Phase::Playing { .. }) && !self.held()
    }

    /// The screens that have taken the panel away from a cart still seated. The switcher is
    /// not one of them: it has its own phase and `sync_speed` names it separately.
    fn held(&self) -> bool {
        self.app.power_menu().is_some()
            || self.app.game_menu_open()
            || self.app.shutting_down()
            || self.app.cheat_menu().is_some()
            || self.app.palette_browser().is_some()
            || self.app.keymap_edit().is_some()
    }

    fn dozing(&self) -> bool {
        matches!(self.app.phase(), Phase::Doze { .. })
    }

    fn ejecting(&self) -> bool {
        matches!(self.app.phase(), Phase::Ejecting { .. })
    }

    /// The switcher pauses the game rather than dimming a live one. Paused publishes no
    /// frames, so the compositor keeps showing the last one behind the cards.
    fn sync_speed(&self) {
        if let Some(emu) = &self.emu {
            // Loading a core and running one are separate things. The insert animation
            // hides the load, but a core left running behind the cart burns through the
            // GBA bios intro, so the reveal catches only its tail. Paused until the cart is
            // home, the boot animation starts from its first frame as the screen comes on.
            //
            // An eject stops it for the same reason in reverse: the game is over as soon as
            // the button is held, and a core still running behind a dark screen is a game
            // still being heard after the player ended it.
            //
            // Fast forward belongs to the game, so a cart still sliding in runs at its own
            // pace no matter what R2 is doing.
            emu.set_speed(
                if self.inserting()
                    || self.ejecting()
                    || self.showing_polaroids()
                    || self.dozing()
                    || self.held()
                {
                    Speed::Paused
                } else if self.actually_fast_forwarding() {
                    // A live link session forbids fast forward the same way it forbids
                    // rewind: running this device's machine out ahead of what the peer has
                    // actually been sent is a desync with no way back, and libretro's
                    // netpacket contract names fast forward in the same breath as pausing
                    // and rewinding. `App::apply`'s own `FfStart` arm is what shakes the
                    // screen for the player; this is what actually withholds the speed.
                    Speed::Fast
                } else if self.actually_slowing() {
                    // ★ 2026-10-20: slow motion, and it is refused for the same reason the two
                    //   above are — a peer whose machine is being walked at half speed is a peer
                    //   that is no longer on the same frame, which is the desync libretro's
                    //   netpacket contract forbids by name. **Fast forward wins over it** because
                    //   the two keys can be held at once and a player pressing R2 mid-slow-motion
                    //   is asking to speed up, not to stay slow.
                    Speed::Slow
                } else {
                    Speed::Normal
                },
            );
            // Held through an eject or into the switcher, L2 stops rewinding rather than
            // eating the history of a cart that is on its way out — and refused outright
            // during a live link session, since rewinding one device desynchronises the
            // other with no way back to agreement.
            emu.set_rewinding(self.actually_rewinding());

            // The two gears travel with the speed, every frame, for the same reason the speed
            // does. **Pushed from the card rather than from the settings screen**: a setting is
            // something the card remembers, and the worker needs the gear of the session that is
            // running — not of the session in which someone once opened a screen. Reading it here
            // is also what makes the change land on the next hold of the key with no second place
            // to remember to push from, which is the bug the deck's volume shipped with once.
            emu.set_ff_steps(self.app.ff_steps());
            emu.set_rewind_speed(self.app.rewind_speed());
            // ★ 2026-10-16: and the sound switch rides with them for exactly the same reason —
            // the worker decides every pass whether the core's audio is kept or dropped, and a
            // value pushed only from the settings screen would leave the first session after a
            // boot (the one where nobody opens a screen) on whatever the worker defaulted to.
            emu.set_ff_sound(self.app.ff_sound());
        }
    }

    /// `SLOT_NO_CORE=1` leaves the slot on screen with the cart in it and never starts a
    /// game, so the insert can be watched at full length. Eject and insert again to replay.
    fn no_core() -> bool {
        std::env::var_os("SLOT_NO_CORE").is_some_and(|v| v != "0")
    }

    /// The core exists exactly while a cart is in the slot. Loading it is what the insert
    /// animation is hiding, so the spawn happens on the way in, not on arrival.
    ///
    /// ★ 2026-10-25：**退卡期间核心刻意保留，只是被暂停。** 我曾把它改成 `Ejecting` 一上来就
    /// `self.emu = None`，被 `eject.rs::the_core_stops_before_the_cart_moves` 逮到
    /// （"the worker never reported pausing"）—— 那条测试等的就是 `observed_speed() == Paused`，
    /// 而 Worker 没了就永远等不到。**原来这样是对的**，理由见 `sync_speed` 里那段注释：
    /// 「游戏在按下那一刻就结束了，黑屏后面还在跑的核心是玩家已经结束却还在响的游戏」——
    /// 所以退卡要的是**暂停**，不是**卸载**；暂停必须有一个活着的 Worker 才能被观测到。
    /// 释放点仍然是回到货架（`Phase::Shelf`），那一刻动画早已放完，占用是有界的。
    fn sync_core(&mut self) {
        if Self::no_core() {
            return;
        }
        let stem = match self.app.phase() {
            Phase::Shelf => {
                self.emu = None;
                return;
            }
            Phase::Inserting { cart, .. } => cart.clone(),
            _ => return,
        };
        if self.emu.is_none() {
            self.spawn_core(&stem);
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => self.app.on_core_ready(),
            // A refused cart leaves a dead worker behind. Dropping it here is what frees the
            // core for the next insert, since libretro allows only one.
            Some(CoreState::Failed) | None => {
                self.emu = None;
                self.app.on_core_failed();
            }
        }
    }

    fn spawn_core(&mut self, stem: &str) {
        // ★ 2026-11-09：**当前机种的货架优先**（`App::cart_by_stem` → `Shelf::find`）。
        //   从前这里是 `carts().iter().find(|c| c.stem == stem)`，也就是全库里**排序靠前**的
        //   那条 —— 而同名跨机种是常态（`Games/FC/魂斗罗.nes` 恒排在 `Games/MD/魂斗罗.md`
        //   之前），于是 MD 货架上选中的那张卡会拉起 NES 核心、跑 FC 的 rom。玩家报的
        //   「加载 MD 游戏时显示的是 FC 卡带且进去也是 FC」正是从这里进去的。
        let Some((rom, system)) = self
            .app
            .cart_by_stem(stem)
            .map(|c| (c.rom.clone(), c.system()))
        else {
            return;
        };
        // Resolved once, and only here: this is which dylib gets opened, which `States/<core>/`
        // directory the resume lookup below reads from, and — via `set_core` — every later
        // flush, eject and polaroid read for this cart too. Deriving it twice let a `gpsp` cart
        // run on mGBA with its state filed under `States/gpsp/` — the two calls always agreed
        // in practice, right up until `open_core` did not yet know `Core` existed. `App` stores
        // this rather than re-deriving it later, which is what makes that class of drift
        // structurally unreachable now instead of merely unobserved.
        // gpSP does the Game Boy Advance and nothing else, so anything that is not one has
        // exactly one core that can take it, whatever the card's own file says — including a
        // line somebody wrote into it by hand. Everything downstream reads the core from the
        // variable above, so clamping it here is all it takes to keep a `.gb` off gpSP.
        let core = match system {
            System::Gba => slot_store::core_for(&self.root, stem),
            System::Gb | System::Gbc => Core::Mgba,
            System::Sfc => Core::Snes,
            System::Fc => Core::Nes,
            // Both arcade families run FBNeo. One core, two `System` variants: the two differ
            // only in how the picture lands on the panel (NeoGeo integer-scaled, the rest
            // stretched to 4:3), never in which dylib runs.
            System::Neogeo | System::Arcade => Core::Fbneo,
            // ★ 2026-10-25：MD 与 PS1。**两者都是"一个机种一个核心"**，所以不像 GBA 那样
            //   去问 `core_for` —— 卡上 `selected_core.ini` 里那一行对它们没有意义，
            //   问它只会让一张手改过的卡把 PS1 指到 mGBA 上。
            //
            //   `genesis_plus_gx` 还覆盖 Master System / Game Gear / SG-1000，机种由 rom
            //   自己决定；`pcsx_rearmed` 只做 PlayStation。
            System::Md => Core::Gpgx,
            System::Ps1 => Core::Pcsx,
            System::Pce => Core::PceFast,
        };
        self.app.set_core(core);
        // An SGB-enhanced GB/GBC cart would draw mGBA's Super Game Boy border, which SLOT
        // already covers with its own bezel — so it is disabled at core open (see
        // `open_core_for`). Note it here so a hand debugging "why is this cart framed
        // differently" can see the real cause in the log.
        if matches!(system, System::Gb | System::Gbc) && slot_store::sgb_enhanced(&rom) {
            eprintln!("slot: {stem} is SGB-enhanced; mGBA Super Game Boy border disabled");
        }
        // A clean start skips the state, it does not delete it: the file stays on the card
        // for the next tap to resume from.
        // ★ 2026-10-28：「插卡续玩」关掉时，插入 / 开机不回读 resume（当作全新开局）；但仍尊重
        //   `starting_clean()` —— 手动选了「重开本局 / 干净开局」也照样不读。两项任一为假都不读。
        let resume = (!self.app.starting_clean() && self.app.insert_resume())
            .then(|| persist::read_resume(&self.root, core, system, stem))
            .flatten();
        // ★★ 2026-10-25：**同名 `.bios` 优先。** 汉化/改版常常要自己那版 BIOS，而 pcsx 的
        //   选项只有 `auto`/`HLE`（不认文件名），它是在 `retro_init` 里扫 `system` 目录的 ——
        //   所以"用这一个"只能靠"让那一次 init 只看得见它"（见 `open_core_for_rom`）。
        let emu = EmuHandle::spawn(
            open_core(&self.root, core, system, &rom),
            rom,
            self.sink.ring(),
            persist::load_sav(&self.root, system, stem),
            resume,
            self.audio,
        );
        // A cart seated after the level was lowered has to start there, not at full.
        emu.set_volume(self.app.output_volume());
        self.app.set_snapshot(Box::new(emu.snapshot()));
        self.emu = Some(emu);
        // Load this cart's cheat list off the card into `App`, then push it to the core. Codes
        // default to OFF (see `App::set_cheats`), so a cart that ships codes launches with
        // nothing applied; the player opens the table (SELECT+A) and switches on the ones they
        // want. SELECT+A while the table is open flips the master `cheats_on` and re-pushes.
        self.app.set_cheats(crate::root::cheats(&self.root, stem));
        self.push_cheats();
        // The screen-overlay set for the cart's system (`Overlay/gb*.png` / `gbc*.png` /
        // `sfc*.png` / `fc*.png`), with the persisted rotation index applied. `overlay_ring`
        // is the one place that knows which slot a machine owns and what its files are
        // called. The Advance has no overlay set at all, so it clears whatever the previous
        // cart left on screen — a Game Boy's model art (or a GBC's frame) must not bleed
        // through into an Advance game, and there is no key on this machine that would turn
        // it off. The frontend mints the GL texture when it next sees `overlay_dirty`.
        let Some((slot, which)) = crate::root::overlay_ring(system) else {
            // No overlay set for this system: drop the previous cart's overlay.
            eprintln!("slot: overlay: {system:?} has no set — clearing");
            self.app.clear_overlay_set();
            return;
        };
        let list = crate::root::overlay_files(&self.root, which);
        // Say what the card actually offered. Art the scanner did not take — a name it does
        // not recognise — is otherwise invisible: the device has no console, and the overlay
        // leaves no other trace anywhere on screen or off.
        let names: Vec<String> = list
            .iter()
            .map(|o| {
                format!(
                    "{}{}",
                    o.path.file_name().unwrap_or_default().to_string_lossy(),
                    if o.reflect { " (reflect)" } else { "" }
                )
            })
            .collect();
        eprintln!(
            "slot: overlay {which}: {} file(s): {}",
            list.len(),
            if names.is_empty() {
                "(none - check Overlay/ naming)".to_string()
            } else {
                names.join(", ")
            }
        );
        self.app.set_overlay_set(list, slot);
    }

    /// (Re)send the live cheat codes to the core. "Live" is `App::live_cheats`: a code switched off,
    /// or every code under a master that is off, is **not sent at all**.
    ///
    /// mGBA's `retro_cheat_set` marks `enabled` as `UNUSED(enabled)` and adds every code it
    /// receives to its cheat table, so a `false` entry sent to it is a code switched on. That is
    /// why the filtering happens here and why `cheat_reset` in the worker is the only way a code
    /// ever comes off the core. Cheap either way: a handful of calls, and a no-op for a core
    /// without cheats (gpSP exports no `retro_cheat_set`).
    fn push_cheats(&mut self) {
        if let Some(emu) = &self.emu {
            emu.set_cheats(self.app.live_cheats());
        }
    }
}

/// `SLOT_TRACE=1` prints every semantic action and the phase it landed in. The one thing
/// the tests cannot cover is whether a key reaches the window at all, so this is how that
/// question gets answered without guessing at the platform.
pub(crate) fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SLOT_TRACE").is_some())
}
