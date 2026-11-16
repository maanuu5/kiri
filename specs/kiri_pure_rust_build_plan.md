# Kiri (切り) — Full Pure Rust Build Plan

A lightweight, zero-bloat Windows desktop app written in **Pure Rust**. It runs silently in your system tray, detects when you are doomscrolling YouTube Shorts or Instagram Reels, and spawns a pixelated cat directly on your browser's tab bar to walk smoothly over and swat/cut the `X` button closed.

## 0. Project Naming (Minimal Japanese Shortlist)

**Active Working Name:** **Kiri** (`kiri.exe`) — short, sharp, minimal, and easy to type. If you prefer a different vibe, swap in any finalist below before `cargo init`:

| Name | Japanese | Meaning / Nuance | Vibe | 
| ----- | ----- | ----- | ----- | 
| **Kiri** *(Active)* | 切り / 斬り | *"Cut"* / *"slice"* | Ultra-minimal, crisp Rust micro-tool energy. | 
| **Batsu** | バツ | Literally the `X` (cross/close) mark | Clever double meaning—the cat hunts the `X` button. | 
| **Choki** | チョキ | Onomatopoeia for scissors snipping | Playful, funky, fits the pixel-scissors tab cut. | 
| **Saku** | サクッ | A clean, quick snip or effortless cut | Breezy, modern, lightweight (`saku.exe`). | 
| **Tachi** | 断ち | To sever or break off a bad habit | Subtle, zen, productivity-focused. | 
| **Pachi** | パチッ | A crisp light *snap* or *tap* | Cute, tactile, fits a cat paw swat. | 

## 1. Core Design Pillars

* **True Micro-Tool Footprint:** Single standalone `.exe` (\~2–4 MB), under 10 MB RAM, and **true 0.0% idle CPU** (the UI thread sleeps completely in `GetMessageW` whenever no cat is on screen).

* **Zero Browser Extensions:** Uses Windows UI Automation (UIA) to inspect Chromium-based browsers natively without injecting scripts or requiring extension store installs.

* **Non-Intrusive & Race-Safe Tab Closing:** Instead of hijacking your mouse cursor, the cat visually swats/snips the `X` button while UIA programmatically invokes that exact tab's `Close` button on the frame of impact—guarded against race conditions so it can never close a work tab.

* **Buttery Motion, Crisp Pixels:** Decoupled 60 FPS window movement with 8–10 FPS pixel-art frame stepping, real-time `GetWindowRect` anchoring, integer nearest-neighbor scaling, and Per-Monitor V2 DPI awareness.

## 2. Tech Stack & Lean Crate Dependencies

Every dependency is chosen to keep compile times fast and the final binary tiny—no WebView, no Electron, no heavy audio/image codec bloat.

| Crate | Role & Lean Configuration | 
| ----- | ----- | 
| `windows` | Win32 layered windows (`UpdateLayeredWindow`), real-time window tracking (`GetWindowRect`, `GetForegroundWindow`), high-res waitable timers, DPI awareness, named single-instance mutex, native audio (`PlaySoundW`), and Registry startup key. | 
| `uiautomation` | Ergonomic wrapper over Windows UI Automation COM APIs (MTA thread) to inspect Chrome/Edge/Brave/Vivaldi tab strips, read URLs, and invoke tab Close buttons. | 
| `png` *(or `image` with `default-features = false, features = ["png"]`)* | Decodes only the embedded PNG sprite sheet at startup—avoids compiling unused JPEG/AVIF/WebP codecs. | 
| `tray-icon` + `muda` | Native Windows system tray icon and right-click context menu. | 
| `serde` + `toml` | Config, customizable URL rules, and daily stats stored in `%APPDATA%\Kiri\config.toml`. | 
| `winresource` *(build-dep)* | Embeds `app.manifest` (PerMonitorV2 DPI awareness) and `tray_icon.ico` into the `.exe` resource table at compile time. | 

> **Why no `rodio` for audio?** `rodio` pulls in `cpal` and audio decoders just to play two 10 KB `.wav` files. Win32's native `PlaySoundW` with `SND_MEMORY | SND_ASYNC` plays embedded `.wav` byte slices with **zero extra crates**, zero idle audio handles, and automatic Windows audio-sleep compliance.

