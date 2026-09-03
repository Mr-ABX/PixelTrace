# 📋 Product Requirements Document (PRD)
# PixelTrace — Ultra-Lightweight Screen Annotator & Laser Pointer

**Version:** 1.0.0-draft  
**Author:** Antigravity Engineering & Product  
**Target Platforms:** macOS (Apple Silicon & Intel) & Windows 10/11 (x64 & ARM64)  
**License:** Open Source / MIT  

---

## 1. Executive Summary & Vision

### 1.1 Problem Statement
During presentations, remote demos (Zoom, Microsoft Teams, Google Meet), code reviews, and video recordings, speakers need to point, highlight, draw shapes, and direct attention to arbitrary UI elements across multiple applications. Existing tools suffer from several major flaws:
- **Heavy Bloat:** Electron-based tools (like Pensela) consume 200MB+ RAM and 150MB+ download sizes.
- **Platform Locks:** ZoomIt is Windows-only; ScreenBrush is macOS-only and commercial.
- **Intrusive UIs:** Most tools feature clunky toolbars that permanently obstruct screen sharing or don't allow interacting with underlying apps while annotations remain visible ("Ghost / Click-Through Mode").

### 1.2 The PixelTrace Solution
**PixelTrace** is an ultra-lightweight, cross-platform (macOS & Windows) desktop utility built with **Tauri v2**, **Rust**, and **Svelte 5 / HTML5 Canvas**. 
- **Sub-3MB binary**, **<35MB idle RAM**, and **0% idle CPU**.
- Features an ultra-clean **Floating Glassmorphic Widget** that docks and collapses into a sleek **Edge Drawer/Pill** when moved or hidden.
- Delivers a buttery 60/120fps **decaying laser pointer trail**, freehand drawing, highlighter, geometric shapes, and numbered callout stamps.
- Supports **Click-Through Mode ("Ghost Mode")** so presenters can click, scroll, and type in underlying apps while annotations stay pinned.
- Resides seamlessly in the **macOS Menu Bar** and **Windows System Tray**.

---

## 2. GitHub & Open-Source Benchmark Analysis

Before designing the architecture, we researched existing leading tools and open-source repositories to adopt the best patterns and avoid known traps:

