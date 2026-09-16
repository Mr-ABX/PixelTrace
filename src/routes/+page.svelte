<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    Sparkles,
    Pencil,
    Highlighter,
    ArrowUpRight,
    Square,
    Circle,
    Slash,
    Type,
    ListOrdered,
    Eraser,
    StickyNote,
    Undo2,
    Redo2,
    Camera,
    Hourglass,
    Trash2,
    Ghost,
    Keyboard,
    Settings,
    Sliders,
    Check,
    GripVertical,
    Monitor,
    ChevronLeft,
    ChevronRight,
    X
  } from "lucide-svelte";

  // Tool types
  type Tool = "laser" | "pen" | "highlighter" | "arrow" | "rect" | "circle" | "line" | "stamp" | "text" | "eraser" | "sticky";

  interface Point {
    x: number;
    y: number;
    time?: number;
  }

  interface LaserPoint {
    x: number;
    y: number;
    time: number;
  }

  interface DrawItem {
    id: string;
    tool: Tool;
    color: string;
    size: number;
    points?: Point[];
    start?: Point;
    end?: Point;
    stampNumber?: number;
    text?: string;
    textPoint?: Point;
    createdAt?: number;
  }

  interface MonitorInfo {
    name: string | null;
    width: number;
    height: number;
    x: number;
    y: number;
    scale_factor: number;
  }

  interface InteractiveRect {
    x: number;
    y: number;
    width: number;
    height: number;
  }

  // Reactive application state
  let currentTool = $state<Tool>("laser");
  let currentColor = $state("#FF3B30"); // Default neon red
  let currentSize = $state(4); // Default stroke size
  let isGhostMode = $state(false); // Click-through state
  let stampCounter = $state(1);
  let ghostNotification = $state<string | null>(null);
  let ghostNotifyTimer: number | null = null;

  // Auto-Fade Mode
  let autoFadeEnabled = $state(false);
  const AUTO_FADE_MS = 3500;

  // Multi-Monitor state
  let monitors = $state<MonitorInfo[]>([]);
  let activeMonitorIndex = $state(0);

  // History for Undo/Redo
  let history = $state<DrawItem[]>([]);
  let redoStack = $state<DrawItem[]>([]);

  // Text Tool State
  let isTextActive = $state(false);
  let textPos = $state({ x: 0, y: 0 });
  let textInput = $state("");
  let textInputRef = $state<HTMLInputElement | null>(null);

  // Sticky Notes Interface & State
  interface StickyNoteItem {
    id: string;
    x: number;
    y: number;
    width: number;
    height: number;
    content: string;
    color: string;
    headerColor: string;
    textColor: string;
    borderColor: string;
    isCollapsed?: boolean;
  }

  const stickyColors = [
    { name: "Canary Yellow", bg: "#FEF08A", header: "#FDE047", text: "#713F12", border: "#EAB308" },
    { name: "Sakura Pink", bg: "#FBCFE8", header: "#F472B6", text: "#831843", border: "#EC4899" },
    { name: "Mint Green", bg: "#BBF7D0", header: "#4ADE80", text: "#14532D", border: "#22C55E" },
    { name: "Sky Blue", bg: "#BAE6FD", header: "#38BDF8", text: "#0C4A6E", border: "#0284C7" },
    { name: "Lavender", bg: "#DDD6FE", header: "#A78BFA", text: "#4C1D95", border: "#8B5CF6" }
  ];

  let stickyNotes = $state<StickyNoteItem[]>([]);
  let activeStickyDragId = $state<string | null>(null);
  let stickyDragOffset = { x: 0, y: 0 };

  // Eraser State
  let eraserCursor = $state({ x: -100, y: -100, visible: false });

  // Settings / Help modal & Preferences
  let showHelpModal = $state(false);
  let modalActiveTab = $state<"shortcuts" | "preferences" | "about">("shortcuts");

  // Canvas references
  let staticCanvas: HTMLCanvasElement;
  let dynamicCanvas: HTMLCanvasElement;
  let staticCtx: CanvasRenderingContext2D;
  let dynamicCtx: CanvasRenderingContext2D;

  // Drawing state
  let isDrawing = false;
  let startPoint = { x: 0, y: 0 };
  let currentStrokePoints: Point[] = [];

  // Continuous Laser Pointer Engine
  let laserPoints: LaserPoint[] = [];
  let laserAnimFrame: number | null = null;
  let laserCursor = $state({ x: -100, y: -100, visible: false, isPressed: false });
  let laserLifetimeMs = $state(850);

  // Floating Widget positioning, snapping & edge docking
  let toolbarRef = $state<HTMLDivElement | null>(null);
  let collapsedTabRef = $state<HTMLButtonElement | null>(null);
  let widgetX = $state(100);
  let widgetY = $state(20);
  let isDragging = false;
  let dragOffset = { x: 0, y: 0 };
  let isDocked = $state(false);
  let dockSide = $state<"left" | "right">("right");
  let isCollapsed = $state(false);
  let lastFloatingX = $state(100);
  let lastFloatingY = $state(20);

  // Popover Picker states & Tooltips
  let showColorPicker = $state(false);
  let showSizePicker = $state(false);
  let colorPickerRef = $state<HTMLDivElement | null>(null);
  let sizePickerRef = $state<HTMLDivElement | null>(null);
  let hoveredTooltip = $state<{ label: string; shortcut?: string; x: number; y: number } | null>(null);

  // Stroke Size Presets
  const sizePresets = [
    { label: "Fine", val: 2 },
    { label: "Med", val: 4 },
    { label: "Thick", val: 8 },
    { label: "Heavy", val: 14 }
  ];

  // Curated MarkerOn / Excalidraw Color Palette
  const colors = [
    { name: "Neon Red", hex: "#FF3B30" },
    { name: "Electric Orange", hex: "#FF9500" },
    { name: "Bright Yellow", hex: "#FFCC00" },
    { name: "Neon Green", hex: "#34C759" },
    { name: "Electric Cyan", hex: "#00C7BE" },
    { name: "Apple Blue", hex: "#007AFF" },
    { name: "Vivid Purple", hex: "#AF52DE" },
    { name: "Jet Black", hex: "#1E293B" },
    { name: "Pure White", hex: "#FFFFFF" }
  ];

  onMount(() => {
    initCanvases();
    centerToolbar();

    window.addEventListener("resize", handleResize);
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("pointerdown", handleWindowClick);

    try {
      const savedNotes = localStorage.getItem("pixeltrace_sticky_notes");
      if (savedNotes) {
        stickyNotes = JSON.parse(savedNotes);
      }
      const savedTool = localStorage.getItem("pixeltrace_pref_tool");
      if (savedTool) currentTool = savedTool as Tool;
      const savedSize = localStorage.getItem("pixeltrace_pref_size");
      if (savedSize) currentSize = Number(savedSize);
      const savedColor = localStorage.getItem("pixeltrace_pref_color");
      if (savedColor) currentColor = savedColor;
      const savedFade = localStorage.getItem("pixeltrace_pref_autofade");
      if (savedFade !== null) autoFadeEnabled = savedFade === "true";
      const savedLaserDur = localStorage.getItem("pixeltrace_pref_laser_duration");
      if (savedLaserDur) laserLifetimeMs = Number(savedLaserDur);
    } catch (e) {
      console.warn("Could not load stored preferences:", e);
    }

    let unlistenGhost: UnlistenFn | undefined;
    let unlistenClear: UnlistenFn | undefined;
    let unlistenPrefs: UnlistenFn | undefined;

    (async () => {
      try {
        const res = await invoke<MonitorInfo[]>("get_monitors");
        if (Array.isArray(res)) {
          monitors = res;
        }
      } catch (e) {
        console.warn("Could not fetch monitors:", e);
      }

      try {
        unlistenGhost = await listen<boolean>("ghost-mode-changed", (event) => {
          isGhostMode = event.payload;
          triggerGhostToast(isGhostMode);
        });

        unlistenClear = await listen("clear-canvas", () => {
          clearCanvas();
        });

        unlistenPrefs = await listen("open-preferences", () => {
          openPreferencesModal("preferences");
        });
      } catch (e) {
        console.warn("Could not attach event listeners:", e);
      }
    })();

    return () => {
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("pointerdown", handleWindowClick);
      if (laserAnimFrame) cancelAnimationFrame(laserAnimFrame);
      if (unlistenGhost) unlistenGhost();
      if (unlistenClear) unlistenClear();
      if (unlistenPrefs) unlistenPrefs();
    };
  });

  async function centerToolbar() {
    await tick();
    const w = window.innerWidth;
    const bar = toolbarRef?.querySelector(".glass-bar") as HTMLElement | null;
    const width = bar ? bar.offsetWidth : (toolbarRef ? toolbarRef.offsetWidth : 940);
    widgetX = Math.max(16, Math.min(w - width - 16, Math.round((w - width) / 2)));
    widgetY = 24;
    updateInteractiveRects();
  }

  function handleResize() {
    initCanvases();
    if (!isCollapsed) {
      centerToolbar();
    } else {
      widgetX = 0;
      updateInteractiveRects();
    }
  }

  function initCanvases() {
    if (!staticCanvas || !dynamicCanvas) return;
    const dpr = window.devicePixelRatio || 1;
    const w = window.innerWidth;
    const h = window.innerHeight;

    staticCanvas.width = w * dpr;
    staticCanvas.height = h * dpr;
    dynamicCanvas.width = w * dpr;
    dynamicCanvas.height = h * dpr;

    staticCtx = staticCanvas.getContext("2d")!;
    dynamicCtx = dynamicCanvas.getContext("2d")!;

    staticCtx.scale(dpr, dpr);
    dynamicCtx.scale(dpr, dpr);

    redrawStaticCanvas();
  }

  function triggerGhostToast(active: boolean) {
    if (ghostNotifyTimer) clearTimeout(ghostNotifyTimer);
    ghostNotification = active
      ? "Ghost Mode Active — Clicks pass through to background apps (⌘⇧X / ⌘⇧G to return)"
      : "Drawing Mode Active — Annotations enabled";
    ghostNotifyTimer = window.setTimeout(() => {
      ghostNotification = null;
    }, 3200);
  }

  function triggerToast(msg: string) {
    if (ghostNotifyTimer) clearTimeout(ghostNotifyTimer);
    ghostNotification = msg;
    ghostNotifyTimer = window.setTimeout(() => {
      ghostNotification = null;
    }, 3200);
  }

  // --- CONTINUOUS GLOWING LASER POINTER ENGINE (BUTTERY SMOOTH SPLINE RIBBON) ---
  function renderLaserLoop(now: number) {
    if (!dynamicCtx) return;

    // Prune expired laser points
    laserPoints = laserPoints.filter((p) => now - p.time < laserLifetimeMs);

    // Only clear dynamicCtx if we are using the laser or if trail points remain
    if (currentTool === "laser" || laserPoints.length > 0) {
      dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    }

    const n = laserPoints.length;

    // Render continuous tapered glowing beam ribbon
    if (n >= 2) {
      const rgb = hexToRgb(currentColor);

      // Compute normal vectors and left/right polygon boundaries
      const leftOuter: Point[] = [];
      const rightOuter: Point[] = [];
      const leftCore: Point[] = [];
      const rightCore: Point[] = [];

      for (let i = 0; i < n; i++) {
        const p = laserPoints[i];
        const age = now - p.time;
        const progress = Math.max(0, Math.min(1, 1 - age / laserLifetimeMs));
        const outerRadius = Math.max(1, (currentSize * 2.8 + 4) * Math.pow(progress, 1.15));
        const coreRadius = Math.max(0.5, (currentSize * 1.2 + 2) * Math.pow(progress, 1.1));

        let dx = 0;
        let dy = 0;
        if (i === 0) {
          dx = laserPoints[1].x - laserPoints[0].x;
          dy = laserPoints[1].y - laserPoints[0].y;
        } else if (i === n - 1) {
          dx = laserPoints[n - 1].x - laserPoints[n - 2].x;
          dy = laserPoints[n - 1].y - laserPoints[n - 2].y;
        } else {
          dx = laserPoints[i + 1].x - laserPoints[i - 1].x;
          dy = laserPoints[i + 1].y - laserPoints[i - 1].y;
        }

        const len = Math.hypot(dx, dy) || 1;
        const nx = -dy / len;
        const ny = dx / len;

        leftOuter.push({ x: p.x + nx * outerRadius, y: p.y + ny * outerRadius });
        rightOuter.push({ x: p.x - nx * outerRadius, y: p.y - ny * outerRadius });
        leftCore.push({ x: p.x + nx * coreRadius, y: p.y + ny * coreRadius });
        rightCore.push({ x: p.x - nx * coreRadius, y: p.y - ny * coreRadius });
      }

      const tailP = laserPoints[0];
      const headP = laserPoints[n - 1];

      // Pass 1: Wide Ambient Bloom Polygon Ribbon
      dynamicCtx.save();
      dynamicCtx.beginPath();
      dynamicCtx.moveTo(leftOuter[0].x, leftOuter[0].y);
      for (let i = 0; i < n - 1; i++) {
        const midX = (leftOuter[i].x + leftOuter[i + 1].x) / 2;
        const midY = (leftOuter[i].y + leftOuter[i + 1].y) / 2;
        dynamicCtx.quadraticCurveTo(leftOuter[i].x, leftOuter[i].y, midX, midY);
      }
      dynamicCtx.lineTo(headP.x, headP.y);
      for (let i = n - 1; i > 0; i--) {
        const midX = (rightOuter[i].x + rightOuter[i - 1].x) / 2;
        const midY = (rightOuter[i].y + rightOuter[i - 1].y) / 2;
        dynamicCtx.quadraticCurveTo(rightOuter[i].x, rightOuter[i].y, midX, midY);
      }
      dynamicCtx.lineTo(tailP.x, tailP.y);
      dynamicCtx.closePath();

      const bloomGrad = dynamicCtx.createLinearGradient(tailP.x, tailP.y, headP.x, headP.y);
      bloomGrad.addColorStop(0, `rgba(${rgb}, 0.0)`);
      bloomGrad.addColorStop(0.3, `rgba(${rgb}, 0.25)`);
      bloomGrad.addColorStop(0.7, `rgba(${rgb}, 0.6)`);
      bloomGrad.addColorStop(1, `rgba(${rgb}, 0.85)`);

      dynamicCtx.fillStyle = bloomGrad;
      dynamicCtx.shadowColor = currentColor;
      dynamicCtx.shadowBlur = 18;
      dynamicCtx.fill();
      dynamicCtx.restore();

      // Pass 2: Saturated Neon Core Ribbon
      dynamicCtx.save();
      dynamicCtx.beginPath();
      dynamicCtx.moveTo(leftCore[0].x, leftCore[0].y);
      for (let i = 0; i < n - 1; i++) {
        const midX = (leftCore[i].x + leftCore[i + 1].x) / 2;
        const midY = (leftCore[i].y + leftCore[i + 1].y) / 2;
        dynamicCtx.quadraticCurveTo(leftCore[i].x, leftCore[i].y, midX, midY);
      }
      dynamicCtx.lineTo(headP.x, headP.y);
      for (let i = n - 1; i > 0; i--) {
        const midX = (rightCore[i].x + rightCore[i - 1].x) / 2;
        const midY = (rightCore[i].y + rightCore[i - 1].y) / 2;
        dynamicCtx.quadraticCurveTo(rightCore[i].x, rightCore[i].y, midX, midY);
      }
      dynamicCtx.lineTo(tailP.x, tailP.y);
      dynamicCtx.closePath();

      const coreGrad = dynamicCtx.createLinearGradient(tailP.x, tailP.y, headP.x, headP.y);
      coreGrad.addColorStop(0, `rgba(${rgb}, 0.0)`);
      coreGrad.addColorStop(0.35, `rgba(${rgb}, 0.5)`);
      coreGrad.addColorStop(1, `rgba(${rgb}, 0.95)`);

      dynamicCtx.fillStyle = coreGrad;
      dynamicCtx.shadowColor = currentColor;
      dynamicCtx.shadowBlur = 8;
      dynamicCtx.fill();
      dynamicCtx.restore();

      // Pass 3: White-Hot Center Spine Spline
      dynamicCtx.save();
      dynamicCtx.beginPath();
      dynamicCtx.moveTo(laserPoints[0].x, laserPoints[0].y);
      for (let i = 0; i < n - 1; i++) {
        const midX = (laserPoints[i].x + laserPoints[i + 1].x) / 2;
        const midY = (laserPoints[i].y + laserPoints[i + 1].y) / 2;
        dynamicCtx.quadraticCurveTo(laserPoints[i].x, laserPoints[i].y, midX, midY);
      }
      dynamicCtx.lineTo(headP.x, headP.y);

      const spineGrad = dynamicCtx.createLinearGradient(tailP.x, tailP.y, headP.x, headP.y);
      spineGrad.addColorStop(0, "rgba(255, 255, 255, 0.0)");
      spineGrad.addColorStop(0.4, "rgba(255, 255, 255, 0.45)");
      spineGrad.addColorStop(1, "rgba(255, 255, 255, 0.95)");

      dynamicCtx.strokeStyle = spineGrad;
      dynamicCtx.lineWidth = Math.max(1, currentSize * 0.45);
      dynamicCtx.lineCap = "round";
      dynamicCtx.lineJoin = "round";
      dynamicCtx.shadowColor = "#FFFFFF";
      dynamicCtx.shadowBlur = 4;
      dynamicCtx.stroke();
      dynamicCtx.restore();
    }

    // Render Laser Pointer Head / Cursor Orb when tool is laser and cursor is visible
    if (currentTool === "laser" && laserCursor.visible) {
      dynamicCtx.save();
      const headX = laserCursor.x;
      const headY = laserCursor.y;
      const pulse = Math.sin(now * 0.012) * 2;
      const baseR = currentSize + 3;

      // Outer Aura Ring
      dynamicCtx.beginPath();
      dynamicCtx.arc(headX, headY, baseR + 5 + pulse, 0, Math.PI * 2);
      dynamicCtx.strokeStyle = `rgba(${hexToRgb(currentColor)}, 0.65)`;
      dynamicCtx.lineWidth = 1.8;
      dynamicCtx.shadowColor = currentColor;
      dynamicCtx.shadowBlur = 14;
      dynamicCtx.stroke();

      // Glowing Center Orb
      dynamicCtx.beginPath();
      dynamicCtx.arc(headX, headY, baseR, 0, Math.PI * 2);
      dynamicCtx.fillStyle = currentColor;
      dynamicCtx.shadowColor = currentColor;
      dynamicCtx.shadowBlur = 20;
      dynamicCtx.fill();

      // White Hot Spark
      dynamicCtx.beginPath();
      dynamicCtx.arc(headX, headY, Math.max(2, baseR * 0.42), 0, Math.PI * 2);
      dynamicCtx.fillStyle = "#FFFFFF";
      dynamicCtx.shadowColor = "#FFFFFF";
      dynamicCtx.shadowBlur = 6;
      dynamicCtx.fill();

      dynamicCtx.restore();
    }

    // Continue loop while laser points decay or while laser tool is active with visible cursor
    if (laserPoints.length > 0 || (currentTool === "laser" && laserCursor.visible)) {
      laserAnimFrame = requestAnimationFrame(renderLaserLoop);
    } else {
      laserAnimFrame = null;
    }
  }

  function hexToRgb(hex: string): string {
    const clean = hex.replace("#", "");
    const bigint = parseInt(clean, 16);
    if (isNaN(bigint)) return "255, 59, 48";
    const r = (bigint >> 16) & 255;
    const g = (bigint >> 8) & 255;
    const b = bigint & 255;
    return `${r}, ${g}, ${b}`;
  }

  // --- SMOOTH FREEHAND DRAWING HELPER ---
  function drawSmoothStroke(
    ctx: CanvasRenderingContext2D,
    points: Point[],
    color: string,
    size: number,
    isHighlighter: boolean = false
  ) {
    if (!points || points.length === 0) return;

    ctx.save();
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    if (isHighlighter) {
      ctx.strokeStyle = color;
      ctx.lineWidth = size * 3.5;
      ctx.globalAlpha = 0.35;
      ctx.globalCompositeOperation = "source-over";
    } else {
      ctx.strokeStyle = color;
      ctx.lineWidth = size;
      ctx.globalAlpha = 1.0;
    }

    if (points.length === 1) {
      // Clean single dot
      ctx.beginPath();
      ctx.arc(points[0].x, points[0].y, (isHighlighter ? size * 3.5 : size) / 2, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
    } else if (points.length === 2) {
      // Clean 2-point line
      ctx.beginPath();
      ctx.moveTo(points[0].x, points[0].y);
      ctx.lineTo(points[1].x, points[1].y);
      ctx.stroke();
    } else {
      // C1 continuous midpoint quadratic spline
      ctx.beginPath();
      ctx.moveTo(points[0].x, points[0].y);
      ctx.lineTo((points[0].x + points[1].x) / 2, (points[0].y + points[1].y) / 2);

      for (let i = 1; i < points.length - 1; i++) {
        const midX = (points[i].x + points[i + 1].x) / 2;
        const midY = (points[i].y + points[i + 1].y) / 2;
        ctx.quadraticCurveTo(points[i].x, points[i].y, midX, midY);
      }

      const last = points[points.length - 1];
      ctx.lineTo(last.x, last.y);
      ctx.stroke();
    }

    ctx.restore();
  }

  // --- MOUSE & DRAWING HANDLERS ---
  function onPointerDown(e: PointerEvent) {
    if (isGhostMode) return;
    const target = e.target as HTMLElement;
    if (target && (target.closest(".widget-container") || target.closest(".inline-text-box") || target.closest(".modal-backdrop") || target.closest(".sticky-note"))) return;

    if (currentTool === "sticky") {
      addStickyNote(e.clientX, e.clientY);
      isDrawing = false;
      return;
    }

    if (currentTool === "eraser") {
      isDrawing = true;
      eraseAt(e.clientX, e.clientY);
      renderEraserPreview();
      return;
    }

    if (currentTool === "text") {
      commitText();
      textPos = { x: e.clientX, y: e.clientY };
      textInput = "";
      isTextActive = true;
      tick().then(() => {
        textInputRef?.focus();
      });
      return;
    }

    if (isTextActive) {
      commitText();
    }

    isDrawing = true;
    startPoint = { x: e.clientX, y: e.clientY };

    if (currentTool === "laser") {
      laserCursor = { x: e.clientX, y: e.clientY, visible: true, isPressed: true };
      laserPoints = [{ x: e.clientX, y: e.clientY, time: performance.now() }];
      if (!laserAnimFrame) {
        laserAnimFrame = requestAnimationFrame(renderLaserLoop);
      }
    } else if (currentTool === "pen" || currentTool === "highlighter") {
      currentStrokePoints = [{ x: e.clientX, y: e.clientY }];
      renderCurrentFreehand(dynamicCtx);
    } else if (currentTool === "stamp") {
      const stampItem: DrawItem = {
        id: crypto.randomUUID(),
        tool: "stamp",
        color: currentColor,
        size: currentSize,
        start: { x: e.clientX, y: e.clientY },
        stampNumber: stampCounter,
        createdAt: Date.now()
      };
      addItemToHistory(stampItem);
      stampCounter += 1;
      isDrawing = false;
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (isGhostMode) return;

    if (currentTool === "laser") {
      laserCursor = { x: e.clientX, y: e.clientY, visible: true, isPressed: isDrawing };
      if (isDrawing) {
        const now = performance.now();
        const last = laserPoints[laserPoints.length - 1];
        if (!last || Math.hypot(e.clientX - last.x, e.clientY - last.y) >= 2.5) {
          laserPoints.push({ x: e.clientX, y: e.clientY, time: now });
        }
      }
      if (!laserAnimFrame) {
        laserAnimFrame = requestAnimationFrame(renderLaserLoop);
      }
      return;
    }

    if (currentTool === "eraser") {
      eraserCursor = { x: e.clientX, y: e.clientY, visible: true };
      if (isDrawing) {
        eraseAt(e.clientX, e.clientY);
      }
      renderEraserPreview();
      return;
    }

    if (!isDrawing) return;

    if (currentTool === "pen" || currentTool === "highlighter") {
      const lastPt = currentStrokePoints[currentStrokePoints.length - 1];
      if (lastPt) {
        const dist = Math.hypot(e.clientX - lastPt.x, e.clientY - lastPt.y);
        if (dist >= 2) {
          const smoothX = lastPt.x * 0.2 + e.clientX * 0.8;
          const smoothY = lastPt.y * 0.2 + e.clientY * 0.8;
          currentStrokePoints.push({ x: smoothX, y: smoothY });
          renderCurrentFreehand(dynamicCtx);
        }
      }
    } else if (["arrow", "rect", "circle", "line"].includes(currentTool)) {
      renderShapePreview(dynamicCtx, startPoint, { x: e.clientX, y: e.clientY }, currentTool);
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (isGhostMode) return;

    if (currentTool === "laser") {
      isDrawing = false;
      laserCursor = { x: e.clientX, y: e.clientY, visible: true, isPressed: false };
      return;
    }

    if (currentTool === "eraser") {
      isDrawing = false;
      renderEraserPreview();
      return;
    }

    if (!isDrawing) return;
    isDrawing = false;

    if (currentTool === "pen" || currentTool === "highlighter") {
      if (currentStrokePoints.length > 0) {
        const last = currentStrokePoints[currentStrokePoints.length - 1];
        if (Math.hypot(e.clientX - last.x, e.clientY - last.y) > 0.5) {
          currentStrokePoints.push({ x: e.clientX, y: e.clientY });
        }
        const item: DrawItem = {
          id: crypto.randomUUID(),
          tool: currentTool,
          color: currentColor,
          size: currentSize,
          points: [...currentStrokePoints],
          createdAt: Date.now()
        };
        addItemToHistory(item);
        currentStrokePoints = [];
        dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
      }
    } else if (["arrow", "rect", "circle", "line"].includes(currentTool)) {
      const endPoint = { x: e.clientX, y: e.clientY };
      const dist = Math.hypot(endPoint.x - startPoint.x, endPoint.y - startPoint.y);
      if (dist > 3) {
        const item: DrawItem = {
          id: crypto.randomUUID(),
          tool: currentTool,
          color: currentColor,
          size: currentSize,
          start: { ...startPoint },
          end: endPoint,
          createdAt: Date.now()
        };
        addItemToHistory(item);
        dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
      }
    }
  }

  function onPointerLeave() {
    if (currentTool === "laser") {
      laserCursor.visible = false;
      isDrawing = false;
    }
    if (currentTool === "eraser") {
      eraserCursor.visible = false;
      isDrawing = false;
      if (dynamicCtx) dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    }
  }

  function addItemToHistory(item: DrawItem) {
    history.push(item);
    redoStack = [];
    redrawStaticCanvas();

    // Auto-fade timer if enabled
    if (autoFadeEnabled) {
      setTimeout(() => {
        history = history.filter((h) => h.id !== item.id);
        redrawStaticCanvas();
      }, AUTO_FADE_MS);
    }
  }

  function commitText() {
    if (isTextActive && textInput.trim().length > 0) {
      const item: DrawItem = {
        id: crypto.randomUUID(),
        tool: "text",
        color: currentColor,
        size: Math.max(16, currentSize * 4.5),
        text: textInput.trim(),
        textPoint: { ...textPos },
        createdAt: Date.now()
      };
      addItemToHistory(item);
    }
    isTextActive = false;
    textInput = "";
  }

  function cancelText() {
    isTextActive = false;
    textInput = "";
  }

  // --- VECTOR RENDERING ---
  function renderCurrentFreehand(ctx: CanvasRenderingContext2D) {
    ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    drawSmoothStroke(ctx, currentStrokePoints, currentColor, currentSize, currentTool === "highlighter");
  }

  function renderShapePreview(
    ctx: CanvasRenderingContext2D,
    start: Point,
    end: Point,
    tool: Tool
  ) {
    ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    ctx.save();
    drawSingleShape(ctx, tool, start, end, currentColor, currentSize);
    ctx.restore();
  }

  function drawSingleShape(
    ctx: CanvasRenderingContext2D,
    tool: Tool,
    start: Point,
    end: Point,
    color: string,
    size: number
  ) {
    ctx.strokeStyle = color;
    ctx.lineWidth = size;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    if (tool === "line") {
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();
    } else if (tool === "rect") {
      ctx.beginPath();
      const x = Math.min(start.x, end.x);
      const y = Math.min(start.y, end.y);
      const w = Math.abs(end.x - start.x);
      const h = Math.abs(end.y - start.y);
      const r = Math.min(8, w / 2, h / 2);
      ctx.roundRect(x, y, w, h, r);
      ctx.stroke();
    } else if (tool === "circle") {
      const radiusX = Math.abs(end.x - start.x) / 2;
      const radiusY = Math.abs(end.y - start.y) / 2;
      const centerX = Math.min(start.x, end.x) + radiusX;
      const centerY = Math.min(start.y, end.y) + radiusY;
      ctx.beginPath();
      ctx.ellipse(centerX, centerY, radiusX, radiusY, 0, 0, Math.PI * 2);
      ctx.stroke();
    } else if (tool === "arrow") {
      // Main arrow shaft
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();

      // Sharp sleek arrowhead
      const angle = Math.atan2(end.y - start.y, end.x - start.x);
      const headLength = Math.max(16, size * 3.5);

      ctx.beginPath();
      ctx.moveTo(end.x, end.y);
      ctx.lineTo(
        end.x - headLength * Math.cos(angle - Math.PI / 6),
        end.y - headLength * Math.sin(angle - Math.PI / 6)
      );
      ctx.moveTo(end.x, end.y);
      ctx.lineTo(
        end.x - headLength * Math.cos(angle + Math.PI / 6),
        end.y - headLength * Math.sin(angle + Math.PI / 6)
      );
      ctx.stroke();
    }
  }

  function redrawStaticCanvas() {
    if (!staticCtx) return;
    staticCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);

    for (const item of history) {
      staticCtx.save();
      if ((item.tool === "pen" || item.tool === "highlighter") && item.points) {
        drawSmoothStroke(staticCtx, item.points, item.color, item.size, item.tool === "highlighter");
      } else if (item.start && item.end) {
        drawSingleShape(staticCtx, item.tool, item.start, item.end, item.color, item.size);
      } else if (item.tool === "stamp" && item.start && item.stampNumber) {
        const r = 16;
        staticCtx.beginPath();
        staticCtx.arc(item.start.x, item.start.y, r, 0, Math.PI * 2);
        staticCtx.fillStyle = item.color;
        staticCtx.shadowColor = "rgba(0,0,0,0.45)";
        staticCtx.shadowBlur = 8;
        staticCtx.fill();

        staticCtx.shadowBlur = 0;
        staticCtx.fillStyle = "#FFFFFF";
        staticCtx.font = "bold 14px -apple-system, BlinkMacSystemFont, sans-serif";
        staticCtx.textAlign = "center";
        staticCtx.textBaseline = "middle";
        staticCtx.fillText(String(item.stampNumber), item.start.x, item.start.y + 1);
      } else if (item.tool === "text" && item.text && item.textPoint) {
        staticCtx.font = `bold ${item.size}px -apple-system, BlinkMacSystemFont, sans-serif`;
        staticCtx.strokeStyle = "rgba(0, 0, 0, 0.85)";
        staticCtx.lineWidth = 4;
        staticCtx.strokeText(item.text, item.textPoint.x, item.textPoint.y);
        staticCtx.fillStyle = item.color;
        staticCtx.fillText(item.text, item.textPoint.x, item.textPoint.y);
      }
      staticCtx.restore();
    }
  }

  // --- STICKY NOTES HELPER FUNCTIONS ---
  function saveStickyNotes() {
    try {
      localStorage.setItem("pixeltrace_sticky_notes", JSON.stringify(stickyNotes));
    } catch (e) {
      console.warn("Failed to persist sticky notes:", e);
    }
  }

  function addStickyNote(x: number, y: number) {
    const defaultColor = stickyColors[stickyNotes.length % stickyColors.length];
    const newNote: StickyNoteItem = {
      id: crypto.randomUUID(),
      x: Math.max(10, Math.min(window.innerWidth - 240, x - 100)),
      y: Math.max(10, Math.min(window.innerHeight - 200, y - 20)),
      width: 210,
      height: 170,
      content: "",
      color: defaultColor.bg,
      headerColor: defaultColor.header,
      textColor: defaultColor.text,
      borderColor: defaultColor.border,
      isCollapsed: false
    };
    stickyNotes = [...stickyNotes, newNote];
    saveStickyNotes();
  }

  function cycleStickyColor(id: string) {
    stickyNotes = stickyNotes.map((note) => {
      if (note.id !== id) return note;
      const currIndex = stickyColors.findIndex((c) => c.bg === note.color);
      const nextColor = stickyColors[(currIndex + 1) % stickyColors.length];
      return {
        ...note,
        color: nextColor.bg,
        headerColor: nextColor.header,
        textColor: nextColor.text,
        borderColor: nextColor.border
      };
    });
    saveStickyNotes();
  }

  function toggleStickyCollapse(id: string) {
    stickyNotes = stickyNotes.map((note) => {
      if (note.id !== id) return note;
      return { ...note, isCollapsed: !note.isCollapsed };
    });
    saveStickyNotes();
  }

  function deleteStickyNote(id: string) {
    stickyNotes = stickyNotes.filter((n) => n.id !== id);
    saveStickyNotes();
  }

  function startStickyDrag(e: MouseEvent, id: string) {
    const note = stickyNotes.find((n) => n.id === id);
    if (!note) return;
    activeStickyDragId = id;
    stickyDragOffset = {
      x: e.clientX - note.x,
      y: e.clientY - note.y
    };
    window.addEventListener("mousemove", onStickyDragMove);
    window.addEventListener("mouseup", onStickyDragEnd);
  }

  function onStickyDragMove(e: MouseEvent) {
    if (!activeStickyDragId) return;
    const currentId = activeStickyDragId;
    stickyNotes = stickyNotes.map((n) => {
      if (n.id !== currentId) return n;
      return {
        ...n,
        x: Math.max(0, Math.min(window.innerWidth - n.width, e.clientX - stickyDragOffset.x)),
        y: Math.max(0, Math.min(window.innerHeight - 40, e.clientY - stickyDragOffset.y))
      };
    });
  }

  function onStickyDragEnd() {
    activeStickyDragId = null;
    window.removeEventListener("mousemove", onStickyDragMove);
    window.removeEventListener("mouseup", onStickyDragEnd);
    saveStickyNotes();
  }

  // --- ERASER ENGINE ---
  function distToSegment(px: number, py: number, x1: number, y1: number, x2: number, y2: number) {
    const l2 = (x2 - x1) ** 2 + (y2 - y1) ** 2;
    if (l2 === 0) return Math.hypot(px - x1, py - y1);
    let t = ((px - x1) * (x2 - x1) + (py - y1) * (y2 - y1)) / l2;
    t = Math.max(0, Math.min(1, t));
    return Math.hypot(px - (x1 + t * (x2 - x1)), py - (y1 + t * (y2 - y1)));
  }

  function eraseAt(x: number, y: number) {
    const r = Math.max(14, currentSize * 3 + 12);
    const toDeleteIds = new Set<string>();

    for (const item of history) {
      if ((item.tool === "pen" || item.tool === "highlighter") && item.points) {
        for (let i = 0; i < item.points.length - 1; i++) {
          const d = distToSegment(x, y, item.points[i].x, item.points[i].y, item.points[i + 1].x, item.points[i + 1].y);
          if (d <= r + item.size / 2) {
            toDeleteIds.add(item.id);
            break;
          }
        }
      } else if ((item.tool === "line" || item.tool === "arrow") && item.start && item.end) {
        const d = distToSegment(x, y, item.start.x, item.start.y, item.end.x, item.end.y);
        if (d <= r + item.size / 2) {
          toDeleteIds.add(item.id);
        }
      } else if (item.tool === "rect" && item.start && item.end) {
        const minX = Math.min(item.start.x, item.end.x);
        const maxX = Math.max(item.start.x, item.end.x);
        const minY = Math.min(item.start.y, item.end.y);
        const maxY = Math.max(item.start.y, item.end.y);
        const nearLeft = distToSegment(x, y, minX, minY, minX, maxY);
        const nearRight = distToSegment(x, y, maxX, minY, maxX, maxY);
        const nearTop = distToSegment(x, y, minX, minY, maxX, minY);
        const nearBottom = distToSegment(x, y, minX, maxY, maxX, maxY);
        if (Math.min(nearLeft, nearRight, nearTop, nearBottom) <= r + item.size / 2) {
          toDeleteIds.add(item.id);
        }
      } else if (item.tool === "circle" && item.start && item.end) {
        const rx = Math.abs(item.end.x - item.start.x) / 2;
        const ry = Math.abs(item.end.y - item.start.y) / 2;
        const cx = Math.min(item.start.x, item.end.x) + rx;
        const cy = Math.min(item.start.y, item.end.y) + ry;
        const distCenter = Math.hypot(x - cx, y - cy);
        const avgR = (rx + ry) / 2;
        if (Math.abs(distCenter - avgR) <= r + item.size / 2) {
          toDeleteIds.add(item.id);
        }
      } else if (item.tool === "stamp" && item.start) {
        if (Math.hypot(x - item.start.x, y - item.start.y) <= r + 18) {
          toDeleteIds.add(item.id);
        }
      } else if (item.tool === "text" && item.textPoint) {
        if (Math.hypot(x - item.textPoint.x, y - item.textPoint.y) <= r + 24) {
          toDeleteIds.add(item.id);
        }
      }
    }

    if (toDeleteIds.size > 0) {
      const deletedItems = history.filter((h) => toDeleteIds.has(h.id));
      redoStack.push(...deletedItems);
      history = history.filter((h) => !toDeleteIds.has(h.id));
      redrawStaticCanvas();
    }
  }

  function renderEraserPreview() {
    if (!dynamicCtx || currentTool !== "eraser" || !eraserCursor.visible) return;
    dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    const r = Math.max(14, currentSize * 3 + 12);

    dynamicCtx.save();
    dynamicCtx.beginPath();
    dynamicCtx.arc(eraserCursor.x, eraserCursor.y, r, 0, Math.PI * 2);
    dynamicCtx.fillStyle = "rgba(255, 255, 255, 0.22)";
    dynamicCtx.fill();

    dynamicCtx.beginPath();
    dynamicCtx.arc(eraserCursor.x, eraserCursor.y, r, 0, Math.PI * 2);
    dynamicCtx.strokeStyle = "rgba(255, 255, 255, 0.9)";
    dynamicCtx.lineWidth = 1.8;
    dynamicCtx.setLineDash([4, 4]);
    dynamicCtx.stroke();
    dynamicCtx.restore();
  }

  // --- ACTIONS ---
  function undo() {
    if (history.length === 0) return;
    const item = history.pop()!;
    redoStack.push(item);
    redrawStaticCanvas();
  }

  function redo() {
    if (redoStack.length === 0) return;
    const item = redoStack.pop()!;
    history.push(item);
    redrawStaticCanvas();
  }

  function clearCanvas() {
    history = [];
    redoStack = [];
    stampCounter = 1;
    isTextActive = false;
    if (staticCtx) staticCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    if (dynamicCtx) dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
  }

  function toggleColorPicker(e?: MouseEvent) {
    e?.stopPropagation();
    showColorPicker = !showColorPicker;
    if (showColorPicker) showSizePicker = false;
    hideTooltip();
    tick().then(updateInteractiveRects);
  }

  function toggleSizePicker(e?: MouseEvent) {
    e?.stopPropagation();
    showSizePicker = !showSizePicker;
    if (showSizePicker) showColorPicker = false;
    hideTooltip();
    tick().then(updateInteractiveRects);
  }

  function closePickers() {
    if (showColorPicker || showSizePicker) {
      showColorPicker = false;
      showSizePicker = false;
      tick().then(updateInteractiveRects);
    }
  }

  function handleWindowClick(e: MouseEvent) {
    if (showColorPicker || showSizePicker) {
      const target = e.target as HTMLElement;
      if (
        !target.closest(".color-popover") &&
        !target.closest(".color-trigger-btn") &&
        !target.closest(".size-popover") &&
        !target.closest(".size-trigger-btn")
      ) {
        closePickers();
      }
    }
  }

  function showTooltip(e: MouseEvent, label: string, shortcut?: string) {
    if (isCollapsed) return;
    const el = e.currentTarget as HTMLElement;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const y = rect.bottom + 8 < window.innerHeight - 40 ? rect.bottom + 8 : rect.top - 32;
    hoveredTooltip = {
      label,
      shortcut,
      x: rect.left + rect.width / 2,
      y
    };
  }

  function hideTooltip() {
    hoveredTooltip = null;
  }

  function updateInteractiveRects() {
    const rects: InteractiveRect[] = [];

    // Toolbar or Collapsed Tab rect
    if (isCollapsed) {
      const tabW = 60;
      const tabH = 68;
      rects.push({ x: 0, y: widgetY, width: tabW, height: tabH });
    } else if (toolbarRef) {
      rects.push({
        x: widgetX,
        y: widgetY,
        width: toolbarRef.offsetWidth || 780,
        height: toolbarRef.offsetHeight || 54
      });

      // Popover rects if open
      if (showColorPicker && colorPickerRef) {
        const cr = colorPickerRef.getBoundingClientRect();
        rects.push({ x: cr.left, y: cr.top, width: cr.width, height: cr.height });
      }
      if (showSizePicker && sizePickerRef) {
        const sr = sizePickerRef.getBoundingClientRect();
        rects.push({ x: sr.left, y: sr.top, width: sr.width, height: sr.height });
      }
    }

    // Sticky notes rects
    for (const note of stickyNotes) {
      rects.push({
        x: note.x,
        y: note.y,
        width: note.width,
        height: note.isCollapsed ? 36 : note.height
      });
    }

    // Preferences & Shortcuts modal
    if (showHelpModal) {
      const mw = 490;
      const mh = 480;
      rects.push({
        x: Math.max(0, (window.innerWidth - mw) / 2),
        y: Math.max(0, (window.innerHeight - mh) / 2),
        width: mw,
        height: mh
      });
    }

    // Inline text editing
    if (isTextActive) {
      rects.push({
        x: textPos.x,
        y: textPos.y - 28,
        width: 340,
        height: 52
      });
    }

    invoke("set_interactive_rects", { rects }).catch(() => {});
  }

  function openPreferencesModal(tab: "shortcuts" | "preferences" | "about" = "shortcuts") {
    modalActiveTab = tab;
    showHelpModal = true;
    tick().then(updateInteractiveRects);
  }

  function collapseToNearestEdge() {
    closePickers();
    // Strictly lock flush against the left wall
    dockSide = "left";
    widgetX = 0;
    widgetY = Math.max(24, Math.min(window.innerHeight - 90, widgetY));
    isCollapsed = true;
    tick().then(updateInteractiveRects);
  }

  async function expandFromEdge() {
    isCollapsed = false;
    await tick();
    const w = window.innerWidth;
    const bar = toolbarRef?.querySelector(".glass-bar") as HTMLElement | null;
    const barWidth = bar ? bar.offsetWidth : (toolbarRef ? toolbarRef.offsetWidth : 940);

    // Pull out straight from left wall directly into exact top-center of the screen
    widgetX = Math.max(16, Math.min(w - barWidth - 16, Math.round((w - barWidth) / 2)));
    widgetY = 24;
    updateInteractiveRects();
  }

  function onMouseEnterInteractive() {
    if (isGhostMode) {
      invoke("set_window_interactive", { interactive: true }).catch(() => {});
    }
  }

  function onMouseLeaveInteractive() {
    if (isGhostMode) {
      invoke("set_window_interactive", { interactive: false }).catch(() => {});
    }
  }

  function onCollapsedTabDragStart(e: MouseEvent) {
    isDragging = true;
    dragOffset = {
      x: e.clientX - widgetX,
      y: e.clientY - widgetY
    };
    window.addEventListener("mousemove", onCollapsedTabDragMove);
    window.addEventListener("mouseup", onCollapsedTabDragEnd);
  }

  function onCollapsedTabDragMove(e: MouseEvent) {
    if (!isDragging) return;
    const h = window.innerHeight;

    // Strictly keep collapsed tab flush against the left wall
    dockSide = "left";
    widgetX = 0;
    widgetY = Math.max(16, Math.min(h - 84, e.clientY - dragOffset.y));
    updateInteractiveRects();
  }

  function onCollapsedTabDragEnd() {
    isDragging = false;
    window.removeEventListener("mousemove", onCollapsedTabDragMove);
    window.removeEventListener("mouseup", onCollapsedTabDragEnd);

    dockSide = "left";
    widgetX = 0;
    updateInteractiveRects();
  }

  async function toggleGhostMode() {
    try {
      const newState = await invoke<boolean>("toggle_ghost_mode");
      isGhostMode = newState;
      triggerGhostToast(isGhostMode);
      tick().then(updateInteractiveRects);
    } catch (err) {
      isGhostMode = !isGhostMode;
      await invoke("set_click_through", { ignore: isGhostMode });
      triggerGhostToast(isGhostMode);
      tick().then(updateInteractiveRects);
    }
  }

  // --- CAPTURE SCREENSHOT ---
  function captureSnapshot() {
    if (!staticCanvas) return;
    try {
      staticCanvas.toBlob((blob) => {
        if (blob) {
          navigator.clipboard.write([new ClipboardItem({ "image/png": blob })])
            .then(() => alert("Annotated screen markups copied to clipboard!"))
            .catch(() => {
              const a = document.createElement("a");
              a.href = staticCanvas.toDataURL("image/png");
              a.download = `pixeltrace-${Date.now()}.png`;
              a.click();
            });
        }
      });
    } catch (e) {
      console.error("Snapshot error:", e);
    }
  }

  // --- MULTI-MONITOR SWITCHING ---
  async function switchToMonitor(index: number) {
    if (!monitors[index]) return;
    const m = monitors[index];
    try {
      await invoke("focus_monitor", {
        x: m.x,
        y: m.y,
        width: m.width,
        height: m.height
      });
      activeMonitorIndex = index;
      setTimeout(() => {
        initCanvases();
        centerToolbar();
      }, 80);
    } catch (e) {
      console.error("Failed to switch monitor:", e);
    }
  }

  // --- KEYBOARD SHORTCUTS ---
  function handleKeyDown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
      if (e.key === "Enter") {
        e.preventDefault();
        commitText();
      } else if (e.key === "Escape") {
        e.preventDefault();
        cancelText();
      }
      return;
    }

    const key = e.key.toLowerCase();

    if ((e.metaKey || e.ctrlKey) && key === "z") {
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
      return;
    }

    if ((e.metaKey || e.ctrlKey) && key === "y") {
      e.preventDefault();
      redo();
      return;
    }

    if ((e.metaKey || e.ctrlKey) && key === ",") {
      e.preventDefault();
      openPreferencesModal("preferences");
      return;
    }

    if (e.key === "?" || (e.metaKey && e.shiftKey && key === "/")) {
      e.preventDefault();
      openPreferencesModal("shortcuts");
      return;
    }

    if (e.key === "Escape") {
      if (showColorPicker || showSizePicker) {
        closePickers();
        return;
      }
      if (showHelpModal) {
        showHelpModal = false;
        tick().then(updateInteractiveRects);
        return;
      }
      clearCanvas();
      return;
    }

    if (key === "x" || key === "g") {
      toggleGhostMode();
      return;
    }

    if (key === "l" || key === "1") currentTool = "laser";
    if (key === "p" || key === "2") currentTool = "pen";
    if (key === "h" || key === "3") currentTool = "highlighter";
    if (key === "a" || key === "4") currentTool = "arrow";
    if (key === "r" || key === "5") currentTool = "rect";
    if (key === "c" || key === "6") currentTool = "circle";
    if (key === "i" || key === "7") currentTool = "line";
    if (key === "s" || key === "8") currentTool = "stamp";
    if (key === "t" || key === "9") currentTool = "text";
    if (key === "e" || key === "0") currentTool = "eraser";
    if (key === "n") currentTool = "sticky";

    if (e.key === " ") {
      e.preventDefault();
      if (isCollapsed) {
        expandFromEdge();
      } else {
        collapseToNearestEdge();
      }
    }
  }

  // --- FLOATING TOOLBAR DRAGGING ---
  function onDragStart(e: MouseEvent) {
    isDragging = true;
    dragOffset = {
      x: e.clientX - widgetX,
      y: e.clientY - widgetY
    };
    window.addEventListener("mousemove", onDragMove);
    window.addEventListener("mouseup", onDragEnd);
  }

  function onDragMove(e: MouseEvent) {
    if (!isDragging) return;
    widgetX = e.clientX - dragOffset.x;
    widgetY = e.clientY - dragOffset.y;
    updateInteractiveRects();
  }

  function onDragEnd() {
    isDragging = false;
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragEnd);

    const w = window.innerWidth;
    const h = window.innerHeight;
    const bar = toolbarRef?.querySelector(".glass-bar") as HTMLElement | null;
    const barWidth = bar ? bar.offsetWidth : (toolbarRef ? toolbarRef.offsetWidth : 940);
    const barHeight = bar ? bar.offsetHeight : (toolbarRef ? toolbarRef.offsetHeight : 54);

    widgetX = Math.max(16, Math.min(w - barWidth - 16, widgetX));
    widgetY = Math.max(16, Math.min(h - barHeight - 16, widgetY));

    isDocked = widgetY <= 16;
    updateInteractiveRects();
  }
