<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  // Tool types
  type Tool = "laser" | "pen" | "highlighter" | "arrow" | "rect" | "circle" | "line" | "stamp" | "text";

  interface Point {
    x: number;
    y: number;
    time?: number;
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
  let isGhostMode = $state(false); // Click-through
  let stampCounter = $state(1);

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

  // Laser trail queue
  let laserTrail: Point[] = [];
  let laserRafId: number | null = null;
  const LASER_DECAY_MS = 650;

  // Floating Widget state
  let widgetX = $state(80);
  let widgetY = $state(60);
  let isDragging = false;
  let dragOffset = { x: 0, y: 0 };
  let isDocked = $state(false);
  let dockEdge = $state<"left" | "right" | "top" | "bottom">("left");
  let isCollapsed = $state(false);

  // Stroke Size Presets
  const sizePresets = [
    { label: "Fine", val: 2 },
    { label: "Med", val: 4 },
    { label: "Thick", val: 8 },
    { label: "Heavy", val: 14 }
  ];

  // Color Palette
  const colors = [
    { name: "Neon Red", hex: "#FF3B30" },
    { name: "Electric Yellow", hex: "#FFCC00" },
    { name: "Neon Green", hex: "#34C759" },
    { name: "Cyan Blue", hex: "#00C7BE" },
    { name: "Electric Purple", hex: "#AF52DE" },
    { name: "Pure White", hex: "#FFFFFF" }
  ];

  onMount(async () => {
    initCanvases();
    window.addEventListener("resize", handleResize);
    window.addEventListener("keydown", handleKeyDown);

    // Fetch system monitors
    try {
      const res = await invoke<MonitorInfo[]>("get_monitors");
      if (Array.isArray(res)) {
        monitors = res;
      }
    } catch (e) {
      console.warn("Could not fetch monitors:", e);
    }

    return () => {
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("keydown", handleKeyDown);
      if (laserRafId) cancelAnimationFrame(laserRafId);
    };
  });

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

  function handleResize() {
    initCanvases();
  }

  // --- LASER POINTER ENGINE ---
  function updateLaserTrail(x: number, y: number) {
    const now = performance.now();
    laserTrail.push({ x, y, time: now });
    if (!laserRafId) {
      laserRafId = requestAnimationFrame(renderLaserLoop);
    }
  }

  function renderLaserLoop(now: number) {
    if (!dynamicCtx) return;
    dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);

    // Filter points older than decay limit
    laserTrail = laserTrail.filter((p) => now - (p.time || 0) < LASER_DECAY_MS);

    if (laserTrail.length > 0) {
      for (let i = 0; i < laserTrail.length; i++) {
        const p = laserTrail[i];
        const age = now - (p.time || 0);
        const progress = 1 - age / LASER_DECAY_MS;

        const radius = Math.max(2, (currentSize + 2) * progress);
        dynamicCtx.beginPath();
        dynamicCtx.arc(p.x, p.y, radius, 0, Math.PI * 2);
        dynamicCtx.fillStyle = `rgba(${hexToRgb(currentColor)}, ${progress * 0.85})`;
        dynamicCtx.shadowColor = currentColor;
        dynamicCtx.shadowBlur = 14 * progress;
        dynamicCtx.fill();
      }

      // Draw high-intensity glowing cursor head
      const head = laserTrail[laserTrail.length - 1];
      dynamicCtx.beginPath();
      dynamicCtx.arc(head.x, head.y, currentSize + 4, 0, Math.PI * 2);
      dynamicCtx.fillStyle = "#FFFFFF";
      dynamicCtx.shadowColor = currentColor;
      dynamicCtx.shadowBlur = 22;
      dynamicCtx.fill();

      dynamicCtx.beginPath();
      dynamicCtx.arc(head.x, head.y, currentSize + 1, 0, Math.PI * 2);
      dynamicCtx.fillStyle = currentColor;
      dynamicCtx.fill();

      laserRafId = requestAnimationFrame(renderLaserLoop);
    } else {
      dynamicRafReset();
    }
  }

  function dynamicRafReset() {
    laserRafId = null;
    if (dynamicCtx && !isDrawing) {
      dynamicCtx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    }
  }

  function hexToRgb(hex: string): string {
    const clean = hex.replace("#", "");
    const bigint = parseInt(clean, 16);
    const r = (bigint >> 16) & 255;
    const g = (bigint >> 8) & 255;
    const b = bigint & 255;
    return `${r}, ${g}, ${b}`;
  }

  // --- MOUSE & DRAWING HANDLERS ---
  function onPointerDown(e: PointerEvent) {
    if (isGhostMode) return;
    const target = e.target as HTMLElement;
    if (target && (target.closest(".widget-container") || target.closest(".inline-text-box"))) return;

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

    // Commit any open text if user clicked away
    if (isTextActive) {
      commitText();
    }

    isDrawing = true;
    startPoint = { x: e.clientX, y: e.clientY };

    if (currentTool === "laser") {
      updateLaserTrail(e.clientX, e.clientY);
    } else if (currentTool === "pen" || currentTool === "highlighter") {
      currentStrokePoints = [{ x: e.clientX, y: e.clientY }];
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
    if (currentTool === "laser") {
      updateLaserTrail(e.clientX, e.clientY);
      return;
    }

    if (!isDrawing || isGhostMode) return;

    if (currentTool === "pen" || currentTool === "highlighter") {
      currentStrokePoints.push({ x: e.clientX, y: e.clientY });
      renderCurrentFreehand(dynamicCtx);
    } else if (["arrow", "rect", "circle", "line"].includes(currentTool)) {
      renderShapePreview(dynamicCtx, startPoint, { x: e.clientX, y: e.clientY }, currentTool);
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!isDrawing || isGhostMode) return;
    isDrawing = false;

    if (currentTool === "pen" || currentTool === "highlighter") {
      if (currentStrokePoints.length > 0) {
        const item: DrawItem = {
          id: crypto.randomUUID(),
          tool: currentTool,
          color: currentColor,
          size: currentTool === "highlighter" ? currentSize * 3.5 : currentSize,
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
      if (dist > 5) {
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

  function addItemToHistory(item: DrawItem) {
    history.push(item);
    redoStack = [];
    redrawStaticCanvas();

    // Auto-fade timer if active
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
        size: Math.max(16, currentSize * 4),
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
    if (currentStrokePoints.length < 2) return;

    ctx.save();
    ctx.beginPath();
    ctx.moveTo(currentStrokePoints[0].x, currentStrokePoints[0].y);

    // Bézier curve smoothing for natural fluid strokes
    for (let i = 1; i < currentStrokePoints.length - 1; i++) {
      const xc = (currentStrokePoints[i].x + currentStrokePoints[i + 1].x) / 2;
      const yc = (currentStrokePoints[i].y + currentStrokePoints[i + 1].y) / 2;
      ctx.quadraticCurveTo(currentStrokePoints[i].x, currentStrokePoints[i].y, xc, yc);
    }

    ctx.strokeStyle = currentColor;
    ctx.lineWidth = currentTool === "highlighter" ? currentSize * 3.5 : currentSize;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    if (currentTool === "highlighter") {
      ctx.globalAlpha = 0.38;
      ctx.globalCompositeOperation = "source-over";
    }

    ctx.stroke();
    ctx.restore();
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
      ctx.strokeRect(start.x, start.y, end.x - start.x, end.y - start.y);
    } else if (tool === "circle") {
      const radiusX = Math.abs(end.x - start.x) / 2;
      const radiusY = Math.abs(end.y - start.y) / 2;
      const centerX = Math.min(start.x, end.x) + radiusX;
      const centerY = Math.min(start.y, end.y) + radiusY;
      ctx.beginPath();
      ctx.ellipse(centerX, centerY, radiusX, radiusY, 0, 0, Math.PI * 2);
      ctx.stroke();
    } else if (tool === "arrow") {
      ctx.beginPath();
      ctx.moveTo(start.x, start.y);
      ctx.lineTo(end.x, end.y);
      ctx.stroke();

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
      if ((item.tool === "pen" || item.tool === "highlighter") && item.points && item.points.length > 1) {
        staticCtx.beginPath();
        staticCtx.moveTo(item.points[0].x, item.points[0].y);
        for (let i = 1; i < item.points.length - 1; i++) {
          const xc = (item.points[i].x + item.points[i + 1].x) / 2;
          const yc = (item.points[i].y + item.points[i + 1].y) / 2;
          staticCtx.quadraticCurveTo(item.points[i].x, item.points[i].y, xc, yc);
        }
        staticCtx.strokeStyle = item.color;
        staticCtx.lineWidth = item.size;
        staticCtx.lineCap = "round";
        staticCtx.lineJoin = "round";
        if (item.tool === "highlighter") {
          staticCtx.globalAlpha = 0.38;
        }
        staticCtx.stroke();
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
        staticCtx.font = "bold 15px -apple-system, BlinkMacSystemFont, sans-serif";
        staticCtx.textAlign = "center";
        staticCtx.textBaseline = "middle";
        staticCtx.fillText(String(item.stampNumber), item.start.x, item.start.y + 1);
      } else if (item.tool === "text" && item.text && item.textPoint) {
        staticCtx.font = `bold ${item.size}px -apple-system, BlinkMacSystemFont, sans-serif`;
        // Dark outline for legibility
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
    isGhostMode = !isGhostMode;
    try {
      await invoke("set_click_through", { ignore: isGhostMode });
    } catch (err) {
      console.error("Failed to toggle click through:", err);
    }
  }

  async function onWidgetMouseEnter() {
    if (isGhostMode) {
      try {
        await invoke("set_click_through", { ignore: false });
      } catch (err) {}
    }
  }

  async function onWidgetMouseLeave() {
    if (isGhostMode) {
      try {
        await invoke("set_click_through", { ignore: true });
      } catch (err) {}
    }
  }

  // --- CAPTURE SCREENSHOT ---
  function captureSnapshot() {
    if (!staticCanvas) return;
    try {
      staticCanvas.toBlob((blob) => {
        if (blob) {
          navigator.clipboard.write([new ClipboardItem({ "image/png": blob })])
            .then(() => alert("✅ Annotated screenshot copied to clipboard!"))
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
    } catch (e) {
      console.error("Failed to switch monitor:", e);
    }
  }

  // --- SHORTCUTS ---
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
      clearCanvas();
      return;
    }

    if (key === "x") {
      toggleGhostMode();
      return;
    }

    if (key === "l" || key === "1") currentTool = "laser";
    if (key === "p" || key === "2") currentTool = "pen";
    if (key === "h" || key === "3") currentTool = "highlighter";
    if (key === "a" || key === "4") currentTool = "arrow";
    if (key === "r" || key === "5") currentTool = "rect";
    if (key === "c" || key === "6") currentTool = "circle";
    if (key === "s" || key === "7") currentTool = "stamp";
    if (key === "t" || key === "8") currentTool = "text";

    if (e.key === " ") {
      e.preventDefault();
      isCollapsed = !isCollapsed;
    }
  }

  // --- FLOATING WIDGET DRAGGING & DOCKING ---
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

    const threshold = 50;
    const w = window.innerWidth;
    const h = window.innerHeight;

    if (widgetX < threshold) {
      widgetX = 12;
      isDocked = true;
      dockEdge = "left";
    } else if (widgetX > w - 620 - threshold) {
      widgetX = w - 630;
      isDocked = true;
      dockEdge = "right";
    } else if (widgetY < threshold) {
      widgetY = 12;
      isDocked = true;
      dockEdge = "top";
    } else if (widgetY > h - 70 - threshold) {
      widgetY = h - 70;
      isDocked = true;
      dockEdge = "bottom";
    } else {
      isDocked = false;
    }
  }
</script>

<!-- Global Transparent Drawing Surfaces -->
<canvas
  bind:this={staticCanvas}
  class="canvas-layer"
></canvas>

<canvas
  bind:this={dynamicCanvas}
  class="canvas-layer"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
></canvas>

<!-- Inline Editable Text Box -->
{#if isTextActive}
  <div
    class="inline-text-box"
    style="left: {textPos.x}px; top: {textPos.y - 20}px;"
  >
    <input
      bind:this={textInputRef}
      bind:value={textInput}
      style="color: {currentColor}; font-size: {Math.max(16, currentSize * 4)}px;"
      placeholder="Type note and press Enter..."
    />
    <div class="text-hint">Press Enter to place, Esc to cancel</div>
  </div>
{/if}

<!-- Floating Edge-Docking Widget Container -->
<div
  class="widget-container"
  class:docked={isDocked}
  class:collapsed={isCollapsed}
  style="transform: translate({widgetX}px, {widgetY}px);"
  role="region"
  aria-label="PixelTrace Controls"
  onmouseenter={onWidgetMouseEnter}
  onmouseleave={onWidgetMouseLeave}
>
  {#if isCollapsed}
    <!-- Collapsed Edge Pill (Tab) -->
    <button
      class="edge-pill"
      onclick={() => (isCollapsed = false)}
      title="Expand PixelTrace Toolbar (Space)"
    >
      <span class="pill-glow" style="background-color: {currentColor}"></span>
      <span class="pill-label">PT</span>
    </button>
  {:else}
    <!-- Expanded Floating Glassmorphic Toolbar -->
    <div class="glass-bar">
      <!-- Drag Handle -->
      <div
        class="drag-handle"
        onmousedown={onDragStart}
        role="button"
        tabindex="0"
        title="Drag toolbar anywhere (Snap to edges)"
      >
        <svg width="14" height="20" viewBox="0 0 14 20" fill="currentColor">
          <circle cx="4" cy="4" r="1.5" />
          <circle cx="10" cy="4" r="1.5" />
          <circle cx="4" cy="10" r="1.5" />
          <circle cx="10" cy="10" r="1.5" />
          <circle cx="4" cy="16" r="1.5" />
          <circle cx="10" cy="16" r="1.5" />
        </svg>
      </div>

      <div class="divider"></div>

      <!-- Tools Group -->
      <div class="btn-group">
        <!-- Laser Pointer -->
        <button
          class="tool-btn"
          class:active={currentTool === "laser"}
          onclick={() => (currentTool = "laser")}
          title="Laser Pointer (L or 1) — Decaying trail"
        >
          <span class="laser-dot" style="background-color: {currentColor}"></span>
          <span class="label">Laser</span>
        </button>

        <!-- Pen -->
        <button
          class="tool-btn"
          class:active={currentTool === "pen"}
          onclick={() => (currentTool = "pen")}
          title="Smooth Pen (P or 2)"
        >
          ✏️
        </button>

        <!-- Highlighter -->
        <button
          class="tool-btn"
          class:active={currentTool === "highlighter"}
          onclick={() => (currentTool = "highlighter")}
          title="Highlighter (H or 3)"
        >
          🖍️
        </button>

        <!-- Arrow -->
        <button
          class="tool-btn"
          class:active={currentTool === "arrow"}
          onclick={() => (currentTool = "arrow")}
          title="Arrow (A or 4)"
        >
          ↗️
        </button>

        <!-- Rectangle -->
        <button
          class="tool-btn"
          class:active={currentTool === "rect"}
          onclick={() => (currentTool = "rect")}
          title="Rectangle (R or 5)"
        >
          🔲
        </button>

        <!-- Circle -->
        <button
          class="tool-btn"
          class:active={currentTool === "circle"}
          onclick={() => (currentTool = "circle")}
          title="Circle (C or 6)"
        >
          ⭕
        </button>

        <!-- Text Tool -->
        <button
          class="tool-btn"
          class:active={currentTool === "text"}
          onclick={() => (currentTool = "text")}
          title="Text Note (T or 8) — Click anywhere to type"
        >
          🔤
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
          title="Numbered Stamp (S or 7). Right-click to reset #{stampCounter}"
        >
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

      <!-- Undo, Redo, Snapshot, Clear -->
      <div class="btn-group">
        <button
          class="icon-btn"
          disabled={history.length === 0}
          onclick={undo}
          title="Undo (Ctrl/Cmd+Z)"
        >
          ↩️
        </button>

        <button
          class="icon-btn"
          disabled={redoStack.length === 0}
          onclick={redo}
          title="Redo (Ctrl/Cmd+Y)"
        >
          ↪️
        </button>

        <!-- Capture Snapshot -->
        <button
          class="icon-btn"
          onclick={captureSnapshot}
          title="Copy Screenshot to Clipboard"
        >
          📸
        </button>

        <!-- Auto Fade Toggle -->
        <button
          class="icon-btn"
          class:active={autoFadeEnabled}
          onclick={() => (autoFadeEnabled = !autoFadeEnabled)}
          title="Auto-Fade Strokes (3.5s): {autoFadeEnabled ? 'ON' : 'OFF'}"
        >
          ⏳
        </button>

        <button
          class="icon-btn danger"
          onclick={clearCanvas}
          title="Clear Screen (Esc)"
        >
          🗑️
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
              🖥️ {idx + 1}
            </button>
          {/each}
        </div>
        <div class="divider"></div>
      {/if}

      <!-- Ghost Mode (Click-Through) -->
      <button
        class="ghost-btn"
        class:active={isGhostMode}
        onclick={toggleGhostMode}
        title="Ghost Mode (X) — Pass clicks through to apps below"
      >
        {isGhostMode ? "👻 Ghost" : "👁️ Draw"}
      </button>

      <!-- Settings / Info Button -->
      <button
        class="icon-btn"
        onclick={() => (showHelpModal = !showHelpModal)}
        title="Keyboard Shortcuts & About"
      >
        ⚙️
      </button>

      <!-- Collapse / Dock Button -->
      <button
        class="collapse-btn"
        onclick={() => (isCollapsed = true)}
        title="Collapse into edge drawer tab (Space)"
      >
        ◀
      </button>
    </div>
  {/if}
</div>

<!-- Shortcuts & Info Modal -->
{#if showHelpModal}
  <div class="modal-backdrop" onclick={() => (showHelpModal = false)}>
    <div class="modal-card" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <h3>⚡ PixelTrace Quick Shortcuts</h3>
        <button class="modal-close" onclick={() => (showHelpModal = false)}>✕</button>
      </div>
      <div class="modal-body">
        <div class="shortcut-row">
          <span>Toggle Overlay (Global)</span>
          <kbd>⌘ + Shift + D</kbd>
        </div>
        <div class="shortcut-row">
          <span>Ghost / Click-Through Mode</span>
          <kbd>X</kbd>
        </div>
        <div class="shortcut-row">
          <span>Laser / Pen / Highlighter</span>
          <kbd>L / P / H</kbd>
        </div>
        <div class="shortcut-row">
          <span>Arrow / Rect / Circle / Line</span>
          <kbd>A / R / C</kbd>
        </div>
        <div class="shortcut-row">
          <span>Numbered Stamp / Text</span>
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
          <span>Clear All Markups</span>
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

  /* Inline Text Box */
  .inline-text-box {
    position: fixed;
    z-index: 100000;
    display: flex;
    flex-direction: column;
    gap: 4px;
    background: rgba(15, 15, 18, 0.85);
    backdrop-filter: blur(16px);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    padding: 6px 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
  }

  .inline-text-box input {
    background: transparent;
    border: none;
    outline: none;
    font-weight: bold;
    min-width: 220px;
  }

  .text-hint {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.5);
  }

  /* Floating Glassmorphic Container */
  .widget-container {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 999999;
    pointer-events: auto;
    transition: transform 0.08s ease-out;
    filter: drop-shadow(0 14px 32px rgba(0, 0, 0, 0.5));
  }

  .widget-container.collapsed {
    transition: transform 0.25s cubic-bezier(0.16, 1, 0.3, 1);
  }

  /* Sleek Collapsed Edge Pill */
  .edge-pill {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: rgba(18, 18, 22, 0.88);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 24px;
    color: #ffffff;
    cursor: pointer;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    transition: transform 0.15s ease, background-color 0.15s;
  }

  .edge-pill:hover {
    transform: scale(1.06);
    background: rgba(28, 28, 34, 0.95);
  }

  .pill-glow {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    box-shadow: 0 0 10px currentColor;
  }

  .pill-label {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.5px;
  }

  /* Glassmorphic Full Toolbar */
  .glass-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: rgba(20, 20, 24, 0.84);
    backdrop-filter: blur(26px) saturate(190%);
    -webkit-backdrop-filter: blur(26px) saturate(190%);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 16px;
    color: #ececed;
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
    margin: 0 2px;
  }

  .btn-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .tool-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 9px;
    color: #e0e0e0;
    font-size: 14px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }

  .tool-btn:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .tool-btn.active {
    background: rgba(255, 255, 255, 0.16);
    border-color: rgba(255, 255, 255, 0.28);
    color: #ffffff;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .laser-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    box-shadow: 0 0 8px currentColor;
  }

  .stamp-btn {
    padding: 4px 6px;
  }

  .stamp-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: #007aff;
    color: white;
    font-weight: bold;
    font-size: 11px;
    border-radius: 50%;
  }

  /* Stroke Size Presets */
  .size-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .size-chip {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
    padding: 0;
  }

  .size-chip:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  .size-chip.active {
    border-color: rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.14);
  }

  .size-dot {
    background: #ffffff;
    border-radius: 50%;
  }

  /* Color Palette Chips */
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
    transition: transform 0.15s, border-color 0.15s;
  }

  .color-chip:hover {
    transform: scale(1.15);
  }

  .color-chip.active {
    transform: scale(1.25);
    border-color: #ffffff;
    box-shadow: 0 0 10px rgba(255, 255, 255, 0.6);
  }

  .icon-btn {
    padding: 6px 7px;
    background: transparent;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    font-size: 13px;
    color: #e0e0e0;
    transition: background 0.15s;
  }

  .icon-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
  }

  .icon-btn.active {
    background: rgba(0, 199, 190, 0.25);
    border: 1px solid rgba(0, 199, 190, 0.4);
  }

  .icon-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .icon-btn.danger:hover {
    background: rgba(255, 59, 48, 0.25);
  }

  /* Multi-Monitor Buttons */
  .monitor-group {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  .mon-btn {
    padding: 3px 6px;
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

  /* Ghost Mode Toggle */
  .ghost-btn {
    padding: 5px 9px;
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
  }

  .ghost-btn.active {
    background: #af52de;
    border-color: #af52de;
    color: #ffffff;
    box-shadow: 0 0 12px rgba(175, 82, 222, 0.6);
  }

  .collapse-btn {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.5);
    cursor: pointer;
    padding: 4px 6px;
    font-size: 11px;
    border-radius: 6px;
    transition: color 0.15s;
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
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(8px);
    z-index: 1000000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-card {
    background: rgba(24, 24, 28, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 16px;
    width: 360px;
    padding: 16px 20px;
    box-shadow: 0 20px 48px rgba(0, 0, 0, 0.6);
    color: #ffffff;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 14px;
  }

  .modal-header h3 {
    margin: 0;
    font-size: 16px;
  }

  .modal-close {
    background: transparent;
    border: none;
    color: rgba(255, 255, 255, 0.6);
    cursor: pointer;
    font-size: 14px;
  }

  .modal-body {
    display: flex;
    flex-direction: column;
    gap: 9px;
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
    padding: 2px 7px;
    font-size: 11px;
    font-family: monospace;
    color: #ffffff;
  }
</style>