| Project | Stack | Strengths | Limitations | Key Takeaways for PixelTrace |
| :--- | :--- | :--- | :--- | :--- |
| **[MarkerOn](https://github.com/ifer47/markeron)** | Tauri v2 + Vue 3 + Canvas | ~1.5MB installer, 11 tools, click-through (`X`), system tray, multi-monitor | Toolbar is relatively basic and lacks smooth edge docking/drawer animation | Proof of concept that Tauri v2 + Canvas delivers best-in-class performance. We can build upon its click-through and toolset foundation. |
| **ScreenBrush** (Mac proprietary) | Swift / AppKit | Best-in-class UX: edge-snapping drawer, decaying flash/laser, spotlight, ghost mode | Mac only, closed source, paid license | We adopt its **docking edge drawer** and **ghost mode** paradigms as the gold standard for presenter ergonomics. |
| **[gInk](https://github.com/geovens/gInk)** | C# / .NET Win32 | Great floating pen dock, magnetic snapping to borders, very responsive on Windows | Windows only, dated WinForms/WPF aesthetic | Magnetic edge snapping is beloved by teachers and presenters. |
| **[Wayscriber](https://github.com/devmobasa/wayscriber)** | Rust (Linux/Wayland) | ZoomIt-style live overlay, zero bloat, instant key controls | Linux/Wayland only | Shows how Rust provides rock-solid performance with zero background overhead. |
| **[Epic Pen](https://epicpen.com)** | Native / C++ | Collapsible vertical toolbar that minimizes to a screen edge handle | Freemium, commercial restrictions | Collapsing the toolbar into a low-profile edge tab prevents blocking presentation content. |

---

## 3. Product Goals & Performance Guardrails

| Metric | Target Goal | Verification Method |
| :--- | :--- | :--- |
| **Binary / Installer Size** | `< 4.0 MB` (Compressed) | Release build artifact check |
| **Idle Memory Usage** | `< 35 MB` RSS | OS Activity Monitor / Task Manager |
| **Active Drawing RAM** | `< 60 MB` RSS | Memory profile under active stroke buffer |
| **Idle CPU Utilization** | `0.0%` (Event-driven sleep) | Profiler verification (no active RAF loops when idle) |
| **Active Draw Frame Rate** | `60 - 120 FPS` (VSync locked) | Canvas `requestAnimationFrame` delta tracking |
| **Cold Startup Time** | `< 400 ms` | Benchmark timing from launch to tray initialization |
| **Cross-Platform Parity** | 100% feature match on Mac & Win | CI builds on `macos-latest` & `windows-latest` |

---

## 4. System Architecture & Core Engineering

```
┌────────────────────────────────────────────────────────────────────────┐
│                              SYSTEM LAYER                              │
│   • macOS Menu Bar (NSStatusBar)   • Windows System Tray (Shell_Notify)│
│   • Global Hotkey Service (tauri-plugin-global-shortcut)               │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                         Tauri IPC Event Bus
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                         RUST CORE ENGINE (Tauri v2)                    │
│  ├── window_manager.rs   : Multi-monitor enumeration & overlay bounds  │
│  ├── input_hook.rs       : Click-through toggle (setIgnoreCursorEvents)│
│  ├── hotkeys.rs          : Global shortcut dispatcher                  │
│  ├── tray.rs             : Menu bar / Tray icon & contextual actions   │
│  └── config.rs           : JSON settings persistence                   │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                         Webview Bridge (Zero-Copy)
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        FRONTEND UI (Svelte 5)                          │
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │  FLOATING WIDGET LAYER (Z-Index: 999999)                       │   │
│   │  • Glassmorphic Floating Pill / Toolbar                        │   │
│   │  • Edge-Docking Drawer Handle (Magnetic Snap to L/R/T/B)       │   │
│   │  • Color Picker, Stroke Slider, Tool Selectors                 │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                   │                                    │
│   ┌───────────────────────────────┴────────────────────────────────┐   │
│   │  HARDWARE-ACCELERATED CANVAS LAYER                             │   │
│   │  • Laser Trail Engine (Particle/polyline queue + decay)        │   │
│   │  • Vector Stroke Engine (Pen, Highlighter with blend modes)    │   │
│   │  • Shape Renderer (Arrow, Rect, Circle, Line, Text, Stamp)     │   │
│   │  • Undo / Redo History Stack                                   │   │
│   └────────────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Windowing & Click-Through Strategy
A screen annotation app must solve the **Click-Through Paradox**:
1. When drawing, the overlay must intercept all mouse clicks.
2. When the user wants to click on a website or slide, clicks must pass right through to apps below.
3. However, even in click-through mode, the **floating toolbar must remain clickable**.

#### The PixelTrace Implementation Strategy:
- **Intelligent Cursor Hit-Testing:**
  - The transparent overlay covers the active screen.
  - When the user's cursor hovers over the **Floating Widget / Edge Tab**, the frontend signals Tauri via `window.setIgnoreCursorEvents(false)` so the toolbar buttons are instantly interactive.
  - When in **Click-Through Mode ("Ghost Mode")** and the cursor is over empty canvas space, Tauri executes `window.setIgnoreCursorEvents(true, { forward: true })`. Mouse clicks fall through directly into Zoom, PowerPoint, IDE, or browsers.
  - When in **Drawing Mode**, `setIgnoreCursorEvents(false)` is active across the entire canvas.

### 4.2 Multi-Monitor Strategy
In multi-display setups, presenters frequently switch between laptop screens and external presentation monitors:
- **Monitor Detection:** Rust core queries `app.available_monitors()`.
- **Dynamic Monitor Tracking (Cursor Follower):**
  - Instead of running 3 distinct WebViews (which triples RAM usage), PixelTrace tracks which monitor the cursor enters when annotation mode is summoned.
  - The overlay instantly relocates/resizes to the active monitor (`window.set_position` & `window.set_size`).
  - *Optionally for multi-screen simultaneous drawing:* An overlay pool spawns a transparent window per monitor on demand with minimal memory footprints.

---

## 5. UI / UX Design: The Floating Edge-Docking Widget

### 5.1 Design Aesthetic
- **Glassmorphism:** Frosted glass blur backdrop (`backdrop-filter: blur(16px)`), subtle 1px border (`rgba(255,255,255,0.12)`), dark mode / light mode adaptive.
- **Micro-animations:** Smooth CSS cubic-bezier springs (`cubic-bezier(0.16, 1, 0.3, 1)`) for expanding, collapsing, and tool switching.
- **Ergonomic Layout:** Compact horizontal or vertical pill design.

### 5.2 Widget States

```
┌─────────────────────────────────────────────────────────┐
│ State 1: Floating Mode (Freely draggable anywhere)      │
│ [ ⋮⋮ | 🔴 Laser | ✏️ Pen | 🖍️ High | ⬛ Shapes | ↩️ ↪️ | ✕ ] │
└─────────────────────────────────────────────────────────┘
                            │
            Dragged near screen edge (< 40px)
            OR Clicked "Collapse" chevron
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│ State 2: Docked Edge Tab / Drawer Handle                │
│ Snaps flush against screen border:                      │
│                                                         │
│ [Screen Left Edge]                                      │
│ ▌▌ [⚡ PT]  <-- Sleek 14px grabber pill with subtle glow│
│                                                         │
│ Hover or Click -> Expands back with fluid spring drawer │
└─────────────────────────────────────────────────────────┘
```

#### 1. Floating Pill Mode (Default)
- A sleek, floating control bar with a textured drag handle `⋮⋮`.
- Can be dragged anywhere on the screen.
- Shows active tool, stroke width indicator, color preview, and quick-action icons.

#### 2. Magnetic Edge Snapping & Docked Drawer Mode
- When dragged within **40px** of any screen edge (Left, Right, Top, or Bottom), subtle magnetic haptic/visual feedback triggers.
- Letting go docks the bar to that edge.
- Clicking the collapse arrow or double-clicking the drag handle collapses the widget into a minimalist **Edge Pill / Tab** (width ~18px) that sits flush on the border.
- **Re-expansion:** Hovering over the edge tab or single-clicking it slides the full toolbar drawer smoothly out.

#### 3. Minimal HUD / Stealth Mode
- Pressing `Space` or `H` toggles visibility of the toolbar entirely, allowing 100% distraction-free presentation.

---

## 6. Detailed Feature Specifications

### 6.1 Tool Suite

#### 1. 🔴 Laser Pointer (with Fading Trail)
- **Visuals:** High-intensity glowing circular dot (outer glow + solid core).
- **Decay Trail:** As the pointer moves, it generates a polyline trail with timestamped points.
- **Decay Engine:** Points fade out smoothly over a configurable duration (default: **600ms**, adjustable 300ms–1500ms).
- **Zero Idle Overhead:** If the cursor stops moving and all trail points reach 0 alpha, the canvas `requestAnimationFrame` loop completely suspends.
- **Click-Through:** Works in Click-Through mode so presenters can guide viewers while navigating slides.

#### 2. ✏️ Freehand Pen
- Smooth anti-aliased Bézier curve smoothing (eliminates jagged mouse jitter).
- Adjustable stroke width (2px to 24px) via mouse wheel or slider.
- Opacity control (100% solid down to subtle marks).

#### 3. 🖍️ Highlighter
- Semi-transparent, wide rectangular or rounded brush stroke.
- Blend mode set to `multiply` or `source-over` with 40% alpha, ensuring text and charts underneath remain legible.
- Preset fluorescent colors: Neon Yellow, Electric Green, Coral Pink, Cyan Blue.

#### 4. 📐 Geometric Shapes & Callouts
- **Arrow:** Smart directional arrow with head geometry auto-calculated from start to end vector. Perfect for pointing out bugs or specific data cells.
- **Rectangle / Box:** Hollow border or semi-transparent fill. Hold `Shift` to constrain to perfect square.
- **Ellipse / Circle:** Oval or circle callouts. Hold `Shift` to constrain to 1:1 circle.
- **Line:** Straight line with angle snapping (0°, 45°, 90°) when holding `Shift`.
- **Numbered Callout Stamps (1, 2, 3...):** Click anywhere to place sequential numbered circular badges (`①`, `②`, `③`). Crucial for explaining multi-step workflows during demos. Auto-increments with each click.

#### 5. 🔤 Text Tool
- Click anywhere to create a floating inline editable text box.
- Crisp font rendering with contrasting outline or backdrop banner for readability on any background.

### 6.2 Canvas Management & History
- **Undo / Redo Stack:** Full vector stroke history (`Ctrl+Z` / `Cmd+Z`, `Ctrl+Y` / `Cmd+Shift+Z`).
- **Clear All:** Instantly clear screen (`Esc` or Trash icon).
- **Eraser Tool:** Stroke eraser (clicks a stroke to delete it) or area eraser brush.
- **Auto-Fade Mode (Optional):** All drawings automatically fade away after X seconds (useful for transient live markup).
- **Save / Copy Screenshot:** One-click button to copy current annotated screen area to clipboard or save as PNG.

### 6.3 Interaction Modes
1. **Annotate Mode (Active):** Canvas captures mouse input; drawing tools active.
2. **Ghost / Click-Through Mode (`X` key toggle):** Drawings remain visible on screen, but mouse clicks pass through to underlying applications (PowerPoint, Zoom, Chrome, VS Code). The floating toolbar remains clickable.
3. **Whiteboard / Blackboard Mode:** Toggles a solid white, dark slate, or frosted glass background for a clean sketching board.

### 6.4 System Integration: Menu Bar & System Tray

#### macOS Status Bar Item:
- Monochrome adaptive icon in top menu bar.
- Dropdown menu:
  - Toggle Annotation Overlay (`Cmd + Shift + D`)
  - Tool Selection submenu
  - Clear Screen (`Cmd + K`)
  - Preferences / Hotkeys...
  - Check for Updates
  - Quit PixelTrace (`Cmd + Q`)

#### Windows System Tray Item:
- Taskbar notification area icon.
- Right-click context menu with identical actions and settings.
- Left-click toggles annotation mode instantly.

---

## 7. Global Keyboard Shortcuts (Default Scheme)

All shortcuts are globally registered in Rust via `tauri-plugin-global-shortcut` and work even when other apps have focus:

| Action | macOS Shortcut | Windows Shortcut | Configurable? |
| :--- | :--- | :--- | :--- |
| **Toggle Annotation Overlay** | `Cmd + Shift + D` | `Ctrl + Shift + D` | Yes |
| **Toggle Click-Through ("Ghost Mode")** | `X` | `X` | Yes |
| **Toggle Toolbar Visibility** | `Space` | `Space` | Yes |
| **Laser Pointer Tool** | `L` or `1` | `L` or `1` | Yes |
| **Pen Tool** | `P` or `2` | `P` or `2` | Yes |
| **Highlighter Tool** | `H` or `3` | `H` or `3` | Yes |
| **Arrow Tool** | `A` or `4` | `A` or `4` | Yes |
| **Rectangle Tool** | `R` or `5` | `R` or `5` | Yes |
| **Circle Tool** | `C` or `6` | `C` or `6` | Yes |
| **Numbered Stamp Tool** | `S` or `7` | `S` or `7` | Yes |
| **Undo / Redo** | `Cmd + Z` / `Cmd + Shift + Z` | `Ctrl + Z` / `Ctrl + Y` | Yes |
| **Clear All Annotations** | `Esc` or `Cmd + Delete` | `Esc` or `Ctrl + Delete` | Yes |
| **Cycle Colors** | `Q` (Prev) / `W` (Next) | `Q` (Prev) / `W` (Next) | Yes |

---

## 8. Technical Architecture & Implementation Plan

### 8.1 Rust Backend (`src-tauri`)
- **Tauri Core v2.x**
- Plugins:
  - `tauri-plugin-global-shortcut`: For responsive, system-wide key listeners.
  - `tauri-plugin-tray`: System tray and menu bar management.
  - `tauri-plugin-store`: Lightweight persistent JSON storage for user settings.
- Platform Specifics:
  - **macOS:** Objective-C / Cocoa bridge (`objc`) to configure `NSWindow` level to `kCGScreenSaverWindowLevel` (or `NSWindow.Level.popUpMenu`), ensuring the overlay floats above full-screen Keynote/Zoom presentations.
  - **Windows:** Win32 API calls (`SetWindowLongPtr` with `WS_EX_LAYERED`, `WS_EX_TRANSPARENT`, and `HWND_TOPMOST`).

### 8.2 Frontend (`src`)
- **Framework:** Svelte 5 (using runes `$state`, `$derived`, `$effect`) for reactive, zero-overhead state management.
- **Styling:** Modern Tailwind CSS v4 or streamlined modern CSS tokens with glassmorphic styling.
- **Rendering Engine:** Dedicated 2D HTML5 Canvas engine:
  - Quadratic curve smoothing for pen strokes.
  - Offscreen canvas caching for static strokes to ensure laser pointer repaints only redraw the dynamic tail layer.
  - High DPI / Retina display scaling via `window.devicePixelRatio`.

---

## 9. Phased Execution Roadmap

### Phase 1: Project Scaffolding & Core Overlay Window
- [ ] Initialize Tauri v2 + Svelte 5 + TypeScript workspace in project directory.
- [ ] Configure `tauri.conf.json` for transparent, frameless, always-on-top window.
- [ ] Implement Rust backend commands: `set_click_through(bool)`, `get_monitors()`, global shortcut listeners.
- [ ] Implement macOS menu bar item and Windows system tray icon.

### Phase 2: Canvas Drawing & Laser Engine
- [ ] High-DPI canvas setup with dynamic resize listeners.
- [ ] Laser pointer engine with real-time velocity-aware trailing decay queue.
- [ ] Freehand pen with spline interpolation.
- [ ] Highlighter with `multiply` blend mode.
- [ ] Undo / Redo vector history stack.

### Phase 3: Shape Tools & Numbered Stamps
- [ ] Shape preview and draw logic: Arrow, Rectangle, Ellipse, Straight Line.
- [ ] Sequential numbered callout stamp tool (`①, ②, ③...`).
- [ ] Text annotation tool with inline editing.

### Phase 4: Floating Edge-Docking Widget UI
- [ ] Glassmorphic floating control bar with tool selection, color palette, and width slider.
- [ ] Magnetic edge-snapping physics (collapsing to edge pill when near borders).
- [ ] Fluid drawer slide-out animation on hover or click.
- [ ] Mouse hit-test boundary handling for click-through mode.

### Phase 5: Multi-Monitor, Packaging & Release
- [ ] Multi-monitor detection and active screen relocation.
- [ ] Keyboard shortcut customization & settings drawer.
- [ ] macOS `.dmg` / `.app` bundle & Windows `.msi` / portable `.zip` build configurations.
- [ ] Performance profiling: verification of `< 35MB` idle RAM and `0%` idle CPU.

---

*Document approved for implementation.*