## 3. System Architecture (Zero-Idle-Wakeup Dual Thread)

COM / UI Automation calls can occasionally take 10–40ms if a browser is busy. To guarantee zero animation stutter *and* zero battery drain while idle, work is strictly partitioned across two threads:

```
+-----------------------------------------------------------------------+
|                        WINDOWS OS / BROWSER                           |
|  (Chrome / Edge / Brave / Vivaldi — UIA Tree + Live HWND Geometry)    |
+-----------------------------------+-----------------------------------+
                                    | Polls every ~400ms (Dedicated COM MTA Thread)
                                    v
+-----------------------------------------------------------------------+
|                    THREAD 2: WATCHER & DETECTOR                       |
|  - Initializes COM as COINIT_MULTITHREADED (all UIA objects stay here)|
|  - Filters foreground HWND by allowlisted browser executables         |
|  - Reads URL (structural lookup, language-agnostic)                   |
|  - Evaluates Dual Trigger: Unique URL Slugs OR Continuous Dwell Time  |
|  - Extracts Tab Strip RECT, Orientation, Active Tab & Close Btn RECT  |
|  - Pushes `WatcherEvent` to mpsc AND calls `PostMessageW` to wake UI  |
|  - Handles `WatcherCommand::ExecuteSwat` with strict safety checks    |
+-----------------------------------+-----------------------------------+
                                    | Lock-free mpsc + WM_APP_WAKE signal
                                    v
+-----------------------------------------------------------------------+
|                  THREAD 1: MAIN / UI & ANIMATION                      |
|  - Acquires Single-Instance Win32 Named Mutex                         |
|  - IDLE STATE (`Hidden`): Blocks in `GetMessageW` -> 0.0% CPU!        |
|  - ACTIVE STATE: Starts 60 FPS high-res timer; tracks live browser    |
|    `GetWindowRect(target_hwnd)` every frame (zero 400ms drag lag!)    |
|  - Runs Cat FSM, relative-offset steering, gravity & frame hooks      |
|  - Blits premultiplied BGRA frames via `UpdateLayeredWindow`          |
|  - Manages System Tray, Global Snooze Hotkey & clean exit teardown    |
+-----------------------------------------------------------------------+

```

## 4. Project Directory Structure

```
kiri/
├── Cargo.toml
├── build.rs                 # Embeds app.manifest + tray_icon.ico via winresource
├── app.manifest             # Declares PerMonitorV2 DPI awareness
├── assets/
│   ├── cat_sheet.png        # 512x32 PNG (16 frames of 32x32, embedded via include_bytes!)
│   ├── tray_icon.ico        # 16x16 & 32x32 pixel cat icon
│   └── sfx/
│       ├── meow.wav         # Tiny 8-bit PCM WAV (optional subtle alert)
│       └── bap.wav          # Tiny 8-bit PCM WAV (snip/swat impact sound)
└── src/
    ├── main.rs              # Mutex lock, logging setup, thread spawn, Win32 event loop
    ├── config.rs            # TOML config, custom URL rules, daily stats, startup toggle
    ├── logger.rs            # Rolling file logger (%APPDATA%\Kiri\kiri.log)
    ├── watcher.rs           # COM MTA UIA scanner, dual-trigger detector, safe tab closer
    ├── cat_fsm.rs           # Cat behavior FSM, relative window anchoring & physics
    ├── animator.rs          # Decoupled frame-timing engine, clip definitions & event hooks
    ├── overlay.rs           # Win32 WS_EX_LAYERED window & UpdateLayeredWindow blit
    ├── sprite.rs            # PNG slicing, nearest-neighbor scale, flip & premult BGRA
    ├── audio.rs             # Thin safe wrapper over Win32 PlaySoundW (SND_MEMORY)
    └── tray.rs              # System tray icon, menu actions & clean shutdown disposal

```

## 5. Deep Dive: Core Technical Mechanics

### 5.1 Browser & Doomscroll Detection (`watcher.rs`)

