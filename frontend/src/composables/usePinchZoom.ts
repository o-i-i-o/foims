/**
 * SVG 画布双指捏合缩放（应用级手势，拓扑画布接入）。
 *
 * 设计要点：
 * - 触控点按 pointerId 记入 Map，第二根手指落下即进入捏合：冻结「起始指距、
 *   起始视野、起始锚点」三个基准量，之后每帧视野只由三者推算，锚点不随
 *   getScreenCTM 漂移，与滚轮缩放（zoomAt）同一门径。
 * - 双指中点经 clientToCanvas 换算为画布锚点，捏合始终以两指间的内容为中心。
 * - window 级 move/up/cancel 监听常驻（仅捏合态消费）：第二指落点可能压在
 *   节点等子元素上，常驻监听不依赖冒泡路径，避免丢事件。
 * - 任一手指抬起立即退出捏合并清空状态：剩余手指不自发演变为拖拽等新交互，
 *   手势主导权交还调用方。
 */
import { onBeforeUnmount, onMounted, ref, type Ref } from "vue";
import { MAX_ZOOM_WIDTH, MIN_ZOOM_WIDTH } from "./useCanvasViewport";
import type { CanvasRect } from "../types/visualization";

export interface PinchZoomOptions {
  /** 当前视野（供进入捏合时快照） */
  viewBox: Ref<CanvasRect>;
  /** 视野写回（与 useCanvasViewport.setViewBox 同一入口） */
  setViewBox(rect: CanvasRect): void;
  /** 客户区坐标 → 画布世界坐标（捏合锚点换算） */
  clientToCanvas(clientX: number, clientY: number): { x: number; y: number } | null;
}

/** 起始指距小于该值视为双指重叠误触，忽略本次捏合 */
const MIN_PINCH_DIST = 8;

export function usePinchZoom(options: PinchZoomOptions) {
  /** 是否处于捏合态：调用方据此拦截自身的单指交互 */
  const isPinching = ref(false);

  /** 活动触控点：pointerId → 客户区坐标 */
  const pointers = new Map<number, { x: number; y: number }>();

  // 进入捏合时冻结的基准量
  let startDist = 0;
  let startViewBox: CanvasRect | null = null;
  let startAnchor: { x: number; y: number } | null = null;

  function exitPinch(): void {
    pointers.clear();
    isPinching.value = false;
    startDist = 0;
    startViewBox = null;
    startAnchor = null;
  }

  /** 按当前双指距离围绕冻结锚点推算视野 */
  function applyPinch(): void {
    if (!startViewBox || !startAnchor || pointers.size < 2) {
      return;
    }
    const points = [...pointers.values()];
    const a = points[0];
    const b = points[1];
    if (!a || !b) {
      return;
    }
    const dist = Math.hypot(a.x - b.x, a.y - b.y);
    if (dist < MIN_PINCH_DIST) {
      return;
    }
    // 指距放大 N 倍 = 视野宽度缩为 1/N（内容放大 N 倍）
    const nextWidth = (startViewBox.width * startDist) / dist;
    if (!Number.isFinite(nextWidth) || nextWidth < MIN_ZOOM_WIDTH || nextWidth > MAX_ZOOM_WIDTH) {
      return;
    }
    const ratio = nextWidth / startViewBox.width;
    options.setViewBox({
      x: startAnchor.x - (startAnchor.x - startViewBox.x) * ratio,
      y: startAnchor.y - (startAnchor.y - startViewBox.y) * ratio,
      width: nextWidth,
      height: startViewBox.height * ratio,
    });
  }

  function onWindowPointerMove(event: PointerEvent): void {
    if (!pointers.has(event.pointerId)) {
      return;
    }
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    applyPinch();
  }

  function onWindowPointerEnd(event: PointerEvent): void {
    if (pointers.has(event.pointerId)) {
      exitPinch();
    }
  }

  /**
   * 画布 pointerdown 入口：调用方须在自身交互处理之前最先调用。
   * 收集触控点，凑满两指且指距有效即进入捏合并冻结基准量。
   */
  function onPointerDown(event: PointerEvent): void {
    if (event.pointerType !== "touch") {
      return;
    }
    pointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointers.size < 2) {
      return;
    }
    const points = [...pointers.values()];
    const a = points[0];
    const b = points[1];
    if (!a || !b) {
      return;
    }
    const dist = Math.hypot(a.x - b.x, a.y - b.y);
    if (dist < MIN_PINCH_DIST) {
      return;
    }
    const anchor = options.clientToCanvas((a.x + b.x) / 2, (a.y + b.y) / 2);
    if (!anchor) {
      return;
    }
    isPinching.value = true;
    startDist = dist;
    startViewBox = { ...options.viewBox.value };
    startAnchor = anchor;
  }

  onMounted(() => {
    window.addEventListener("pointermove", onWindowPointerMove);
    window.addEventListener("pointerup", onWindowPointerEnd);
    window.addEventListener("pointercancel", onWindowPointerEnd);
  });

  onBeforeUnmount(() => {
    window.removeEventListener("pointermove", onWindowPointerMove);
    window.removeEventListener("pointerup", onWindowPointerEnd);
    window.removeEventListener("pointercancel", onWindowPointerEnd);
  });

  return { isPinching, onPointerDown };
}
