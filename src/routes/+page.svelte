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
    Undo2,
    Redo2,
    Camera,
    Hourglass,
    Trash2,
    Ghost,
    Keyboard,
    GripVertical,
    Monitor,
    ChevronLeft,
    X
  } from "lucide-svelte";

  // Tool types
  type Tool = "laser" | "pen" | "highlighter" | "arrow" | "rect" | "circle" | "line" | "stamp" | "text";

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

  // Settings / Help modal
  let showHelpModal = $state(false);

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
  const LASER_LIFETIME_MS = 850;

  // Floating Widget positioning & dragging
  let toolbarRef = $state<HTMLDivElement | null>(null);
  let widgetX = $state(100);
  let widgetY = $state(20);
  let isDragging = false;
  let dragOffset = { x: 0, y: 0 };
  let isDocked = $state(false);
  let dockEdge = $state<"left" | "right" | "top" | "bottom">("top");
  let isCollapsed = $state(false);

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

    let unlistenGhost: UnlistenFn | undefined;
    let unlistenClear: UnlistenFn | undefined;

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
      } catch (e) {
        console.warn("Could not attach event listeners:", e);
      }
    })();

    return () => {
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("keydown", handleKeyDown);
      if (laserAnimFrame) cancelAnimationFrame(laserAnimFrame);
      if (unlistenGhost) unlistenGhost();
      if (unlistenClear) unlistenClear();
    };
  });

  function centerToolbar() {
    tick().then(() => {
      const width = toolbarRef ? toolbarRef.offsetWidth : 940;
      widgetX = Math.max(16, Math.round((window.innerWidth - width) / 2));
      widgetY = 20;
    });
  }

  function handleResize() {
    initCanvases();
    centerToolbar();
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

  // --- CONTINUOUS GLOWING LASER POINTER ENGINE ---
  function renderLaserLoop(now: number) {
    if (!dynamicCtx) return;

    // Prune expired laser points
    laserPoints = laserPoints.filter((p) => now - p.time < LASER_LIFETIME_MS);

    // Only clear dynamicCtx if we are using the laser or if trail points remain
    if (currentTool === "laser" || laserPoints.length > 0) {
      dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    }

    // Render continuous tapered glowing beam trail
    if (laserPoints.length >= 2) {
      dynamicCtx.save();
      dynamicCtx.lineCap = "round";
      dynamicCtx.lineJoin = "round";

      // Pass 1: Wide Neon Bloom with progressive alpha decay
      for (let i = 0; i < laserPoints.length - 1; i++) {
        const p0 = laserPoints[i];
        const p1 = laserPoints[i + 1];
        const age = now - p1.time;
        const progress = Math.max(0, Math.min(1, 1 - age / LASER_LIFETIME_MS));
        const width = Math.max(2, (currentSize * 3.6 + 6) * Math.pow(progress, 1.2));
        const alpha = Math.pow(progress, 0.8) * 0.45;

        dynamicCtx.beginPath();
        dynamicCtx.moveTo(p0.x, p0.y);
        dynamicCtx.lineTo(p1.x, p1.y);
        dynamicCtx.lineWidth = width;
        dynamicCtx.strokeStyle = `rgba(${hexToRgb(currentColor)}, ${alpha})`;
        dynamicCtx.shadowColor = currentColor;
        dynamicCtx.shadowBlur = 16 * progress;
        dynamicCtx.stroke();
      }

      // Pass 2: Saturated Neon Beam Core
      for (let i = 0; i < laserPoints.length - 1; i++) {
        const p0 = laserPoints[i];
        const p1 = laserPoints[i + 1];
        const age = now - p1.time;
        const progress = Math.max(0, Math.min(1, 1 - age / LASER_LIFETIME_MS));
        const width = Math.max(1.5, (currentSize * 1.6 + 2) * Math.pow(progress, 1.1));
        const alpha = Math.pow(progress, 0.9) * 0.9;

        dynamicCtx.beginPath();
        dynamicCtx.moveTo(p0.x, p0.y);
        dynamicCtx.lineTo(p1.x, p1.y);
        dynamicCtx.lineWidth = width;
        dynamicCtx.strokeStyle = `rgba(${hexToRgb(currentColor)}, ${alpha})`;
        dynamicCtx.shadowColor = currentColor;
        dynamicCtx.shadowBlur = 6 * progress;
        dynamicCtx.stroke();
      }

      // Pass 3: White-Hot Center Spine
      for (let i = 0; i < laserPoints.length - 1; i++) {
        const p0 = laserPoints[i];
        const p1 = laserPoints[i + 1];
        const age = now - p1.time;
        const progress = Math.max(0, Math.min(1, 1 - age / LASER_LIFETIME_MS));
        const width = Math.max(1, (currentSize * 0.5 + 1) * Math.pow(progress, 1.3));
        const alpha = Math.pow(progress, 1.1) * 0.95;

        dynamicCtx.beginPath();
        dynamicCtx.moveTo(p0.x, p0.y);
        dynamicCtx.lineTo(p1.x, p1.y);
        dynamicCtx.lineWidth = width;
        dynamicCtx.strokeStyle = `rgba(255, 255, 255, ${alpha})`;
        dynamicCtx.shadowBlur = 0;
        dynamicCtx.stroke();
      }

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
    if (target && (target.closest(".widget-container") || target.closest(".inline-text-box") || target.closest(".modal-backdrop"))) return;

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

  async function toggleGhostMode() {
    try {
      const newState = await invoke<boolean>("toggle_ghost_mode");
      isGhostMode = newState;
      triggerGhostToast(isGhostMode);
    } catch (err) {
      isGhostMode = !isGhostMode;
      await invoke("set_click_through", { ignore: isGhostMode });
      triggerGhostToast(isGhostMode);
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

    if (e.key === "Escape") {
      if (showHelpModal) {
        showHelpModal = false;
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
    if (key === "n" || key === "7") currentTool = "line";
    if (key === "s" || key === "8") currentTool = "stamp";
    if (key === "t" || key === "9") currentTool = "text";

    if (e.key === " ") {
      e.preventDefault();
      isCollapsed = !isCollapsed;
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
  }

  function onDragEnd() {
    isDragging = false;
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragEnd);

    const w = window.innerWidth;
    const h = window.innerHeight;
    const barWidth = toolbarRef ? toolbarRef.offsetWidth : 940;
    const barHeight = toolbarRef ? toolbarRef.offsetHeight : 54;

    widgetX = Math.max(12, Math.min(w - barWidth - 12, widgetX));
    widgetY = Math.max(12, Math.min(h - barHeight - 12, widgetY));

    isDocked = widgetY <= 16;
  }
</script>

<!-- Global Transparent Drawing Surfaces -->
<canvas
  bind:this={staticCanvas}
  class="canvas-layer"
></canvas>

<canvas
  bind:this={dynamicCanvas}
  class="canvas-layer tool-{currentTool}"
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

<!-- Floating Centered Toolbar -->
<div
  bind:this={toolbarRef}
  class="widget-container"
  class:docked={isDocked}
  class:collapsed={isCollapsed}
  style="transform: translate({widgetX}px, {widgetY}px);"
  role="region"
  aria-label="PixelTrace Screen Toolbar"
>
  {#if isCollapsed}
    <!-- Collapsed Edge Pill -->
    <button
      class="edge-pill"
      onclick={() => (isCollapsed = false)}
      title="Expand PixelTrace Toolbar (Space)"
    >
      <span class="pill-dot" style="background-color: {currentColor}"></span>
      <span class="pill-text">PixelTrace</span>
    </button>
  {:else}
    <!-- Full Modern Glassmorphic Toolbar -->
    <div class="glass-bar">
      <!-- Drag Handle -->
      <div
        class="drag-handle"
        onmousedown={onDragStart}
        role="button"
        tabindex="0"
        title="Drag toolbar anywhere"
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
          title="Laser Pointer (L or 1) — Continuous glowing trail"
        >
          <Sparkles size={17} />
          <span class="laser-indicator" style="background-color: {currentColor}"></span>
        </button>

        <!-- Pen -->
        <button
          class="tool-btn"
          class:active={currentTool === "pen"}
          onclick={() => (currentTool = "pen")}
          title="Smooth Pen (P or 2)"
        >
          <Pencil size={17} />
        </button>

        <!-- Highlighter -->
        <button
          class="tool-btn"
          class:active={currentTool === "highlighter"}
          onclick={() => (currentTool = "highlighter")}
          title="Highlighter (H or 3)"
        >
          <Highlighter size={17} />
        </button>

        <!-- Arrow -->
        <button
          class="tool-btn"
          class:active={currentTool === "arrow"}
          onclick={() => (currentTool = "arrow")}
          title="Arrow (A or 4)"
        >
          <ArrowUpRight size={18} />
        </button>

        <!-- Rectangle -->
        <button
          class="tool-btn"
          class:active={currentTool === "rect"}
          onclick={() => (currentTool = "rect")}
          title="Rectangle (R or 5)"
        >
          <Square size={16} />
        </button>

        <!-- Circle -->
        <button
          class="tool-btn"
          class:active={currentTool === "circle"}
          onclick={() => (currentTool = "circle")}
          title="Circle (C or 6)"
        >
          <Circle size={16} />
        </button>

        <!-- Line -->
        <button
          class="tool-btn"
          class:active={currentTool === "line"}
          onclick={() => (currentTool = "line")}
          title="Straight Line (N or 7)"
        >
          <Slash size={16} />
        </button>

        <!-- Text -->
        <button
          class="tool-btn"
          class:active={currentTool === "text"}
          onclick={() => (currentTool = "text")}
          title="Text Note (T or 9) — Click anywhere to write"
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
          title="Numbered Stamp (S or 8). Right-click to reset #{stampCounter}"
        >
          <ListOrdered size={16} />
          <span class="stamp-badge">{stampCounter}</span>
        </button>
      </div>

      <div class="divider"></div>

      <!-- Stroke Size Presets -->
      <div class="size-group">
        {#each sizePresets as s}
          <button
            class="size-chip"
            class:active={currentSize === s.val}
            onclick={() => (currentSize = s.val)}
            title="{s.label} stroke ({s.val}px)"
          >
            <span class="size-dot" style="width: {s.val + 2}px; height: {s.val + 2}px;"></span>
          </button>
        {/each}
      </div>

      <div class="divider"></div>

      <!-- Color Palette -->
      <div class="color-palette">
        {#each colors as c}
          <button
            class="color-chip"
            class:active={currentColor === c.hex}
            style="background-color: {c.hex}"
            onclick={() => (currentColor = c.hex)}
            title={c.name}
          ></button>
        {/each}
      </div>

      <div class="divider"></div>

      <!-- Actions Group -->
      <div class="btn-group">
        <button
          class="icon-btn"
          disabled={history.length === 0}
          onclick={undo}
          title="Undo (⌘Z)"
        >
          <Undo2 size={16} />
        </button>

        <button
          class="icon-btn"
          disabled={redoStack.length === 0}
          onclick={redo}
          title="Redo (⌘⇧Z / ⌘Y)"
        >
          <Redo2 size={16} />
        </button>

        <button
          class="icon-btn"
          onclick={captureSnapshot}
          title="Copy Screenshot to Clipboard"
        >
          <Camera size={16} />
        </button>

        <button
          class="icon-btn"
          class:active={autoFadeEnabled}
          onclick={() => (autoFadeEnabled = !autoFadeEnabled)}
          title="Auto-Fade Markups (3.5s): {autoFadeEnabled ? 'ON' : 'OFF'}"
        >
          <Hourglass size={16} />
        </button>

        <button
          class="icon-btn danger"
          onclick={clearCanvas}
          title="Clear Screen Markups (Esc)"
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
              title="Switch overlay to Monitor {idx + 1}"
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
        title="Ghost Mode (⌘⇧X / ⌘⇧G / X) — Pass clicks through to background apps"
      >
        <Ghost size={15} />
        <span class="ghost-label">{isGhostMode ? "Ghost" : "Draw"}</span>
      </button>

      <!-- Keyboard Shortcuts / Help Button -->
      <button
        class="icon-btn"
        onclick={() => (showHelpModal = !showHelpModal)}
        title="Keyboard Shortcuts & About"
      >
        <Keyboard size={16} />
      </button>

      <!-- Collapse Button -->
      <button
        class="collapse-btn"
        onclick={() => (isCollapsed = true)}
        title="Collapse into edge pill (Space)"
      >
        <ChevronLeft size={16} />
      </button>
    </div>
  {/if}
</div>

<!-- Shortcuts Modal -->
{#if showHelpModal}
  <div
    class="modal-backdrop"
    role="dialog"
    aria-modal="true"
    tabindex="0"
    onclick={(e) => {
      if (e.target === e.currentTarget) showHelpModal = false;
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") showHelpModal = false;
    }}
  >
    <div
      class="modal-card"
      role="document"
    >
      <div class="modal-header">
        <div class="modal-title">
          <Sparkles size={18} />
          <h3>PixelTrace Shortcuts</h3>
        </div>
        <button
          class="modal-close"
          onclick={() => (showHelpModal = false)}
          aria-label="Close dialog"
        >
          <X size={16} />
        </button>
      </div>

      <div class="modal-body">
        <div class="shortcut-row">
          <span>Toggle Overlay Visibility (Hide/Show)</span>
          <kbd>⌘ + Shift + D</kbd>
        </div>
        <div class="shortcut-row">
          <span>Toggle Ghost Mode (Global Passthrough)</span>
          <kbd>⌘ + Shift + X / ⌘⇧G</kbd>
        </div>
        <div class="shortcut-row">
          <span>Ghost Mode Quick Toggle (when focused)</span>
          <kbd>X / G</kbd>
        </div>
        <div class="shortcut-row">
          <span>Laser Pointer (Continuous Trail)</span>
          <kbd>L / 1</kbd>
        </div>
        <div class="shortcut-row">
          <span>Smooth Pen / Highlighter</span>
          <kbd>P / H</kbd>
        </div>
        <div class="shortcut-row">
          <span>Arrow / Rect / Circle / Line</span>
          <kbd>A / R / C / N</kbd>
        </div>
        <div class="shortcut-row">
          <span>Numbered Stamp / Text Note</span>
          <kbd>S / T</kbd>
        </div>
        <div class="shortcut-row">
          <span>Collapse / Expand Toolbar</span>
          <kbd>Space</kbd>
        </div>
        <div class="shortcut-row">
          <span>Undo / Redo</span>
          <kbd>⌘Z / ⌘⇧Z</kbd>
        </div>
        <div class="shortcut-row">
          <span>Clear All Screen Markups</span>
          <kbd>Esc</kbd>
        </div>
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

  .canvas-layer {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    pointer-events: auto;
    z-index: 10;
  }

  .canvas-layer.tool-laser {
    cursor: none;
  }

  .canvas-layer.tool-pen,
  .canvas-layer.tool-highlighter,
  .canvas-layer.tool-arrow,
  .canvas-layer.tool-rect,
  .canvas-layer.tool-circle,
  .canvas-layer.tool-line,
  .canvas-layer.tool-stamp {
    cursor: crosshair;
  }

  .canvas-layer.tool-text {
    cursor: text;
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

  .widget-container.collapsed {
    transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }

  /* Sleek Collapsed Pill */
  .edge-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: rgba(20, 20, 26, 0.92);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 24px;
    color: #ffffff;
    cursor: pointer;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    transition: transform 0.15s ease, background-color 0.15s;
  }

  .edge-pill:hover {
    transform: scale(1.05);
    background: rgba(28, 28, 36, 0.96);
  }

  .pill-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    box-shadow: 0 0 10px currentColor;
  }

  .pill-text {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.4px;
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

  /* Stroke Size Presets */
  .size-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .size-chip {
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 5px;
    cursor: pointer;
    padding: 0;
  }

  .size-chip:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .size-chip.active {
    border-color: rgba(255, 255, 255, 0.45);
    background: rgba(255, 255, 255, 0.16);
  }

  .size-dot {
    background: #ffffff;
    border-radius: 50%;
  }

  /* Color Palette */
  .color-palette {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .color-chip {
    width: 17px;
    height: 17px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
    transition: transform 0.15s, border-color 0.15s, box-shadow 0.15s;
  }

  .color-chip:hover {
    transform: scale(1.15);
  }

  .color-chip.active {
    transform: scale(1.22);
    border-color: #ffffff;
    box-shadow: 0 0 10px rgba(255, 255, 255, 0.7);
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
    background: rgba(24, 24, 30, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 16px;
    width: 390px;
    padding: 18px 22px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.65);
    color: #ffffff;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
  }

  .modal-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .modal-title h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
  }

  .modal-close {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    padding: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    transition: color 0.15s;
  }

  .modal-close:hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.08);
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .shortcut-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    color: #d1d1d6;
  }

  kbd {
    background: rgba(255, 255, 255, 0.12);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 5px;
    padding: 3px 8px;
    font-size: 11px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: #ffffff;
  }
</style>
