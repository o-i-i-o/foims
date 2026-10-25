/**
 * 可视化几何算法的可执行自检（P0 安全网）。
 *
 * 运行：在 frontend/ 下执行 `npm run selfcheck:viz`（内含本脚本与视图模型自检）。
 * 该命令带 ts-resolve-hook，补上无扩展名 import 的 .ts 解析；直接 `node` 运行
 * 会因 `../src/utils/visualization/geometry` 无法解析而失败。
 * （Node ≥ 22.6 原生擦除类型，不引入测试框架依赖；脚本在 tsconfig include
 * 之外，故不参与 vue-tsc 检查）。
 *
 * 覆盖：视图覆盖矩形、网格标尺有界性、对齐线、工位/机柜排布、U 位刻度、
 * 布局投影、拓扑分组推挤与自动布局、锚点错开、正交走线、IP 归并。
 */
import {
  applyGroupedLayout,
  arrangeWorkstationRects,
  buildGridRuler,
  buildLayoutItems,
  buildUMarks,
  cabinetGroupKey,
  CABINET_EDGE_PADDING,
  CABINET_GAP,
  CABINET_HEADER_HEIGHT,
  CABINET_WIDTH,
  chunk,
  computeAlignmentSegments,
  computeCabinetLayout,
  computeCabinetPositionRect,
  computeCoverRect,
  computeGroupBox,
  computeOrthogonalPath,
  computeTempConnectionPath,
  fitRects,
  isValidRect,
  mapIpsByPosition,
  mapIpsByWorkstation,
  moveGroupTo,
  nodeEdgeAnchor,
  pickAnchor,
  separateGroupBoxes,
  snapToGrid,
  workstationCanvasRect,
  workstationColumns,
} from "../src/utils/visualization/geometry.ts";

let failures = 0;

function check(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`  ok  ${name}`);
  } catch (error) {
    failures += 1;
    console.error(`FAIL  ${name}`);
    console.error(`      ${error instanceof Error ? error.message : String(error)}`);
  }
}

function equal<T>(actual: T, expected: T, what = "值"): void {
  if (actual !== expected) {
    throw new Error(`${what}不符：期望 ${String(expected)}，实际 ${String(actual)}`);
  }
}

function close(actual: number, expected: number, what = "值", tolerance = 1e-6): void {
  if (Math.abs(actual - expected) > tolerance) {
    throw new Error(`${what}不符：期望 ${expected}，实际 ${actual}`);
  }
}

function truthy(value: unknown, what = "条件"): void {
  if (!value) {
    throw new Error(`${what}应为真`);
  }
}

function falsy(value: unknown, what = "条件"): void {
  if (value) {
    throw new Error(`${what}应为假`);
  }
}

// ==================== 基础校验 ====================

check("isValidRect 拒绝 NaN / 负值 / 零宽高", () => {
  falsy(isValidRect({ x: Number.NaN, y: 0, width: 10, height: 10 }));
  falsy(isValidRect({ x: -1, y: 0, width: 10, height: 10 }));
  falsy(isValidRect({ x: 0, y: 0, width: 0, height: 10 }));
  falsy(isValidRect(null));
  truthy(isValidRect({ x: 0, y: 0, width: 1, height: 1 }));
});

check("snapToGrid 四舍五入到最近网格点", () => {
  equal(snapToGrid(31), 40);
  equal(snapToGrid(29), 20);
  equal(snapToGrid(-11), -20);
});

// ==================== 覆盖矩形与网格 ====================

check("computeCoverRect 正方形 viewBox 在 16:9 容器中横向扩展", () => {
  const cover = computeCoverRect(
    { x: 0, y: 0, width: 1000, height: 1000 },
    { width: 1600, height: 900 },
    "xMinYMin meet",
  );
  equal(cover.x, 0, "x 锚定左缘");
  equal(cover.y, 0, "y 锚定上缘");
  close(cover.width, 1600 * (1000 / 900));
  equal(cover.height, 1000);
});

check("computeCoverRect 容器隐藏（0 尺寸）退化为 viewBox", () => {
  const cover = computeCoverRect(
    { x: 10, y: 20, width: 100, height: 200 },
    { width: 0, height: 0 },
    "xMidYMid meet",
  );
  equal(cover.x, 10);
  equal(cover.y, 20);
  equal(cover.width, 100);
  equal(cover.height, 200);
});

