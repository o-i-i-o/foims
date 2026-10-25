/**
 * SVG 画布视口（工位 / 机柜 / 拓扑三画布共享）。
 *
 * 旧实现把 viewBox、覆盖矩形、标尺、坐标换算、缩放散落在 SVGCanvasBase 与
 * 各 Core 子类中，靠手写 DOM 属性维护；Vue 化后这些量收敛为响应式状态：
 * `viewBox` 是唯一输入，覆盖矩形与主网格标尺由 computed 派生，SVG 属性交给
 * 模板输出，不再有「先渲染再纠正」的中间态。
 */
import { computed, onBeforeUnmount, onMounted, ref, type ComputedRef, type Ref } from "vue";
import {
  GRID_SIZE,
  buildGridRuler,
  computeCoverRect,
  type GridRulerData,
} from "../utils/visualization/geometry";
import type { CanvasRect } from "../types/visualization";

/** 缩放比例上下限（与旧 TopologyCore.setZoom 口径一致） */
const MIN_ZOOM_SCALE = 0.05;
const MAX_ZOOM_SCALE = 8;
/** 滚轮缩放的视野宽度闸门：超出即忽略，避免一次滚轮把画布缩成不可用状态 */
const MIN_ZOOM_WIDTH = 300;
const MAX_ZOOM_WIDTH = 30000;

/** 容器可视尺寸（隐藏页签下为 0，覆盖矩形此时退化为 viewBox 本身） */
export interface CanvasSize {
  width: number;
  height: number;
}

export interface CanvasViewportOptions {
  /** 容器元素（模板 ref 由调用方持有，用于尺寸测量） */
  containerRef: Ref<HTMLDivElement | null>;
  /** SVG 根元素（鼠标坐标 → 画布坐标换算） */
  svgRef: Ref<SVGSVGElement | null>;
  /** 初始视野；`preserveAspectRatio` 决定内容在容器中的锚定方向 */
  initialViewBox: CanvasRect;
  /** 标尺字号随视野宽度缩放的除数（工位/机柜 100，拓扑 150） */
  rulerFontDivisor?: number;
  /** 与 `<svg preserveAspectRatio>` 同值的锚定声明，覆盖矩形按它补边 */
  preserveAspectRatio: string;
}

export interface CanvasViewport {
  viewBox: Ref<CanvasRect>;
  containerSize: Ref<CanvasSize>;
  /** 与可视区等大的世界坐标矩形（viewBox ∪ letterbox 区），网格铺满用 */
  coverRect: ComputedRef<CanvasRect>;
  /** 主网格线与坐标标注数据，异常视野下为 null */
  ruler: ComputedRef<GridRulerData | null>;
  /** 当前显示比例（容器宽 / 视野宽），工具栏据此双向绑定 */
  zoomScale: ComputedRef<number>;
  gridSize: number;
  setViewBox(rect: CanvasRect): void;
  /** 鼠标视口坐标 → 画布世界坐标（拖拽/缩放锚点换算） */
  toCanvasPoint(event: MouseEvent): { x: number; y: number } | null;
  /** 重新测量容器尺寸（ResizeObserver 之外的兜底入口） */
  syncContainerSize(): void;
  /** 视野至少覆盖容器尺寸（只增不减，工位画布语义） */
  fitToContainer(): void;
  setZoom(scale: number): void;
  zoomBy(factor: number): void;
  /** 以画布坐标点为锚缩放（滚轮缩放） */
  zoomAt(anchor: { x: number; y: number }, factor: number): void;
}