</script>

<svelte:body class:ghost-active={isGhostMode} />

<!-- Global Transparent Drawing Surfaces -->
<canvas
  bind:this={staticCanvas}
  class="canvas-layer"
  class:ghost-active={isGhostMode}
></canvas>

<canvas
  bind:this={dynamicCanvas}
  class="canvas-layer tool-{currentTool}"
  class:ghost-active={isGhostMode}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointerleave={onPointerLeave}
></canvas>

<!-- Inline Editable Text Box -->
{#if isTextActive}
  <div
    class="inline-text-box"
    style="left: {textPos.x}px; top: {textPos.y - 24}px;"
    role="region"
    aria-label="Text Annotation Input"
    onmouseenter={onMouseEnterInteractive}
    onmouseleave={onMouseLeaveInteractive}
  >
    <input
      bind:this={textInputRef}
      bind:value={textInput}
      style="color: {currentColor}; font-size: {Math.max(16, currentSize * 4.5)}px;"
      placeholder="Type annotation and press Enter..."
    />
    <div class="text-hint">Enter to place • Esc to cancel</div>
  </div>
{/if}

<!-- Ghost Mode Toast Notification -->
{#if ghostNotification}
  <div class="ghost-toast" class:active={isGhostMode}>
    <Ghost size={16} />
    <span>{ghostNotification}</span>
  </div>
{/if}

<!-- Interactive Draggable Physical Sticky Notes Layer -->
{#each stickyNotes as note (note.id)}
  <div
    class="sticky-note"
    class:collapsed={note.isCollapsed}
    style="left: {note.x}px; top: {note.y}px; width: {note.width}px; background-color: {note.color}; border-color: {note.borderColor}; color: {note.textColor};"
    role="region"
    aria-label="Sticky Note"
    onmouseenter={onMouseEnterInteractive}
    onmouseleave={onMouseLeaveInteractive}
  >
    <!-- Translucent Frosted Tape Pin at Top -->
    <div class="sticky-tape"></div>

    <!-- Note Header (Draggable) -->
    <div
      class="sticky-header"
      style="background-color: {note.headerColor}; border-bottom-color: {note.borderColor};"
      onmousedown={(e) => startStickyDrag(e, note.id)}
      role="button"
      tabindex="0"
    >
      <span class="sticky-title">Sticky Note</span>

      <div class="sticky-actions">
        <!-- Color Cycle Button -->
        <button
          class="sticky-action-btn"
          onclick={() => cycleStickyColor(note.id)}
          title="Change Note Color"
          style="color: {note.textColor};"
        >
          🎨
        </button>

        <!-- Minimize / Fold Button -->
        <button
          class="sticky-action-btn"
          onclick={() => toggleStickyCollapse(note.id)}
          title={note.isCollapsed ? "Expand Note" : "Fold Note"}
          style="color: {note.textColor};"
        >
          {note.isCollapsed ? "□" : "─"}
        </button>

        <!-- Delete Button -->
        <button
          class="sticky-action-btn close"
          onclick={() => deleteStickyNote(note.id)}
          title="Delete Note"
          style="color: {note.textColor};"
        >
          ✕
        </button>
      </div>
    </div>

    <!-- Note Content -->
    {#if !note.isCollapsed}
      <textarea
        class="sticky-body"
        style="color: {note.textColor};"
        placeholder="Write note here..."
        bind:value={note.content}
        oninput={saveStickyNotes}
      ></textarea>
    {/if}
  </div>
{/each}

<!-- Floating Centered Toolbar & Edge Dock -->
<div
  bind:this={toolbarRef}
  class="widget-container"
  class:docked={isDocked}
  class:collapsed={isCollapsed}
  style="transform: translate({widgetX}px, {widgetY}px);"
  onmouseenter={onMouseEnterInteractive}
  onmouseleave={onMouseLeaveInteractive}
  role="region"
  aria-label="PixelTrace Screen Toolbar"
>
  {#if isCollapsed}
    <!-- Docked Left Edge Tab (Half-Pill sticking flush to left wall) -->
    <button
      bind:this={collapsedTabRef}
      class="edge-dock-tab dock-left"
      onclick={expandFromEdge}
      onmousedown={onCollapsedTabDragStart}
      onmouseenter={(e) => showTooltip(e, "Expand PixelTrace", "Space")}
      onmouseleave={hideTooltip}
      aria-label="Expand PixelTrace Toolbar"
    >
      <div class="dock-logo-badge">
        <img src="/app-logo.png" alt="PixelTrace" class="dock-app-logo" />
      </div>
      <div class="dock-chevron">
        <ChevronRight size={16} />
      </div>
    </button>
  {:else}
    <!-- Full Modern Glassmorphic Toolbar -->
    <div class="glass-bar">
      <!-- App Brand Logo -->
      <div
        class="bar-brand"
        onmouseenter={(e) => showTooltip(e, "PixelTrace Screen Annotator")}
        onmouseleave={hideTooltip}
        role="presentation"
      >
        <img src="/app-logo.png" alt="PixelTrace Logo" class="bar-brand-logo" />
      </div>

      <!-- Drag Handle -->
      <div
        class="drag-handle"
        onmousedown={onDragStart}
        onmouseenter={(e) => showTooltip(e, "Drag Toolbar")}
        onmouseleave={hideTooltip}
        role="button"
        tabindex="0"
        aria-label="Drag toolbar"
      >
        <GripVertical size={16} />
      </div>

      <div class="divider"></div>

      <!-- Tools Group -->
      <div class="btn-group">
        <!-- Laser Pointer -->
        <button
          class="tool-btn"
          class:active={currentTool === "laser"}
          onclick={() => (currentTool = "laser")}
          onmouseenter={(e) => showTooltip(e, "Laser Pointer", "L / 1")}
          onmouseleave={hideTooltip}
          aria-label="Laser Pointer"
        >
          <Sparkles size={17} />
          <span class="laser-indicator" style="background-color: {currentColor}"></span>
        </button>

        <!-- Pen -->
        <button
          class="tool-btn"
          class:active={currentTool === "pen"}
          onclick={() => (currentTool = "pen")}
          onmouseenter={(e) => showTooltip(e, "Smooth Pen", "P / 2")}
          onmouseleave={hideTooltip}
          aria-label="Smooth Pen"
        >
          <Pencil size={17} />
        </button>

        <!-- Highlighter -->
        <button
          class="tool-btn"
          class:active={currentTool === "highlighter"}
          onclick={() => (currentTool = "highlighter")}
          onmouseenter={(e) => showTooltip(e, "Highlighter", "H / 3")}
          onmouseleave={hideTooltip}
          aria-label="Highlighter"
        >
          <Highlighter size={17} />
        </button>

        <!-- Arrow -->
        <button
          class="tool-btn"
          class:active={currentTool === "arrow"}
          onclick={() => (currentTool = "arrow")}
          onmouseenter={(e) => showTooltip(e, "Arrow Tool", "A / 4")}
          onmouseleave={hideTooltip}
          aria-label="Arrow Tool"
        >
          <ArrowUpRight size={18} />
        </button>

        <!-- Rectangle -->
        <button
          class="tool-btn"
          class:active={currentTool === "rect"}
          onclick={() => (currentTool = "rect")}
          onmouseenter={(e) => showTooltip(e, "Rectangle", "R / 5")}
          onmouseleave={hideTooltip}
          aria-label="Rectangle"
        >
          <Square size={16} />
        </button>

        <!-- Circle -->
        <button
          class="tool-btn"
          class:active={currentTool === "circle"}
          onclick={() => (currentTool = "circle")}
          onmouseenter={(e) => showTooltip(e, "Circle", "C / 6")}
          onmouseleave={hideTooltip}
          aria-label="Circle"
        >
          <Circle size={16} />
        </button>

        <!-- Line -->
        <button
          class="tool-btn"
          class:active={currentTool === "line"}
          onclick={() => (currentTool = "line")}
          onmouseenter={(e) => showTooltip(e, "Straight Line", "I / 7")}
          onmouseleave={hideTooltip}
          aria-label="Straight Line"
        >
          <Slash size={16} />
        </button>

        <!-- Text -->
        <button
          class="tool-btn"
          class:active={currentTool === "text"}
          onclick={() => (currentTool = "text")}
          onmouseenter={(e) => showTooltip(e, "Text Note", "T / 9")}
          onmouseleave={hideTooltip}
          aria-label="Text Note"
        >
          <Type size={17} />
        </button>

        <!-- Numbered Stamp -->
        <button
          class="tool-btn stamp-btn"
          class:active={currentTool === "stamp"}
          onclick={() => (currentTool = "stamp")}
          oncontextmenu={(e) => {
            e.preventDefault();
            stampCounter = 1;
          }}
          onmouseenter={(e) => showTooltip(e, `Stamp #${stampCounter} (Right-click reset)`, "S / 8")}
          onmouseleave={hideTooltip}
          aria-label="Numbered Stamp"
        >
          <ListOrdered size={16} />
          <span class="stamp-badge">{stampCounter}</span>
        </button>

        <!-- Eraser -->
        <button
          class="tool-btn"
          class:active={currentTool === "eraser"}
          onclick={() => (currentTool = "eraser")}
          onmouseenter={(e) => showTooltip(e, "Eraser", "E / 0")}
          onmouseleave={hideTooltip}
          aria-label="Eraser"
        >
          <Eraser size={16} />
        </button>

        <!-- Sticky Note -->
        <button
          class="tool-btn"
          class:active={currentTool === "sticky"}
          onclick={() => (currentTool = "sticky")}
          onmouseenter={(e) => showTooltip(e, "Sticky Note", "N")}
          onmouseleave={hideTooltip}
          aria-label="Sticky Note"
        >
          <StickyNote size={16} />
        </button>
      </div>

      <div class="divider"></div>

      <!-- Stroke Size Compact Trigger & Popover -->
      <div class="popover-anchor">
        <button
          class="size-trigger-btn"
          class:active={showSizePicker}
          onclick={toggleSizePicker}
          onmouseenter={(e) => showTooltip(e, "Stroke Width", `${currentSize}px`)}
          onmouseleave={hideTooltip}
          aria-label="Stroke Width Picker"
        >
          <span class="size-dot-preview" style="width: {Math.max(4, Math.min(14, currentSize + 2))}px; height: {Math.max(4, Math.min(14, currentSize + 2))}px;"></span>
          <span class="size-val-label">{currentSize}px</span>
        </button>

        {#if showSizePicker}
          <div
            bind:this={sizePickerRef}
            class="popover-menu size-popover"
            onmouseenter={onMouseEnterInteractive}
            onmouseleave={onMouseLeaveInteractive}
            role="region"
            aria-label="Stroke Width Settings"
          >
            <div class="popover-header">
              <span class="popover-title">Stroke Width</span>
              <span class="popover-badge">{currentSize} px</span>
            </div>

            <div class="size-presets-grid">
              {#each sizePresets as s}
                <button
                  class="size-preset-item"
                  class:active={currentSize === s.val}
                  onclick={() => {
                    currentSize = s.val;
                    showSizePicker = false;
                    tick().then(updateInteractiveRects);
                  }}
                  title="{s.label} ({s.val}px)"
                >
                  <span class="size-dot" style="width: {s.val + 2}px; height: {s.val + 2}px;"></span>
                  <span class="preset-label">{s.label}</span>
                </button>
              {/each}
            </div>

            <div class="slider-container">
              <input
                type="range"
                min="1"
                max="32"
                step="1"
                bind:value={currentSize}
                class="stroke-range-slider"
                aria-label="Adjust stroke width"
              />
            </div>
          </div>
        {/if}
      </div>

      <div class="divider"></div>

      <!-- Color Palette Compact Trigger & Popover -->
      <div class="popover-anchor">
        <button
          class="color-trigger-btn"
          class:active={showColorPicker}
          onclick={toggleColorPicker}
          onmouseenter={(e) => showTooltip(e, "Color Palette", currentColor)}
          onmouseleave={hideTooltip}
          aria-label="Color Palette"
        >
          <span class="active-color-swatch" style="background-color: {currentColor};"></span>
        </button>

        {#if showColorPicker}
          <div
            bind:this={colorPickerRef}
            class="popover-menu color-popover"
            onmouseenter={onMouseEnterInteractive}
            onmouseleave={onMouseLeaveInteractive}
            role="region"
            aria-label="Color Palette Settings"
          >
            <div class="popover-header">
              <span class="popover-title">Palette</span>
              <label class="custom-color-btn" title="Choose Custom Color">
                <input
                  type="color"
                  bind:value={currentColor}
                  class="hidden-color-input"
                />
                <span class="custom-swatch" style="background-color: {currentColor};"></span>
                <span class="custom-text">Custom</span>
              </label>
            </div>

            <div class="color-swatches-grid">
              {#each colors as c}
                <button
                  class="color-chip-pop"
                  class:active={currentColor === c.hex}
                  style="background-color: {c.hex}"
                  onclick={() => {
                    currentColor = c.hex;
                    showColorPicker = false;
                    tick().then(updateInteractiveRects);
                  }}
                  title={c.name}
                  aria-label={c.name}
                ></button>
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <div class="divider"></div>

      <!-- Actions Group -->
      <div class="btn-group">
        <button
          class="icon-btn"
          disabled={history.length === 0}
          onclick={undo}
          onmouseenter={(e) => showTooltip(e, "Undo", "⌘Z")}
          onmouseleave={hideTooltip}
          aria-label="Undo"
        >
          <Undo2 size={16} />
        </button>

        <button
          class="icon-btn"
          disabled={redoStack.length === 0}
          onclick={redo}
          onmouseenter={(e) => showTooltip(e, "Redo", "⌘⇧Z")}
          onmouseleave={hideTooltip}
          aria-label="Redo"
        >
          <Redo2 size={16} />
        </button>

        <button
          class="icon-btn"
          onclick={captureSnapshot}
          onmouseenter={(e) => showTooltip(e, "Copy Screenshot", "⌘⇧S")}
          onmouseleave={hideTooltip}
          aria-label="Copy Screenshot to Clipboard"
        >
          <Camera size={16} />
        </button>

        <button
          class="icon-btn"
          class:active={autoFadeEnabled}
          onclick={() => (autoFadeEnabled = !autoFadeEnabled)}
          onmouseenter={(e) => showTooltip(e, `Auto-Fade (${autoFadeEnabled ? 'ON' : 'OFF'})`, "F")}
          onmouseleave={hideTooltip}
          aria-label="Toggle Auto-Fade"
        >
          <Hourglass size={16} />
        </button>

        <button
          class="icon-btn danger"
          onclick={clearCanvas}
          onmouseenter={(e) => showTooltip(e, "Clear Markups", "Esc")}
          onmouseleave={hideTooltip}
          aria-label="Clear Screen Markups"
        >
          <Trash2 size={16} />
        </button>
      </div>

      <div class="divider"></div>

      <!-- Multi-Monitor Switcher (if > 1 monitor detected) -->
      {#if monitors.length > 1}
        <div class="monitor-group">
          {#each monitors as m, idx}
            <button
              class="mon-btn"
              class:active={activeMonitorIndex === idx}
              onclick={() => switchToMonitor(idx)}
              onmouseenter={(e) => showTooltip(e, `Switch to Display ${idx + 1}`)}
              onmouseleave={hideTooltip}
              aria-label="Switch to Monitor {idx + 1}"
            >
              <Monitor size={12} />
              <span>{idx + 1}</span>
            </button>
          {/each}
        </div>
        <div class="divider"></div>
      {/if}

      <!-- Ghost Mode (Click-Through) Button -->
      <button
        class="ghost-btn"
        class:active={isGhostMode}
        onclick={toggleGhostMode}
        onmouseenter={(e) => showTooltip(e, isGhostMode ? "Ghost Active (Drawing Locked)" : "Ghost Mode (Click-Through)", "⌘⇧X")}
        onmouseleave={hideTooltip}
        aria-label="Toggle Ghost Mode"
      >
        <Ghost size={16} />
        <div class="ghost-btn-text">
          <span class="ghost-title">{isGhostMode ? "Ghost Active" : "Ghost"}</span>
        </div>
      </button>

      <!-- Preferences / Settings Button -->
      <button
        class="icon-btn"
        onclick={() => openPreferencesModal("preferences")}
        onmouseenter={(e) => showTooltip(e, "Preferences & Settings", "⌘,")}
        onmouseleave={hideTooltip}
        aria-label="Preferences & Settings"
      >
        <Settings size={16} />
      </button>

      <!-- Keyboard Shortcuts / Help Button -->
      <button
        class="icon-btn"
        onclick={() => openPreferencesModal("shortcuts")}
        onmouseenter={(e) => showTooltip(e, "Shortcuts & About", "?")}
        onmouseleave={hideTooltip}
        aria-label="Keyboard Shortcuts & About"
      >
        <Keyboard size={16} />
      </button>

      <!-- Collapse Button -->
      <button
        class="collapse-btn"
        onclick={collapseToNearestEdge}
        onmouseenter={(e) => showTooltip(e, "Collapse to Left Edge", "Space")}
        onmouseleave={hideTooltip}
        aria-label="Collapse to screen edge"
      >
        <ChevronLeft size={16} />
      </button>
    </div>
  {/if}
</div>

<!-- Sleek Floating Tooltip Overlay -->
{#if hoveredTooltip && !isCollapsed}
  <div
    class="custom-tooltip"
    style="left: {hoveredTooltip.x}px; top: {hoveredTooltip.y}px;"
  >
    <span class="tooltip-title">{hoveredTooltip.label}</span>
    {#if hoveredTooltip.shortcut}
      <span class="tooltip-shortcut">{hoveredTooltip.shortcut}</span>
    {/if}
  </div>
{/if}

<!-- Shortcuts & Preferences Modal -->
{#if showHelpModal}
  <div
    class="modal-backdrop"
    role="dialog"
    aria-modal="true"
    tabindex="0"
    onmouseenter={onMouseEnterInteractive}
    onmouseleave={onMouseLeaveInteractive}
    onclick={(e) => {
      if (e.target === e.currentTarget) {
        showHelpModal = false;
        tick().then(updateInteractiveRects);
      }
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") {
        showHelpModal = false;
        tick().then(updateInteractiveRects);
      }
    }}
  >
    <div
      class="modal-card"
      role="document"
    >
      <div class="modal-header">
        <div class="modal-tabs">
          <button
            class="tab-btn"
            class:active={modalActiveTab === "shortcuts"}
            onclick={() => { modalActiveTab = "shortcuts"; tick().then(updateInteractiveRects); }}
          >
            <Keyboard size={13} />
            <span>Shortcuts</span>
          </button>
          <button
            class="tab-btn"
            class:active={modalActiveTab === "preferences"}
            onclick={() => { modalActiveTab = "preferences"; tick().then(updateInteractiveRects); }}
          >
            <Sliders size={13} />
            <span>Preferences</span>
          </button>
          <button
            class="tab-btn"
            class:active={modalActiveTab === "about"}
            onclick={() => { modalActiveTab = "about"; tick().then(updateInteractiveRects); }}
          >
            <span>About</span>
          </button>
        </div>
        <button
          class="modal-close"
          onclick={() => {
            showHelpModal = false;
            tick().then(updateInteractiveRects);
          }}
          aria-label="Close dialog"
        >
          <X size={15} />
        </button>
      </div>

      <div class="modal-body">
        {#if modalActiveTab === "shortcuts"}
          <div class="shortcuts-scrollable">
            <div class="shortcut-group">
              <span class="group-title">Navigation & Modes</span>
              <div class="shortcut-row">
                <span>Toggle Overlay (Hide / Show)</span>
                <kbd>⌘ + Shift + D</kbd>
              </div>
              <div class="shortcut-row">
                <span>Toggle Ghost Mode (Click-Through)</span>
                <kbd>⌘ + Shift + X / ⌘⇧G</kbd>
              </div>
              <div class="shortcut-row">
                <span>Ghost Quick Toggle (when focused)</span>
                <kbd>X / G</kbd>
              </div>
              <div class="shortcut-row">
                <span>Collapse / Expand Toolbar</span>
                <kbd>Space</kbd>
              </div>
              <div class="shortcut-row">
                <span>Open Preferences & Settings</span>
                <kbd>⌘ + ,</kbd>
              </div>
            </div>

            <div class="shortcut-group">
              <span class="group-title">Drawing & Annotation Tools</span>
              <div class="shortcut-row">
                <span>Laser Pointer (Continuous Glow)</span>
                <kbd>1 / L</kbd>
              </div>
              <div class="shortcut-row">
                <span>Pen / Marker</span>
                <kbd>2 / P</kbd>
              </div>
              <div class="shortcut-row">
                <span>Translucent Highlighter</span>
                <kbd>3 / H</kbd>
              </div>
              <div class="shortcut-row">
                <span>Arrow Tool</span>
                <kbd>4 / A</kbd>
              </div>
              <div class="shortcut-row">
                <span>Rectangle & Circle Shapes</span>
                <kbd>5 / 6 / R / C</kbd>
              </div>
              <div class="shortcut-row">
                <span>Straight Line</span>
                <kbd>7 / I</kbd>
              </div>
              <div class="shortcut-row">
                <span>Numbered Step Stamp</span>
                <kbd>8 / S</kbd>
              </div>
              <div class="shortcut-row">
                <span>Screen Text Note</span>
                <kbd>9 / T</kbd>
              </div>
              <div class="shortcut-row">
                <span>Vector Eraser</span>
                <kbd>0 / E</kbd>
              </div>
              <div class="shortcut-row">
                <span>Desktop Sticky Note</span>
                <kbd>N</kbd>
              </div>
            </div>

            <div class="shortcut-group">
              <span class="group-title">Canvas Management</span>
              <div class="shortcut-row">
                <span>Undo Last Action</span>
                <kbd>⌘ + Z</kbd>
              </div>
              <div class="shortcut-row">
                <span>Redo Action</span>
                <kbd>⌘ + Shift + Z / ⌘ + Y</kbd>
              </div>
              <div class="shortcut-row">
                <span>Clear Screen Markups</span>
                <kbd>Esc</kbd>
              </div>
            </div>
          </div>
        {:else if modalActiveTab === "preferences"}
          <div class="preferences-pane">
            <div class="pref-card">
              <div class="pref-info">
                <span class="pref-label">Auto-Fade Annotations</span>
                <span class="pref-sub">Automatically clear drawn strokes after 3.5 seconds</span>
              </div>
              <label class="switch">
                <input
                  type="checkbox"
                  checked={autoFadeEnabled}
                  onchange={(e) => {
                    autoFadeEnabled = e.currentTarget.checked;
                    try { localStorage.setItem("pixeltrace_pref_autofade", String(autoFadeEnabled)); } catch {}
                    triggerToast(autoFadeEnabled ? "Auto-Fade Enabled (3.5s)" : "Auto-Fade Disabled");
                  }}
                />
                <span class="slider"></span>
              </label>
            </div>

            <div class="pref-card">
              <div class="pref-info">
                <span class="pref-label">Default Tool on Launch</span>
                <span class="pref-sub">Initial active annotation tool when opening the app</span>
              </div>
              <select
                class="pref-select"
                value={currentTool}
                onchange={(e) => {
                  currentTool = e.currentTarget.value as Tool;
                  try { localStorage.setItem("pixeltrace_pref_tool", currentTool); } catch {}
                }}
              >
                <option value="laser">Laser Pointer</option>
                <option value="pen">Pen Marker</option>
                <option value="highlighter">Highlighter</option>
                <option value="arrow">Arrow</option>
                <option value="rect">Rectangle</option>
                <option value="circle">Circle</option>
                <option value="stamp">Stamp</option>
                <option value="sticky">Sticky Note</option>
              </select>
            </div>

            <div class="pref-card">
              <div class="pref-info">
                <span class="pref-label">Default Stroke Thickness</span>
                <span class="pref-sub">Default line width for drawing tools</span>
              </div>
              <select
                class="pref-select"
                value={currentSize}
                onchange={(e) => {
                  currentSize = Number(e.currentTarget.value);
                  try { localStorage.setItem("pixeltrace_pref_size", String(currentSize)); } catch {}
                }}
              >
                <option value="2">Fine (2px)</option>
                <option value="4">Medium (4px)</option>
                <option value="8">Thick (8px)</option>
                <option value="14">Heavy (14px)</option>
              </select>
            </div>

            <div class="pref-card">
              <div class="pref-info">
                <span class="pref-label">Laser Beam Persistence</span>
                <span class="pref-sub">Smooth glowing trail decay duration</span>
              </div>
              <select
                class="pref-select"
                value={laserLifetimeMs}
                onchange={(e) => {
                  laserLifetimeMs = Number(e.currentTarget.value);
                  try { localStorage.setItem("pixeltrace_pref_laser_duration", String(laserLifetimeMs)); } catch {}
                }}
              >
                <option value="500">Snappy (500ms)</option>
                <option value="850">Balanced (850ms)</option>
                <option value="1300">Cinematic Trail (1.3s)</option>
              </select>
            </div>
          </div>
        {:else}
          <div class="about-pane">
            <img src="/app-logo.png" alt="PixelTrace" class="about-logo" />
            <h3 class="about-name">PixelTrace</h3>
            <span class="about-tagline">macOS Native Screen Annotator</span>
            <span class="about-ver">Version 0.1.0 • Built with Tauri & Svelte</span>
            <p class="about-desc">
              PixelTrace brings instant, fluid on-screen markup, continuous spline laser trails, persistent desktop sticky notes, multi-display targeting, and zero-latency ghost passthrough to macOS.
            </p>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: transparent !important;
    user-select: none;
    -webkit-user-select: none;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  }

  :global(body.ghost-active) {
    cursor: default !important;
  }

  .canvas-layer {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    pointer-events: auto;
    z-index: 10;
  }

  .canvas-layer.ghost-active {
    pointer-events: none !important;
    cursor: default !important;
  }

  .canvas-layer.tool-laser,
  .canvas-layer.tool-eraser {
    cursor: none;
  }

  .canvas-layer.tool-pen,
  .canvas-layer.tool-highlighter,
  .canvas-layer.tool-arrow,
  .canvas-layer.tool-rect,
  .canvas-layer.tool-circle,
  .canvas-layer.tool-line,
  .canvas-layer.tool-stamp,
  .canvas-layer.tool-sticky {
    cursor: crosshair;
  }

  .canvas-layer.tool-text {
    cursor: text;
  }

  /* Physical Skeuomorphic Sticky Notes */
  .sticky-note {
    position: fixed;
    z-index: 100;
    border-radius: 7px;
    border-width: 1px;
    border-style: solid;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.45), 0 8px 10px -6px rgba(0, 0, 0, 0.3);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    user-select: none;
    transition: box-shadow 0.2s ease, transform 0.15s ease;
    pointer-events: auto;
  }

  .sticky-note:hover {
    box-shadow: 0 16px 36px -4px rgba(0, 0, 0, 0.55), 0 10px 14px -5px rgba(0, 0, 0, 0.35);
  }

  .sticky-tape {
    position: absolute;
    top: -6px;
    left: 50%;
    transform: translateX(-50%);
    width: 52px;
    height: 16px;
    background: rgba(255, 255, 255, 0.48);
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
    border: 1px solid rgba(255, 255, 255, 0.4);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
    border-radius: 2px;
    z-index: 2;
    pointer-events: none;
  }

  .sticky-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px 6px 10px;
    border-bottom-width: 1px;
    border-bottom-style: solid;
    cursor: grab;
  }

  .sticky-header:active {
    cursor: grabbing;
  }

  .sticky-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    opacity: 0.85;
  }

  .sticky-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .sticky-action-btn {
    background: transparent;
    border: none;
    font-size: 11px;
    cursor: pointer;
    padding: 2px 5px;
    border-radius: 4px;
    opacity: 0.75;
    transition: opacity 0.15s, background-color 0.15s;
    line-height: 1;
  }

  .sticky-action-btn:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.1);
  }

  .sticky-body {
    width: 100%;
    height: 135px;
    background: transparent;
    border: none;
    outline: none;
    padding: 10px 12px;
    font-size: 13.5px;
    line-height: 1.5;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
    resize: both;
    box-sizing: border-box;
  }

  /* Inline Text Box */
  .inline-text-box {
    position: fixed;
    z-index: 100000;
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: rgba(18, 18, 24, 0.88);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 9px;
    padding: 6px 12px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.55);
  }

  .inline-text-box input {
    background: transparent;
    border: none;
    outline: none;
    font-weight: 700;
    min-width: 240px;
  }

  .text-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
  }

  /* Ghost Mode Toast */
  .ghost-toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 9999999;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: rgba(24, 24, 30, 0.92);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 20px;
    color: #f0f0f5;
    font-size: 12px;
    font-weight: 600;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
    pointer-events: none;
    animation: toast-in 0.2s ease-out;
  }

  .ghost-toast.active {
    background: rgba(142, 68, 173, 0.88);
    border-color: rgba(255, 255, 255, 0.3);
    box-shadow: 0 8px 28px rgba(142, 68, 173, 0.5);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 12px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }

  /* Floating Centered Toolbar Container */
  .widget-container {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 999999;
    pointer-events: auto;
    transition: transform 0.08s ease-out;
    filter: drop-shadow(0 14px 36px rgba(0, 0, 0, 0.5));
  }

  /* Floating Centered Toolbar Container */
  .widget-container {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 999999;
    pointer-events: auto;
    transition: transform 0.32s cubic-bezier(0.16, 1, 0.3, 1);
    filter: drop-shadow(0 14px 36px rgba(0, 0, 0, 0.5));
  }

  .widget-container.collapsed {
    transition: transform 0.35s cubic-bezier(0.16, 1, 0.3, 1);
  }

  /* Sleek Edge-Docked Half-Pill Tab matching screenshot & Apple HIG */
  .edge-dock-tab {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px 8px 6px;
    background: rgba(18, 18, 24, 0.9);
    backdrop-filter: blur(28px) saturate(190%);
    -webkit-backdrop-filter: blur(28px) saturate(190%);
    border: 1px solid rgba(255, 255, 255, 0.16);
    color: #ffffff;
    cursor: grab;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.55), 0 0 1px rgba(255, 255, 255, 0.3);
    transition: transform 0.18s cubic-bezier(0.16, 1, 0.3, 1), background-color 0.15s, border-color 0.15s;
    user-select: none;
  }

  .edge-dock-tab:active {
    cursor: grabbing;
  }

  .edge-dock-tab.dock-left {
    border-top-right-radius: 32px;
    border-bottom-right-radius: 32px;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    border-left: none;
    padding-right: 12px;
    padding-left: 6px;
  }

  .edge-dock-tab:hover {
    background: rgba(28, 28, 38, 0.95);
    border-color: rgba(255, 255, 255, 0.32);
  }

  .edge-dock-tab.dock-left:hover {
    transform: translateX(4px) scale(1.03);
  }

  .dock-chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.85);
  }

  .dock-logo-badge {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dock-app-logo {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    display: block;
    user-select: none;
    -webkit-user-drag: none;
    pointer-events: none;
  }

  /* Brand Logo in Toolbar */
  .bar-brand {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 4px 0 2px;
  }

  .bar-brand-logo {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    display: block;
    user-select: none;
    -webkit-user-drag: none;
    pointer-events: none;
  }

  .modal-brand-logo {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    display: block;
    user-select: none;
    -webkit-user-drag: none;
    pointer-events: none;
  }

  /* Ghost Mode Button */
  .ghost-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 9px;
    color: #e0e0e0;
    cursor: pointer;
    transition: all 0.18s ease;
  }

  .ghost-btn:hover {
    background: rgba(255, 255, 255, 0.15);
    color: #ffffff;
  }

  .ghost-btn.active {
    background: #af52de;
    border-color: rgba(255, 255, 255, 0.4);
    color: #ffffff;
    box-shadow: 0 0 16px rgba(175, 82, 222, 0.75);
  }

  .ghost-btn-text {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    line-height: 1.1;
  }

  .ghost-title {
    font-size: 11px;
    font-weight: 700;
  }

  /* Modern MarkerOn-Style Glassmorphic Bar */
  .glass-bar {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    background: rgba(22, 22, 28, 0.92);
    backdrop-filter: blur(28px) saturate(190%);
    -webkit-backdrop-filter: blur(28px) saturate(190%);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 16px;
    color: #e5e5ea;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }

  .drag-handle {
    cursor: grab;
    padding: 6px 4px;
    color: rgba(255, 255, 255, 0.4);
    display: flex;
    align-items: center;
    transition: color 0.15s;
  }

  .drag-handle:hover {
    color: rgba(255, 255, 255, 0.9);
  }

  .divider {
    width: 1px;
    height: 22px;
    background: rgba(255, 255, 255, 0.12);
    margin: 0 3px;
  }

  .btn-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .tool-btn {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 9px;
    color: rgba(255, 255, 255, 0.75);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .tool-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #ffffff;
  }

  .tool-btn.active {
    background: rgba(255, 255, 255, 0.18);
    border-color: rgba(255, 255, 255, 0.28);
    color: #ffffff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
  }

  .laser-indicator {
    position: absolute;
    bottom: 4px;
    right: 4px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    box-shadow: 0 0 6px currentColor;
  }

  .stamp-btn {
    position: relative;
  }

  .stamp-badge {
    position: absolute;
    top: 2px;
    right: 2px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    background: #007aff;
    color: white;
    font-weight: 700;
    font-size: 9px;
    border-radius: 50%;
  }

  /* Popover Anchor & Menus */
  .popover-anchor {
    position: relative;
    display: flex;
    align-items: center;
  }

  .size-trigger-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 8px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    color: #e5e5ea;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .size-trigger-btn:hover,
  .size-trigger-btn.active {
    background: rgba(255, 255, 255, 0.16);
    border-color: rgba(255, 255, 255, 0.35);
    color: #ffffff;
  }

  .size-dot-preview {
    background: #ffffff;
    border-radius: 50%;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }

  .size-val-label {
    font-size: 11px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .color-trigger-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    cursor: pointer;
    transition: all 0.15s ease;
    padding: 0;
  }

  .color-trigger-btn:hover,
  .color-trigger-btn.active {
    background: rgba(255, 255, 255, 0.16);
    border-color: rgba(255, 255, 255, 0.35);
  }

  .active-color-swatch {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.9);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
  }

  .popover-menu {
    position: absolute;
    top: calc(100% + 10px);
    left: 50%;
    transform: translateX(-50%);
    background: rgba(22, 22, 28, 0.96);
    backdrop-filter: blur(28px) saturate(190%);
    -webkit-backdrop-filter: blur(28px) saturate(190%);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 14px;
    padding: 12px 14px;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.65), 0 0 1px rgba(255, 255, 255, 0.2);
    z-index: 100000;
    display: flex;
    flex-direction: column;
    gap: 10px;
    animation: popoverIn 0.14s cubic-bezier(0.16, 1, 0.3, 1);
  }

  @keyframes popoverIn {
    from {
      opacity: 0;
      transform: translate(-50%, -6px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0) scale(1);
    }
  }

  .size-popover {
    width: 220px;
  }

  .color-popover {
    width: 220px;
  }

  .popover-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .popover-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: rgba(255, 255, 255, 0.6);
  }

  .popover-badge {
    font-size: 11px;
    font-weight: 700;
    color: #00c7be;
    background: rgba(0, 199, 190, 0.15);
    padding: 2px 7px;
    border-radius: 6px;
  }

  .size-presets-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }

  .size-preset-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 6px 4px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    color: #d1d1d6;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .size-preset-item:hover {
    background: rgba(255, 255, 255, 0.12);
    color: #ffffff;
  }

  .size-preset-item.active {
    background: rgba(0, 122, 255, 0.25);
    border-color: rgba(0, 122, 255, 0.6);
    color: #ffffff;
  }

  .size-dot {
    background: #ffffff;
    border-radius: 50%;
  }

  .preset-label {
    font-size: 10px;
    font-weight: 600;
  }

  .slider-container {
    padding: 4px 2px;
  }

  .stroke-range-slider {
    width: 100%;
    accent-color: #007aff;
    cursor: pointer;
    height: 4px;
  }

  .custom-color-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    cursor: pointer;
    padding: 2px 6px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    transition: all 0.15s ease;
  }

  .custom-color-btn:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .hidden-color-input {
    opacity: 0;
    width: 0;
    height: 0;
    position: absolute;
    pointer-events: none;
  }

  .custom-swatch {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.8);
  }

  .custom-text {
    font-size: 10px;
    font-weight: 600;
    color: #ffffff;
  }

  .color-swatches-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 8px;
    justify-items: center;
  }

  .color-chip-pop {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    transition: transform 0.15s ease, border-color 0.15s, box-shadow 0.15s;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
  }

  .color-chip-pop:hover {
    transform: scale(1.18);
  }

  .color-chip-pop.active {
    transform: scale(1.22);
    border-color: #ffffff;
    box-shadow: 0 0 12px rgba(255, 255, 255, 0.75);
  }

  /* Sleek Floating Tooltip */
  .custom-tooltip {
    position: fixed;
    transform: translateX(-50%);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: rgba(18, 18, 24, 0.94);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.6);
    color: #ffffff;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    pointer-events: none;
    z-index: 10000000;
    animation: tooltipFade 0.12s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .tooltip-title {
    color: #f3f4f6;
    font-weight: 600;
  }

  .tooltip-shortcut {
    background: rgba(255, 255, 255, 0.14);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 10px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, monospace;
    color: rgba(255, 255, 255, 0.9);
  }

  @keyframes tooltipFade {
    from {
      opacity: 0;
      transform: translate(-50%, -4px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }

  /* Icon Buttons */
  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    color: rgba(255, 255, 255, 0.75);
    transition: all 0.15s;
  }

  .icon-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.09);
    color: #ffffff;
  }

  .icon-btn.active {
    background: rgba(0, 199, 190, 0.25);
    border-color: rgba(0, 199, 190, 0.4);
    color: #00c7be;
  }

  .icon-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .icon-btn.danger:hover {
    background: rgba(255, 59, 48, 0.22);
    color: #ff3b30;
  }

  /* Multi-Monitor Switcher */
  .monitor-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .mon-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 4px 7px;
    font-size: 11px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 6px;
    color: #e0e0e0;
    cursor: pointer;
  }

  .mon-btn.active {
    background: #007aff;
    border-color: #007aff;
    color: white;
  }

  /* Ghost Mode Button */
  .ghost-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 9px;
    color: #e0e0e0;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .ghost-btn:hover {
    background: rgba(255, 255, 255, 0.14);
    color: #ffffff;
  }

  .ghost-btn.active {
    background: #af52de;
    border-color: #af52de;
    color: #ffffff;
    box-shadow: 0 0 14px rgba(175, 82, 222, 0.6);
  }

  .collapse-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    border-radius: 6px;
    transition: all 0.15s;
  }

  .collapse-btn:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.08);
  }

  /* Modal Dialog */
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(10px);
    z-index: 10000000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-card {
    background: rgba(20, 20, 28, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 18px;
    width: 480px;
    max-height: 520px;
    padding: 16px 20px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.75);
    color: #ffffff;
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 14px;
    padding-bottom: 10px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .modal-tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(255, 255, 255, 0.06);
    padding: 3px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .tab-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.65);
    background: transparent;
    border: none;
    border-radius: 7px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .tab-btn:hover {
    color: #ffffff;
  }

  .tab-btn.active {
    background: rgba(255, 255, 255, 0.15);
    color: #ffffff;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
  }

  .modal-close {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.55);
    cursor: pointer;
    padding: 5px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    transition: color 0.15s, background 0.15s;
  }

  .modal-close:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.1);
  }

  .modal-body {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-right: 4px;
  }

  .modal-body::-webkit-scrollbar {
    width: 5px;
  }

  .modal-body::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.15);
    border-radius: 10px;
  }

  /* Shortcuts Pane */
  .shortcuts-scrollable {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .shortcut-group {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .group-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #00c7be;
    margin-bottom: 2px;
  }

  .shortcut-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
    color: #d1d1d6;
    padding: 2px 0;
  }

  kbd {
    background: rgba(255, 255, 255, 0.12);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 5px;
    padding: 2px 7px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: #ffffff;
  }

  /* Preferences Pane */
  .preferences-pane {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .pref-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 11px;
    gap: 12px;
  }

  .pref-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pref-label {
    font-size: 13px;
    font-weight: 600;
    color: #f3f4f6;
  }

  .pref-sub {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
  }

  .pref-select {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    color: #ffffff;
    font-size: 12px;
    font-weight: 500;
    padding: 5px 10px;
    outline: none;
    cursor: pointer;
  }

  .pref-select option {
    background: #1c1c24;
    color: #ffffff;
  }

  /* Apple Switch */
  .switch {
    position: relative;
    display: inline-block;
    width: 38px;
    height: 22px;
    flex-shrink: 0;
  }

  .switch input {
    opacity: 0;
    width: 0;
    height: 0;
  }

  .slider {
    position: absolute;
    cursor: pointer;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-color: rgba(255, 255, 255, 0.16);
    transition: 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    border-radius: 34px;
    border: 1px solid rgba(255, 255, 255, 0.2);
  }

  .slider:before {
    position: absolute;
    content: "";
    height: 16px;
    width: 16px;
    left: 2px;
    bottom: 2px;
    background-color: white;
    transition: 0.2s cubic-bezier(0.16, 1, 0.3, 1);
    border-radius: 50%;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
  }

  input:checked + .slider {
    background-color: #34c759;
    border-color: #34c759;
  }

  input:checked + .slider:before {
    transform: translateX(16px);
  }

  /* About Pane */
  .about-pane {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 16px 12px;
    gap: 8px;
  }

  .about-logo {
    width: 64px;
    height: 64px;
    border-radius: 14px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
  }

  .about-name {
    margin: 4px 0 0 0;
    font-size: 18px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .about-tagline {
    font-size: 12px;
    color: #00c7be;
    font-weight: 600;
  }

  .about-ver {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.45);
  }

  .about-desc {
    margin: 8px 0 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: rgba(255, 255, 255, 0.7);
    max-width: 360px;
  }
</style>