check("computeCoverRect 中锚点时按比例双向补边", () => {
  // 容器宽高比（2:1）大于 viewBox（1:1）：缩放由高度决定，宽度补 1000
  const cover = computeCoverRect(
    { x: 0, y: 0, width: 1000, height: 1000 },
    { width: 1000, height: 500 },
    "xMidYMid meet",
  );
  equal(cover.x, -500);
  equal(cover.width, 2000);
  equal(cover.y, 0);
  equal(cover.height, 1000);
});

check("buildGridRuler 正常视野生成主线与标注", () => {
  const ruler = buildGridRuler({ x: 0, y: 0, width: 1000, height: 800 }, { gridSize: 20 });
  truthy(ruler);
  // 1000/100 = 10 条竖线，800/100 = 8 条横线
  equal(ruler?.lines.length, 18);
  equal(ruler?.labels.length, 18);
  equal(ruler?.labels[0]?.text, "0");
});

check("buildGridRuler 非有限视野返回 null（防百万节点冻结）", () => {
  const ruler = buildGridRuler({
    x: 0,
    y: 0,
    width: Number.POSITIVE_INFINITY,
    height: 800,
  });
  equal(ruler, null);
});

check("buildGridRuler 超大视野条数封顶", () => {
  const ruler = buildGridRuler({ x: 0, y: 0, width: 1e8, height: 1e8 }, { maxLines: 500 });
  equal(ruler?.lines.length, 1000);
});

// ==================== 对齐线 ====================

check("computeAlignmentSegments 中心与左缘对齐命中各一条", () => {
  const segments = computeAlignmentSegments(
    { x: 100, y: 100, width: 160, height: 160 },
    [{ x: 104, y: 400, width: 160, height: 160 }],
    10,
  );
  // 左缘差 4、中心差 4，两条都命中；右缘差 4 不参与判定
  equal(segments.length, 2);
});

check("computeAlignmentSegments 超出阈值不产生参考线", () => {
  const segments = computeAlignmentSegments(
    { x: 0, y: 0, width: 100, height: 100 },
    [{ x: 50, y: 50, width: 100, height: 100 }],
    10,
  );
  equal(segments.length, 0);
});

// ==================== 工位布局 ====================

check("workstationColumns 按数量分档", () => {
  equal(workstationColumns(3), 3);
  equal(workstationColumns(5), 3);
  equal(workstationColumns(6), 4);
  equal(workstationColumns(12), 4);
  equal(workstationColumns(13), 6);
  equal(workstationColumns(24), 6);
  equal(workstationColumns(25), 5);
  equal(workstationColumns(100), 8);
});

check("arrangeWorkstationRects 按行列连续排布且不重叠", () => {
  const rects = arrangeWorkstationRects(7);
  equal(rects.length, 7);
  equal(rects[0]?.x, 150);
  equal(rects[0]?.y, 100);
  equal(rects[1]?.x, 330);
  // 7 个工位 → 4 列：第 5 个（index 4）换行
  equal(rects[4]?.y, 280);
  const unique = new Set(rects.map((rect) => `${rect.x},${rect.y}`));
  equal(unique.size, 7);
});

check("workstationCanvasRect 覆盖内容且不小于最小视野", () => {
  const view = workstationCanvasRect(
    30,
    { x: 50, y: 100, width: 40, height: 80 },
    { width: 500, height: 400 },
  );
  truthy(view.width >= 1000);
  truthy(view.height >= 800);
  // 6 列：150 + 6*180 + 50 = 1280
  equal(view.width, 1280);
});

// ==================== 机柜布局 ====================

const cabinetInputs = [
  { id: "a", capacity: 42, savedX: null, savedWidth: null },
  { id: "b", capacity: 42, savedX: null, savedWidth: null },
  { id: "c", capacity: 42, savedX: null, savedWidth: null },
];