export function useCanvasViewport(options: CanvasViewportOptions): CanvasViewport {
  const { containerRef, svgRef } = options;
  const viewBox = ref<CanvasRect>({ ...options.initialViewBox });
  const containerSize = ref<CanvasSize>({ width: 0, height: 0 });
  const rulerFontDivisor = options.rulerFontDivisor ?? 100;

  let observer: ResizeObserver | null = null;

  const coverRect = computed(() =>
    computeCoverRect(viewBox.value, containerSize.value, options.preserveAspectRatio),
  );
  const ruler = computed(() =>
    buildGridRuler(coverRect.value, { gridSize: GRID_SIZE, fontDivisor: rulerFontDivisor }),
  );
  const zoomScale = computed(() => {
    const width = containerSize.value.width;
    return width && viewBox.value.width ? width / viewBox.value.width : 1;
  });

  function setViewBox(rect: CanvasRect): void {
    viewBox.value = { ...rect };
  }

  function syncContainerSize(): void {
    const element = containerRef.value;
    if (!element) {
      return;
    }
    const width = element.clientWidth;
    const height = element.clientHeight;
    if (width === containerSize.value.width && height === containerSize.value.height) {
      return;
    }
    containerSize.value = { width, height };
  }

  function toCanvasPoint(event: MouseEvent): { x: number; y: number } | null {
    const svg = svgRef.value;
    if (!svg) {
      return null;
    }
    const matrix = svg.getScreenCTM();
    if (!matrix) {
      return null;
    }
    const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix.inverse());
    if (!Number.isFinite(point.x) || !Number.isFinite(point.y)) {
      return null;
    }
    return { x: point.x, y: point.y };
  }

  function fitToContainer(): void {
    const { width, height } = containerSize.value;
    if (!width || !height) {
      return;
    }
    const current = viewBox.value;
    if (current.width >= width && current.height >= height) {
      return;
    }
    setViewBox({
      x: current.x,
      y: current.y,
      width: Math.max(current.width, width),
      height: Math.max(current.height, height),
    });
  }

  function setZoom(scale: number): void {
    const container = containerSize.value;
    const current = viewBox.value;
    if (!container.width || !current.width || !Number.isFinite(scale) || scale <= 0) {
      return;
    }
    const target = Math.min(MAX_ZOOM_SCALE, Math.max(MIN_ZOOM_SCALE, scale));
    const nextWidth = container.width / target;
    if (nextWidth < MIN_ZOOM_WIDTH || nextWidth > MAX_ZOOM_WIDTH) {
      return;
    }
    // 保持视野自身宽高比（与旧 TopologyCore.setZoom 一致）：以视野中心为锚
    const nextHeight = nextWidth * (current.height / current.width);
    const centerX = current.x + current.width / 2;
    const centerY = current.y + current.height / 2;
    setViewBox({
      x: centerX - nextWidth / 2,
      y: centerY - nextHeight / 2,
      width: nextWidth,
      height: nextHeight,
    });
  }

  function zoomBy(factor: number): void {
    setZoom(zoomScale.value * factor);
  }

  function zoomAt(anchor: { x: number; y: number }, factor: number): void {
    const current = viewBox.value;
    const nextWidth = current.width * factor;
    if (!Number.isFinite(nextWidth) || nextWidth < MIN_ZOOM_WIDTH || nextWidth > MAX_ZOOM_WIDTH) {
      return;
    }
    const ratio = nextWidth / current.width;
    setViewBox({
      x: anchor.x - (anchor.x - current.x) * ratio,
      y: anchor.y - (anchor.y - current.y) * ratio,
      width: nextWidth,
      height: current.height * factor,
    });
  }

  onMounted(() => {
    syncContainerSize();
    const element = containerRef.value;
    if (element && typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => syncContainerSize());
      observer.observe(element);
    }
    // 兜底：部分嵌入式 WebView 不产渲染帧、ResizeObserver 回调不派发，
    // 窗口缩放时按容器尺寸比较补一次同步（输入事件不依赖渲染帧）
    window.addEventListener("resize", syncContainerSize);
  });

  onBeforeUnmount(() => {
    observer?.disconnect();
    observer = null;
    window.removeEventListener("resize", syncContainerSize);
  });

  return {
    viewBox,
    containerSize,
    coverRect,
    ruler,
    zoomScale,
    gridSize: GRID_SIZE,
    setViewBox,
    toCanvasPoint,
    syncContainerSize,
    fitToContainer,
    setZoom,
    zoomBy,
    zoomAt,
  };
}