1. **Foreground & Fullscreen Gate:**

   * Call `GetForegroundWindow()` and check the owning executable name (`chrome.exe`, `msedge.exe`, `brave.exe`, `vivaldi.exe`). Skip all UIA work if another app is focused.

   * Compare `GetWindowRect(hwnd)` against `GetMonitorInfoW` (`rcMonitor`). If the browser is in true fullscreen (e.g., watching a full-screen lecture or movie, where Chrome hides its tab bar anyway), pause detection automatically.

   * *Firefox Note:* Chromium browsers share a unified UIA tree and are supported on Day 1; Firefox uses a separate UIA hierarchy and is scoped as a clean adapter in v2.

2. **Language-Agnostic UIA Discovery (No Brittle English Strings!):**

   * *Why:* Hardcoding `"Address and search bar"` or `"Close"` breaks on non-English Windows/Chrome installs and across Chrome UI updates.

   * **Address Bar:** Locate the `ControlType::Edit` inside the browser's toolbar hierarchy that supports `ValuePattern` (or `AutomationId` / `AccessKey` containing `L`). Cache it per `HWND`, and immediately invalidate/re-query on any COM error (e.g., browser restart or tab tear-off).

   * **URL Normalization:** Chrome's address bar `ValuePattern` often omits `https://` and `www.` (e.g., returning `youtube.com/shorts/abc123`). Normalize by lowercasing and stripping any leading scheme/www prefix before matching against `config.toml` rules.

3. **Dual-Trigger Detection (Fixes Instagram Reels & Looping-Short Edge Cases):**

   * *Why URL-slug counting alone isn't enough:* On Instagram Web, `/reels/` sometimes keeps the base URL or updates slowly, and on YouTube Shorts a user can stare at a looping 60-second Short without swiping.

   * **Trigger A — Unique Slug Swipe Counter:** Every *new* URL matching a rule (e.g., `youtube.com/shorts/*`, `instagram.com/reels/*`) adds its slug to a session `HashSet` and increments `swipe_count`.

   * **Trigger B — Continuous Dwell Timer:** Tracks continuous active time spent on any matching doomscroll URL (`dwell_secs`).

   * **Escalation Rule:** Whichever threshold hits first advances the strike level (e.g., Strike 1 at `2 swipes OR 20s`, Strike 2 at `4 swipes OR 45s`, Strike 3 at `5 swipes OR 75s`).

   * **Cooldown Decay:** Both counters reset cleanly after a configurable grace period (default: `120s`) spent away from matching URLs.

### 5.2 Tab Bar Anchoring, Maximized/Vertical/Pinned Tabs & Safe Closing (`watcher.rs` + `cat_fsm.rs`)

1. **Real-Time 60 FPS Window Tracking (Eliminates 400ms Drag Lag):**

   * Instead of moving the cat using stale screen coordinates from the 400ms UIA poll, `watcher.rs` converts the tab strip and `Close` button coordinates into **window-relative offsets** (`rel_x = screen_x - browser_rect.left`, `rel_y = screen_y - browser_rect.top`).

   * On every 60 FPS tick, Thread 1 calls `GetWindowRect(target_hwnd)` and `GetForegroundWindow()` (under $1\ \mu\text{s}$):

     * If the user drags Chrome gently, the cat stays glued to the tab bar with **zero lag**.

     * If the window moves violently (exceeds a drag velocity threshold, e.g., $> 900\ \text{px/sec}$), minimizes (`IsIconic`), or closes, the cat immediately detaches into the `Falling` state.

     * If the user `Alt+Tab`s to another app (`GetForegroundWindow() != target_hwnd`), the cat immediately aborts (`Hidden` or `ConfusedAbort`) so a `WS_EX_TOPMOST` cat never floats awkwardly on top of VS Code or a game.

2. **Maximized Window Ledge Switching (`y = 0` Fix):**

   * When Chrome is **windowed** (`tab_strip.top >= scaled_sprite_height`), the cat perches right on top of the tab bar (`y = tab_strip.top - scaled_height + foot_overlap`).

   * When Chrome is **maximized** (`tab_strip.top < scaled_sprite_height` or `IsZoomed(hwnd)`), placing the cat above the strip would clip it off the top of the monitor. Automatically switch the walking ledge to the **lower tab-bar edge** (walking across the toolbar/address-bar divider right beneath the tabs) so the cat stays 100% visible while its paw still reaches directly onto the `X` button.