check("computeCabinetLayout 首柜贴左缘、柜底贴底、等距排列", () => {
  const layout = computeCabinetLayout(cabinetInputs, 900);
  equal(layout.entries.length, 3);
  equal(layout.entries[0]?.rect.x, CABINET_EDGE_PADDING);
  // 900 - 16*2 - 40 = 828；828/42 = 19.71 → 19
  equal(layout.uHeight, 19);
  const height = 42 * 19 + CABINET_HEADER_HEIGHT;
  equal(layout.entries[0]?.rect.height, height);
  equal(layout.entries[0]?.rect.y, 900 - CABINET_EDGE_PADDING - height);
  equal(
    layout.entries[1]?.rect.x,
    CABINET_EDGE_PADDING + CABINET_WIDTH + CABINET_GAP,
  );
  equal(
    layout.totalWidth,
    CABINET_EDGE_PADDING + 3 * (CABINET_WIDTH + CABINET_GAP) - CABINET_GAP + CABINET_EDGE_PADDING,
  );
});

check("computeCabinetLayout 已保存 x 仅保留相对排列并整体左对齐", () => {
  const layout = computeCabinetLayout(
    [
      { id: "a", capacity: 42, savedX: 500, savedWidth: 200 },
      { id: "b", capacity: 42, savedX: 800, savedWidth: null },
    ],
    900,
  );
  equal(layout.entries[0]?.rect.x, CABINET_EDGE_PADDING);
  equal(layout.entries[1]?.rect.x, CABINET_EDGE_PADDING + 300);
  equal(layout.entries[0]?.rect.width, 200);
  equal(layout.entries[1]?.rect.width, CABINET_WIDTH);
});

check("computeCabinetLayout 容器过矮时 U 高钳制为 1", () => {
  const layout = computeCabinetLayout([{ id: "a", capacity: 42, savedX: null, savedWidth: null }], 60);
  equal(layout.uHeight, 1);
});

check("computeCabinetPositionRect 按 U 位从柜底向上排布", () => {
  const cabinet = { x: 16, y: 100, width: 150, height: 42 * 19 + 40 };
  const rect = computeCabinetPositionRect(cabinet, 42, 19, 40, 42);
  equal(rect.x, 36);
  equal(rect.width, 110);
  equal(rect.height, 3 * 19);
  equal(rect.y, 100 + 40 + 0);
  const top = computeCabinetPositionRect(cabinet, 42, 19, 1, 2);
  equal(top.y, 100 + 40 + 40 * 19);
});

check("buildUMarks 线全量、编号含首末 U", () => {
  const marks = buildUMarks(42, 19, 150);
  equal(marks.lines.length, 42);
  const texts = new Set(marks.labels.map((label) => label.text));
  truthy(texts.has("1"));
  truthy(texts.has("42"));
  truthy(texts.has("5"));
  falsy(texts.has("2"));
});

// ==================== 布局投影 ====================

check("buildLayoutItems 丢弃非有限值与零宽高、负坐标钳制为 0", () => {
  const items = buildLayoutItems([
    { id: "ok", elementType: "workstation", rect: { x: 10, y: 20, width: 160, height: 160 } },
    {
      id: "negative",
      elementType: "workstation",
      rect: { x: -30, y: -10, width: 160, height: 160 },
    },
    { id: "zero", elementType: "workstation", rect: { x: 0, y: 0, width: 0, height: 160 } },
    {
      id: "nan",
      elementType: "workstation",
      rect: { x: Number.NaN, y: 0, width: 160, height: 160 },
    },
  ]);
  equal(items.length, 2);
  equal(items[0]?.position.x, 10);
  equal(items[1]?.position.x, 0);
  equal(items[1]?.position.y, 0);
  equal(items[0]?.position.rotation, 0);
});

// ==================== 拓扑分组 ====================

check("cabinetGroupKey 优先 id、其次名称、否则 null", () => {
  equal(cabinetGroupKey({ cabinet_id: "c1", cabinet_name: "A" }), "cab:c1");
  equal(cabinetGroupKey({ cabinet_id: "c1", cabinet_name: null }), "cab:c1");
  equal(cabinetGroupKey({ cabinet_id: null, cabinet_name: "A" }), "cab:name:A");
  equal(cabinetGroupKey({ cabinet_id: null, cabinet_name: null }), null);
});

check("computeGroupBox 含留白与标题区，空分组返回 null", () => {
  const box = computeGroupBox(
    [{ rect: { x: 100, y: 200, width: 50, height: 60 } }],
    10,
    20,
  );
  equal(box?.minX, 90);
  equal(box?.minY, 170);
  equal(box?.maxX, 160);
  equal(box?.maxY, 270);
  equal(computeGroupBox([], 10, 20), null);
});