3. **Active Tab, Pinned Tabs & Vertical Tabs:**

   * **Vertical Tabs Guard (Edge / Brave / Vivaldi):** Check `tab_strip.width >= tab_strip.height`. If the user has Vertical Tabs enabled, skip top-bar patrolling and perch the cat along the top edge of the browser toolbar/viewport instead, targeting the active tab's `Close` button if visible.

   * **Language-Agnostic Close Button:** Inside the active `ControlType::TabItem` (`SelectionItemPattern::is_selected() == true`), find its child `ControlType::Button` (the `X` button is typically the sole button child of a Chromium `TabItem`; verify via bounding box on the right edge of the tab).

   * **Pinned Tabs Guard:** Pinned tabs have no `Close` button (`close_btn == None`). If `config.allow_close_pinned` is `false` (default), the cat walks over and plays a scolding `Nudge` instead of killing a pinned tab.

4. **Race-Safe Kill Switch (`ExecuteSwat`):**

   * **Primary Path:** Call `InvokePattern::invoke()` on the *cached UIA reference to that exact tab's `Close` button*. Even if the user switched tabs 5ms earlier, invoking that specific element either closes only the Shorts/Reels tab or returns a harmless COM error—never closing the wrong tab.

   * **Strict `Ctrl+W` Fallback (Only if `InvokePattern` fails):** Before synthesizing `Ctrl+W` via `SendInput`, Thread 2 **must** re-verify at the exact instant of execution:

     1. `GetForegroundWindow() == target_hwnd`

     2. The live Address Bar URL *still* matches a doomscroll rule.

     3. No modifier keys (`VK_CONTROL`, `VK_MENU`, `VK_SHIFT`) are currently held down by the user (`GetAsyncKeyState`).

### 5.3 The "Tiny Moving Window" Overlay (`overlay.rs`)

Create a tiny borderless window sized strictly to the integer-scaled sprite (e.g., $64 \times 64$ at $2\times$, $96 \times 96$ at $3\times$).

* **Extended Window Styles:**

  * `WS_EX_LAYERED` — Enables per-pixel alpha transparency.

  * `WS_EX_TRANSPARENT` — 100% click-through so the cat never eats clicks on your tabs or browser controls. *(Because it is click-through, snoozing/mercy uses a Global Hotkey or System Tray rather than clicking the cat).*

  * `WS_EX_TOPMOST` — Keeps the cat visible above the browser chrome.

  * `WS_EX_TOOLWINDOW` — Hides the overlay from the Windows taskbar and `Alt+Tab`.

  * `WS_EX_NOACTIVATE` — Guarantees spawning or moving the cat never steals keyboard focus.

* **Flicker-Free Blitting (`UpdateLayeredWindow`):**

  * Pre-convert every decoded sprite frame into **premultiplied-alpha BGRA** at startup (and on DPI change):
    

    $$
    B' = \lfloor B \cdot A / 255 \rfloor, \quad G' = \lfloor G \cdot A / 255 \rfloor, \quad R' = \lfloor R \cdot A / 255 \rfloor
    $$

  * Blit directly into a cached 32-bit DIB section (`CreateDIBSection`) and call `UpdateLayeredWindow`—atomically updating both pixels and signed `(i32, i32)` screen coordinates (supporting monitors positioned to the left/above the primary monitor with negative coordinates).

## 6. Animation Engine, Smooth Walking & Tab-Cutting Sequence

### 6.1 Decoupled 60 FPS Smooth Walking (`cat_fsm.rs` & `animator.rs`)

To prevent choppy movement or frantic leg-spinning ("moonwalking"), **decouple window motion from sprite frame stepping**:

* **Window Motion (60 FPS via `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`):**

  * Store coordinates as `f32` (`pos_x`, `pos_y`) for sub-pixel smoothness, rounding to `i32` only when calling `UpdateLayeredWindow`.

  * Clamp `dt = raw_dt.min(0.05)` so sleep/wake spikes never launch the cat off-screen.

  * Timer runs **only** while state $\neq$ `Hidden`; stopping it returns CPU usage to `0.0%`.