function makeNodes(count: number, x: number, y: number) {
  return Array.from({ length: count }, (_unused, index) => ({
    rect: { x: x + index * 200, y, width: 200, height: 100 },
  }));
}

check("separateGroupBoxes 完全重叠的两组被推开且不再相交", () => {
  const a = { key: "a", nodes: makeNodes(1, 0, 0), padding: 10, header: 20 };
  const b = { key: "b", nodes: makeNodes(1, 0, 0), padding: 10, header: 20 };
  const moved = separateGroupBoxes([a, b], 80, null);
  truthy(moved);
  const boxA = computeGroupBox(a.nodes, 10, 20);
  const boxB = computeGroupBox(b.nodes, 10, 20);
  truthy(boxA && boxB);
  const overlapX = Math.min(boxA.maxX, boxB.maxX) - Math.max(boxA.minX, boxB.minX);
  const overlapY = Math.min(boxA.maxY, boxB.maxY) - Math.max(boxA.minY, boxB.minY);
  truthy(overlapX <= 0 || overlapY <= 0, "两组包围盒应已分离");
  equal(overlapX <= 0 ? boxB.minX - boxA.maxX : boxB.minY - boxA.maxY, 80);
});

check("separateGroupBoxes 固定组保持不动，位移全部由对方承担", () => {
  const fixed = { key: "fixed", nodes: makeNodes(1, 0, 0), padding: 10, header: 20 };
  const other = { key: "other", nodes: makeNodes(1, 0, 0), padding: 10, header: 20 };
  separateGroupBoxes([fixed, other], 40, "fixed");
  equal(fixed.nodes[0]?.rect.x, 0);
  equal(fixed.nodes[0]?.rect.y, 0);
  truthy(other.nodes[0] !== undefined);
});

check("separateGroupBoxes 不相交时不动", () => {
  const a = { key: "a", nodes: makeNodes(1, 0, 0), padding: 10, header: 20 };
  const b = { key: "b", nodes: makeNodes(1, 1000, 1000), padding: 10, header: 20 };
  falsy(separateGroupBoxes([a, b], 40, null));
  equal(a.nodes[0]?.rect.x, 0);
});

check("applyGroupedLayout 房间块网格铺开且块间不重叠", () => {
  const rooms = [
    {
      key: "room:1",
      label: "R1",
      orgLabel: "",
      cabinets: [{ key: "cab:1", label: "C1", nodes: makeNodes(4, 0, 0) }],
      loose: makeNodes(2, 0, 0),
      nodes: [] as ReturnType<typeof makeNodes>,
    },
    {
      key: "room:2",
      label: "R2",
      orgLabel: "",
      cabinets: [],
      loose: makeNodes(3, 0, 0),
      nodes: [] as ReturnType<typeof makeNodes>,
    },
  ];
  rooms[0]?.nodes.push(...(rooms[0]?.cabinets[0]?.nodes ?? []), ...(rooms[0]?.loose ?? []));
  rooms[1]?.nodes.push(...(rooms[1]?.loose ?? []));

  applyGroupedLayout(rooms);

  const boxes = rooms.map((room) => computeGroupBox(room.nodes, 40, 52));
  truthy(boxes[0] && boxes[1]);
  equal(boxes[0]?.minX, 100);
  equal(boxes[0]?.minY, 100);
  truthy((boxes[1]?.minX ?? 0) >= (boxes[0]?.maxX ?? 0), "第二个房间应在右侧");
  // 机柜内三列排布：第 4 个节点回到第 2 行第 1 列（相对关系与房间块平移无关）
  const cabinetNodes = rooms[0]?.cabinets[0]?.nodes ?? [];
  const first = cabinetNodes[0]?.rect;
  const fourth = cabinetNodes[3]?.rect;
  truthy(first && fourth);
  equal(fourth.x - first.x, 0, "第 4 个节点回到第 1 列");
  equal(fourth.y - first.y, 140, "第 4 个节点落在第 2 行");
});

check("moveGroupTo 平移整组到目标原点", () => {
  const nodes = makeNodes(2, 0, 0);
  truthy(moveGroupTo(nodes, 10, 20, 300, 400));
  const box = computeGroupBox(nodes, 10, 20);
  equal(box?.minX, 300);
  equal(box?.minY, 400);
  falsy(moveGroupTo(nodes, 10, 20, 300, 400));
});