* **Sprite Playback (8–10 FPS):**

  * Advance walk frames independently via a `frame_timer: f32` accumulator (`100ms` per frame).

* **Stride-to-Speed Matching:**

  * Set walk speed to `120.0 * scale` px/sec (tunable to match your sprite's exact pixel stride) so the cat's paws look planted on the tab bar.

* **Ease-Out Arrival Steering:**

  * Within `arrival_radius = 28.0 * scale` of the target `X` button, smoothly damp velocity:
    

    $$
    v = v_{\max} \cdot \max\left(0.25,\ \frac{\vert x_{\text{target}} - x \vert}{\text{arrival\_radius}}\right)
    $$

* **Zero-Cost Direction Flip:**

  * Pre-compute horizontally mirrored premultiplied-BGRA buffers for every frame at startup in `sprite.rs`.

### 6.2 Tab-Cutting Paw Animation & Impact-Frame Sync

Bind the UIA tab-close command to a **Frame Event Hook** on the exact impact frame so the tab disappears the millisecond the paw/scissors strike `X`:

| Sequence Step | Sprite Frame | Duration | Visual Action (Paw Swat or Pixel Scissors) | Engine Hook | 
| ----- | ----- | ----- | ----- | ----- | 
| **1. Alert** | `8` | 400ms | Ears perked, tiny pixel `!` overhead. | Optional soft `meow.wav`. | 
| **2. Wind-Up** | `9` | 120ms | Cat halts beside `X` and raises paw (or draws tiny pixel scissors). | Align window via `PAW_OFFSET` so paw tip targets `(target_x, target_y)`. | 
| **3. Anticipation Hold** | `10` | 180ms | Brief menacing mid-air pause—lets you see you're busted. | Re-verify active tab is still doomscrolling; if user switched tabs, abort to `Confused` (`14`). | 
| **4. IMPACT (The Cut/Bap)** | `11` | 80ms | Paw slams down / scissors snip across `X` with a bright 1-frame pixel slash flash. | **Fires `WatcherCommand::ExecuteSwat` + plays `bap.wav` on this exact frame.** | 
| **5. Follow-Through** | `12` | 220ms | Paw rests down with residual snip spark; tab vanishes underneath. | Increment daily `"Tabs cut"` counter. | 
| **6. Smug Exit** | `13` | 450ms | Cat sits back and licks its paw smugly, then trots off-screen. | Transition to `SmugExit` $\rightarrow$ `Hidden`. | 

* **Paw-Tip Anchor Math:**

  * Keep the cat body \~18–20px wide inside the $32 \times 32$ cell so there is ample horizontal reach for the extended paw/scissors + slash VFX without clipping the frame edge.

  * Define `PAW_OFFSET_RIGHT = (26, 22)` (in unscaled $32 \times 32$ coordinates; also allow configurable `frame_width`/`frame_height` in `sprite.rs`).

  * Facing Right: `window_x = target_x - (PAW_OFFSET_RIGHT.0 * scale)`.

  * Facing Left: `window_x = target_x - ((frame_width - PAW_OFFSET_RIGHT.0) * scale)`.

### 6.3 Master Sprite Sheet Layout (`assets/cat_sheet.png`)

Single horizontal PNG strip on a $32 \times 32$ **pixel** grid (**16 frames total** = clean power-of-two $512 \times 32$ **PNG**):

| Frame Index | Clip Name | Timing | Description | 
| ----- | ----- | ----- | ----- | 
| `0–1` | `Idle` | 450ms / frame | Sitting on the tab ledge, slow tail flick. | 
| `2–7` | `Walk` | 100ms / frame | Smooth 6-frame trot cycle (pre-mirrored for left/right). | 
| `8` | `Alert` | 400ms | Ears perked up, tiny pixel `!` overhead. | 
| `9` | `WindUp` | 120ms | Raising paw / pulling out tiny pixel scissors. | 
| `10` | `Anticipate` | 180ms | Paw poised right over the `X` button. | 
| `11` | `Impact` | 80ms | Downward *bap* / snip + bright 1-frame pixel slash VFX *(triggers tab close)*. | 
| `12` | `FollowThrough` | 220ms | Paw landed, satisfied squint, fading spark. | 
| `13` | `SmugLick` | 450ms | Smug paw-lick before walking away. | 
| `14` | `Confused` | 450ms | Head tilt + tiny pixel `?` (used when user escapes to another tab). | 
| `15` | `Falling` | Loop | Startled splayed paws during gravity fall to the taskbar. | 

### 6.4 Integer Nearest-Neighbor Scaling & Live DPI Changes

* Query `GetDpiForWindow(browser_hwnd)` when spawning or when the browser crosses monitors (`WM_DPICHANGED` / DPI check on spawn).

* Compute an exact integer multiplier (`base_scale = 2` at 100% DPI $\rightarrow$ $64 \times 64$, scaling to $3\times \rightarrow 96 \times 96$ at 150%+ DPI).

* Rebuild cached premultiplied-BGRA DIB frames only when `scale` changes, using strict nearest-neighbor pixel replication.

## 7. Cat State Machine (FSM) & 3-Strike Escalation

1. **`Hidden`:** Overlay hidden (`SW_HIDE`), 60 FPS timer stopped (**0.0% CPU**).

2. **`Peeking` (Strike 1):** Cat hops onto the tab bar edge, plays `Idle`, and stares silently.

3. **`Patrolling` (Strike 2):** Cat walks smoothly back and forth along the tab bar (`Walk`), occasionally pausing (`Idle`).

4. **`Alert` (Strike 3):** Cat stops, plays `Alert` (`!`) for `400ms`, then enters `WalkingToTarget`.

5. **`WalkingToTarget`:** Cat trots with ease-out steering to align `PAW_OFFSET` over the active tab's `X` button.

6. **`SwatSequence`:** Plays `WindUp` $\rightarrow$ `Anticipate` $\rightarrow$ `Impact` (fires `ExecuteSwat` if action mode is `Swat`, or just hisses/swats visually in `NudgeOnly` mode) $\rightarrow$ `FollowThrough`.

7. **`SmugExit`:** Plays `SmugLick`, then trots to the nearest edge of the browser window and returns to `Hidden`.

8. **`ConfusedAbort` (Escape Attempt):** If the user switches to a safe tab during `WalkingToTarget` or `Anticipate`, cat stops, plays `Confused` (`?`), and trots off-screen.

9. **`Falling` (Window Drag/Minimize Interrupt):** If the browser is minimized, closed, or violently dragged while visible, cat plays `Falling` with gravity ($v_y \leftarrow v_y + g \cdot dt$) down to the monitor's work-area bottom (`rcWork.bottom`, right on top of the Windows taskbar), lands briefly, and walks off-screen.

## 8. Configuration, UX & System Hygiene

### 8.1 Configurable `config.toml` (`%APPDATA%\Kiri\config.toml`)

* **Customizable Target Rules:** Defaults to `["youtube.com/shorts/", "instagram.com/reels"]`, with optional user-editable entries (e.g., `tiktok.com`, `reddit.com`, `x.com`) so you can expand what the cat hunts without recompiling.

* **Action Mode:** `Swat` (closes tab) vs. `NudgeOnly` (full animation + sound, doesn't close tab—great for testing or softer reminders).

* **Dual Limits:** Configurable `strike_1/2/3_swipes`, `strike_1/2/3_secs`, and `cooldown_secs`.

* **Audio & Startup:** `sound_enabled: bool`, `start_with_windows: bool`.

* **Daily Stats:** `date`, `tabs_cut_today`, `total_tabs_cut` (resets daily counter cleanly at local midnight).

### 8.2 System Tray, Global Hotkey & Observability

* **Single-Instance Lock:** Win32 Named Mutex (`Local\Kiri_SingleInstance_Mutex`). Second launch exits silently.

* **Global Mercy Hotkey (`RegisterHotKey`):** Optional shortcut (e.g., `Ctrl+Shift+Alt+K`) to snooze Kiri for 15 minutes without opening the tray menu—keeps the cat overlay 100% click-through.

* **System Tray Menu (`tray.rs`):**

  * Header (disabled): `Tabs Cut Today: 7 (~28m saved)`

  * `Pause for 15 Minutes` (Snooze toggle)

  * `Mode`: `Swat Tab` | `Nudge Only`

  * `Trigger Limit`: `Strict (3 Swipes / 45s)` | `Normal (5 Swipes / 75s)` | `Chill (10 Swipes / 120s)`

  * `Sound Effects` (Checkmark)

  * `Start with Windows` (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`)

  * `Open Config & Logs Folder` (Essential for tweaking URL rules or inspecting debug logs)

  * `Quit` (Explicitly drops `TrayIcon` before `PostQuitMessage(0)` so no ghost icon lingers)

* **File Logging (`logger.rs`):** Because release builds hide the console (`windows_subsystem = "windows"`), write warnings, COM recovery events, and panics (via `std::panic::set_hook`) to a size-capped `%APPDATA%\Kiri\kiri.log`.

## 9. Phased Implementation Roadmap

### Phase 1: UIA Inspector CLI (Proof of Concept)

* **Goal:** Prove language-agnostic URL reading, dual-trigger detection, maximized/vertical/pinned tab detection, and race-safe tab closing.

* **Tasks:**

  * Set up `Cargo.toml` with `uiautomation` and `windows`.

  * Run a dedicated COM MTA watcher thread polling foreground Chromium windows every 400ms with cache invalidation.

  * Implement URL normalization + Dual Trigger (unique slug counter + dwell timer).

  * Print window-relative Tab Strip `RECT`, orientation, Maximized status, and Active Tab `Close` button coordinates; test `InvokePattern`.

* **Milestone:** Scrolling through 3 YouTube Shorts or dwelling on Instagram Reels reliably invokes the exact tab's `Close` button from Rust without touching the mouse.

### Phase 2: Transparent Pixel Sprite & 60 FPS Smooth Walking Engine

* **Goal:** Render a crisp, click-through pixel cat gliding at 60 FPS with decoupled 10 FPS leg frames and 0% idle CPU.

* **Tasks:**

  * Add `app.manifest` + `build.rs` (`winresource`) for Per-Monitor V2 DPI awareness.

  * Build `overlay.rs` (`UpdateLayeredWindow` + DIB section + `WM_APP_WAKE` + high-res waitable timer).

  * Build `sprite.rs` (`png` decoding of $512 \times 32$ sheet, nearest-neighbor scaling, pre-mirrored premultiplied BGRA frames).

  * Build `animator.rs` with clip playback and Impact-frame event hooks; verify smooth walking, ease-out stops, and swat playback.

* **Milestone:** A razor-sharp, click-through pixel cat walks smoothly across the desktop, decelerates naturally, and plays its full paw-swat sequence with zero flicker.

### Phase 3: Connect the Brain (FSM + Real-Time Window Tracking)

* **Goal:** Glue the cat to live browser windows at 60 FPS and sync the impact frame to tab closing.

* **Tasks:**

  * Wire bidirectional `mpsc` channels + `PostMessageW` wakeup between `watcher.rs` and `main.rs`.

  * Implement `cat_fsm.rs` with 60 FPS `GetWindowRect` anchoring, maximized-window ledge switching, `PAW_OFFSET` alignment, `ConfusedAbort`, `Alt+Tab` dismissal, and `Falling` gravity physics.

  * Connect frame `11` (`Impact`) to `WatcherCommand::ExecuteSwat` + `PlaySoundW("bap.wav")`.

* **Milestone:** The cat tracks your Chrome window smoothly even while dragging, trots to the Shorts/Reels tab, hesitates, and snips it closed on the exact impact frame.

### Phase 4: System Tray, Config, Logging & Hygiene

* **Goal:** Turn the prototype into a daily-driver Windows background app.

* **Tasks:**

  * Add Single-Instance Named Mutex, panic hook, and rolling `kiri.log`.

  * Integrate `tray-icon` + `muda` menu, `config.toml` persistence, customizable URL rules, `NudgeOnly` mode, and Global Snooze Hotkey.

  * Add fullscreen auto-pause, native `PlaySoundW` SFX, and Registry startup toggle.

* **Milestone:** App starts silently with Windows, uses 0% idle CPU, and cleanly shuts down with no ghost tray icon.

### Phase 5: Release Optimization & Packaging

* **Goal:** Produce a tiny, clean standalone `.exe`.

* **Tasks:**

  * Enable `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` in `main.rs`.

  * Configure `Cargo.toml` release profile:

    ```
    [profile.release]
    opt-level = "z"
    lto = true
    codegen-units = 1
    strip = true
    panic = "abort"
    
    ```

  * *SmartScreen / Antivirus Note:* Unsigned Windows binaries using `SendInput` or startup Registry keys can occasionally trigger Defender heuristics. Keeping `InvokePattern` as primary (never using keyboard hooks) and keeping the binary clean reduces false positives; for public distribution later, code-signing or a GitHub Releases build pipeline is recommended.

## 10. Architectural Audit: Key Risks & Mitigations

| Risk / Question | How This Plan Solves It | 
| ----- | ----- | 
| **Doesn't a 60 FPS loop drain laptop battery all day?** | **Solved:** Thread 1 sleeps completely inside `GetMessageW` while `Hidden` (`0.0%` CPU). The 60 FPS high-res timer only spins up when the cat is actually visible. | 
| **Won't a 400ms UIA poll make the cat lag behind when moving Chrome?** | **Solved:** UIA provides window-*relative* offsets; Thread 1 polls `GetWindowRect(hwnd)` at 60 FPS ($< 1\ \mu\text{s}$) so the cat sticks to the browser smoothly—and triggers `Falling` on rapid drags. | 
| **What if Chrome isn't in English or changes button names?** | **Solved:** Uses structural UIA discovery (`ControlType::Edit` with `ValuePattern`, child `ControlType::Button` inside selected `TabItem`) instead of brittle English-only string matching. | 
| **What if Instagram Reels doesn't change the URL slug on every swipe?** | **Solved:** Dual-Trigger model triggers on *either* unique URL slugs **or** continuous dwell time. | 
| **What if I `Alt+Tab` right before the swat? Will it close my work tab?** | **Solved:** Primary close invokes the exact cached `Close` button UIA element; `Ctrl+W` fallback re-verifies foreground `HWND`, live URL match, and unpressed modifier keys at the instant of execution. | 
| **What if I use Vertical Tabs or Pinned Tabs?** | **Solved:** Detects vertical tab strips (`width < height`) and missing Close buttons on pinned tabs, falling back safely without clipping or crashing. | 

## 11. Verification & Testing Checklist

* \[ \] **Zero-Idle Footprint:** With no Shorts/Reels open, Task Manager shows **0.0% CPU** and **< 12 MB RAM**.

* \[ \] **Single-Link Mercy:** Watching 1 short clip sent by a friend does *not* close the tab.

* \[ \] **Dual Trigger:** Both rapid swiping (5 Shorts) *and* lingering on a single looping Reel reliably escalate Strikes $1 \rightarrow 2 \rightarrow 3$.

* \[ \] **Smooth Walking & Tracking:** Cat glides at 60 FPS without moonwalking, stays glued during gentle window drags, and eases to a stop beside `X`.

* \[ \] **Impact-Frame Sync:** The tab vanishes on Frame `11` (`Impact`) simultaneously with the pixel slash and `bap.wav`—never early or late.

* \[ \] **Maximized Browser (`y = 0`):** Maximizing Chrome switches the cat to the lower tab-bar ledge without clipping off-screen.

* \[ \] **Escape & `Alt+Tab` Safety:** Switching tabs during `Anticipate` triggers `Confused` (`?`); `Alt+Tab`bing to another app immediately dismisses the cat without closing anything.

* \[ \] **Edge Layouts:** Test with **Pinned Tabs**, **Edge Vertical Tabs**, **Fullscreen YouTube video**, and **Multi-Monitor** setups (100%/125%/150% DPI + left-hand monitor with negative `x` coordinates).

* \[ \] **System Hygiene:** Double-launching does not spawn a second instance; `Quit` removes the tray icon immediately; crashes/UIA errors log cleanly to `%APPDATA%\Kiri\kiri.log`.