// ==================== 连线 ====================

check("pickAnchor 优先主轴并夹在边缘内", () => {
  const right = pickAnchor(
    { x: 0, y: 0, width: 200, height: 100 },
    { x: 600, y: 20, width: 200, height: 100 },
    0,
  );
  equal(right.dir, "right");
  equal(right.x, 200);
  equal(right.y, 50);

  const bottom = pickAnchor(
    { x: 0, y: 0, width: 200, height: 100 },
    { x: 10, y: 600, width: 200, height: 100 },
    0,
  );
  equal(bottom.dir, "bottom");
  equal(bottom.y, 100);
});

check("pickAnchor 同侧多连线按交替方向错开且不越界", () => {
  const source = { x: 0, y: 0, width: 200, height: 100 };
  const target = { x: 600, y: 20, width: 200, height: 100 };
  const centers = [0, 1, 2, 3, 4].map((slot) => pickAnchor(source, target, slot).y);
  equal(centers[0], 50, "第一条居中");
  equal(centers[1], 68, "第二条下移 18");
  equal(centers[2], 32, "第三条上移 18");
  for (const y of centers) {
    truthy(y >= 16 && y <= 84, "锚点不得越出边缘 16px 缓冲");
  }
});

check("computeOrthogonalPath 四段折线且水平起手", () => {
  const path = computeOrthogonalPath(
    { x: 0, y: 50, dir: "right" },
    { x: 400, y: 200, dir: "left" },
    0,
  );
  equal(path.split(" L ").length, 4);
  truthy(path.startsWith("M 0 50"));
  truthy(path.endsWith("L 400 200"));
});

check("computeTempConnectionPath 生成贝塞尔曲线", () => {
  const path = computeTempConnectionPath({ x: 0, y: 0 }, { x: 100, y: 0 });
  truthy(path.startsWith("M 0 0 C "));
  truthy(path.includes("100 0"));
});

check("nodeEdgeAnchor 四个方向", () => {
  const rect = { x: 10, y: 20, width: 100, height: 60 };
  equal(nodeEdgeAnchor(rect, "top").x, 60);
  equal(nodeEdgeAnchor(rect, "right").x, 110);
  equal(nodeEdgeAnchor(rect, "bottom").y, 80);
  equal(nodeEdgeAnchor(rect, "left").y, 50);
});

check("fitRects 包裹全部内容并留白", () => {
  const view = fitRects(
    [
      { x: 100, y: 100, width: 100, height: 100 },
      { x: 400, y: 300, width: 100, height: 100 },
    ],
    50,
    { x: 0, y: 0, width: 1, height: 1 },
  );
  equal(view.x, 50);
  equal(view.y, 50);
  equal(view.width, 500);
  equal(view.height, 400);
  equal(fitRects([], 50, { x: 0, y: 0, width: 3000, height: 2000 }).width, 3000);
});

// ==================== IP 归并 ====================

check("mapIpsByWorkstation 同工位 active 优先", () => {
  const map = mapIpsByWorkstation([
    { workstation_id: "w1", status: "inactive", ip_address: "1.1.1.1" },
    { workstation_id: "w1", status: "active", ip_address: "2.2.2.2" },
    { workstation_id: null, status: "active", ip_address: "3.3.3.3" },
  ]);
  equal(map.size, 1);
  equal(map.get("w1")?.ip_address, "2.2.2.2");
});

check("mapIpsByPosition 已 active 时不被后续 inactive 覆盖", () => {
  const map = mapIpsByPosition([
    { position_id: "p1", status: "active", ip_address: "1.1.1.1" },
    { position_id: "p1", status: "inactive", ip_address: "2.2.2.2" },
  ]);
  equal(map.get("p1")?.ip_address, "1.1.1.1");
});

check("chunk 按大小切块并保留余数", () => {
  equal(chunk([1, 2, 3, 4, 5], 2).length, 3);
  equal(chunk([1, 2, 3, 4, 5], 2)[2]?.length, 1);
  equal(chunk([], 2).length, 0);
  equal(chunk([1], 0).length, 0);
});

if (failures > 0) {
  throw new Error(`${failures} 项自检失败`);
}
console.log("\n几何算法自检全部通过");
