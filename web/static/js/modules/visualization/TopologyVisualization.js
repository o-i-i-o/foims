import { TopologyCore } from "./TopologyCore.js";
import { TopologyRenderer, cabinetGroupKey } from "./TopologyRenderer.js";
import { TopologyDataManager } from "./TopologyDataManager.js";
import { SVG_NS } from "./SVGCore.js";
import { showConfirm } from "../../utils/confirm.js";
import { showToast } from "../../utils/ui.js";
import { t } from "../../utils/i18n.js";
import { loadModal, openModal, closeModal } from "../../utils/modalLoader.js";

// 推挤位移分配：自身固定不动；对方固定时承担全部推量；双方可动时对半分摊
function separateShift(selfFixed, otherFixed, push) {
  if (selfFixed) {
    return 0;
  }
  return otherFixed ? push : push / 2;
}

// 房间容器留白：左右下内边距与顶部标题区高度
const ROOM_PADDING = 40;
const ROOM_HEADER = 52;
// 机柜容器留白
const CABINET_PADDING = 24;
const CABINET_HEADER = 36;
// 房间容器之间的最小间距（推开重叠分组时使用）
const ROOM_GAP = 80;
const CABINET_GAP = 40;

export class TopologyVisualization {
  constructor(containerId) {
    this.core = new TopologyCore(containerId, {
      onNodeClick: (deviceId) => this.openDeviceDetail(deviceId),
      onNodeDrag: (deviceId) => this.renderer.updateConnectionPaths(deviceId),
      onNodeDragEnd: (deviceId) => this._handleNodeDragEnd(deviceId),
      onContainerClick: (kind, key, box) => this.openContainerCoordinateModal(kind, key, box),
      onContainerDragEnd: (fixedKey) => this._handleGroupDragEnd(fixedKey),
      onConnectionComplete: (sDev, sPort, tDev, tPort) =>
        this._createConnection(sDev, sPort, tDev, tPort),
      onConnectionClick: (connId) => this._handleConnectionClick(connId),
      onCanvasClick: () => this._deselectConnection()
    });
    this.renderer = new TopologyRenderer(this.core);
    this.dataManager = new TopologyDataManager();
    this.nodes = [];
    this.connections = [];
    this.connectionsMap = new Map();
    this.selectedConnectionId = null;
    // 组织筛选：null 显示全部，否则仅渲染集合内组织的设备（一个组织一套布局）
    this.orgFilterSet = null;
    // 拖拽自动保存计时器（页面直接编辑坐标后静默持久化）
    this._autoSaveTimer = null;
  }

  async loadTopology() {
    this.renderer.clearAll();
    const [nodes, connections] = await Promise.all([
      this.dataManager.fetchTopologyNodes(),
      this.dataManager.fetchTopologyConnections()
    ]);

    this.nodes = nodes;
    this.connections = connections;
    this.connectionsMap.clear();
    connections.forEach((c) => this.connectionsMap.set(c.id, c));
    this.renderer.setConnectionsMap(this.connectionsMap);

    // 修正历史布局中房间包围盒相互覆盖的情况（整体平移分组，不动单个设备）
    this._separateOverlappingGroups();

    this._renderCurrentView();
  }

  /// 设置组织筛选（传入组织 id 集合或 null），并按筛选重新渲染
  setOrgFilter(orgIds) {
    this.orgFilterSet = orgIds;
    this._renderCurrentView();
  }

  /// 当前筛选下可见的节点
  visibleNodes() {
    if (!this.orgFilterSet) {
      return this.nodes;
    }
    return this.nodes.filter((n) => n.org_id && this.orgFilterSet.has(n.org_id));
  }

  /// 当前筛选下可见的连线（两端设备均可见才显示）
  visibleConnections() {
    const ids = new Set(this.visibleNodes().map((n) => n.device_id));
    return this.connections.filter(
      (c) => ids.has(c.source_device_id) && ids.has(c.target_device_id)
    );
  }

  _renderCurrentView() {
    this.renderer.clearAll();
    this.visibleNodes().forEach((node) => this.renderer.drawDeviceNode(node));
    this.visibleConnections().forEach((conn) => this.renderer.drawConnection(conn));
    this._renderContainers();

    this._fitView();
    this.core._updateZoomIndicator();
  }

  async deleteDevice(deviceId) {
    const confirmed = await showConfirm(t("viz.confirm_remove_device"));
    if (!confirmed) {
      return;
    }

    const success = await this.dataManager.deleteTopologyNode(deviceId);
    if (success) {
      this.nodes = this.nodes.filter((n) => n.device_id !== deviceId);
      this.connections = this.connections.filter(
        (c) => c.source_device_id !== deviceId && c.target_device_id !== deviceId
      );
      this.connectionsMap.clear();
      this.connections.forEach((c) => this.connectionsMap.set(c.id, c));
      await this.loadTopology();
    }
  }

  async _createConnection(sourceDeviceId, sourcePortId, targetDeviceId, targetPortId) {
    const result = await this.dataManager.createConnection({
      connection_type: "physical",
      source_device_id: sourceDeviceId,
      target_device_id: targetDeviceId,
      source_device_port_id: sourcePortId || null,
      target_device_port_id: targetPortId || null
    });

    if (result) {
      await this.loadTopology();
    }
  }

  /// 拖拽结束后：以用户摆放位置为准，推开其他被覆盖的房间分组，并自动保存坐标
  _handleNodeDragEnd(deviceId) {
    const node = this.nodes.find((n) => n.device_id === deviceId);
    const fixedKey = node?.room_id ? `room:${node.room_id}` : "room:none";

    this._syncPositionsFromDom();
    const moved = this._separateOverlappingGroups(fixedKey);
    if (moved) {
      this._renderCurrentView();
    } else {
      this._renderContainers();
    }
    this._scheduleAutoSave();
  }

  /// 容器（房间/机柜分组框）拖拽结束后：同步位置、推挤重叠分组并自动保存
  _handleGroupDragEnd(fixedKey) {
    this._syncPositionsFromDom();
    const moved = this._separateOverlappingGroups(fixedKey);
    if (moved) {
      this._renderCurrentView();
    } else {
      this._renderContainers();
    }
    this._scheduleAutoSave();
  }

  /// 拖拽编辑防抖保存：500ms 内连续拖动只落一次
  _scheduleAutoSave() {
    clearTimeout(this._autoSaveTimer);
    this._autoSaveTimer = setTimeout(() => {
      this._savePositions({ silent: true });
    }, 500);
  }

  /// 同步 DOM 坐标到节点对象并提交保存（silent 时成功不提示）
  async _savePositions({ silent = false } = {}) {
    this._syncPositionsFromDom();
    const nodes = this.visibleNodes().map((n) => ({
      device_id: n.device_id,
      // 坐标 0 合法：仅 null/undefined 时回退默认值
      x: n.x ?? 100,
      y: n.y ?? 100,
      width: n.width ?? 200,
      height: n.height ?? 100
    }));
    if (nodes.length === 0) {
      if (!silent) {
        showToast(t("viz.no_nodes_to_save"), "warning");
      }
      return;
    }
    await this.dataManager.saveTopologyNodes(nodes, { silent });
  }

  async _handleConnectionClick(connectionId) {
    this._deselectConnection();
    this.selectedConnectionId = connectionId;

    const g = this.core.connectionsGroup.querySelector(
      `[data-connection-id="${CSS.escape(connectionId)}"]`
    );
    if (!g) {
      return;
    }

    const path = g.querySelector(".topology-connection");
    if (path) {
      path.classList.add("selected");
    }

    const conn = this.connectionsMap.get(connectionId);
    if (!conn) {
      return;
    }

    await this._openConnectionDetail(conn);
  }

  /// 连线详情弹窗：展示两端设备与端口（逻辑连线含成员端口，派生连线含途经线路）
  async _openConnectionDetail(conn) {
    const modal = await loadModal("topology-connection-detail-modal");
    if (!modal) {
      return;
    }

    const setText = (id, text) => {
      const el = modal.querySelector(`#${id}`);
      if (el) {
        el.textContent = text;
      }
    };
    const toggle = (id, show) => {
      const el = modal.querySelector(`#${id}`);
      if (el) {
        el.classList.toggle("hidden", !show);
      }
    };

    let typeKey = "viz.connection_type_physical";
    if (conn.derived) {
      typeKey = "viz.connection_type_derived";
    } else if (conn.connection_type === "logical") {
      typeKey = "viz.connection_type_logical";
    }
    setText("topo-conn-detail-type", t(typeKey));
    setText("topo-conn-detail-label", conn.label || "");
    toggle("topo-conn-detail-label", Boolean(conn.label));

    setText("topo-conn-detail-source-device", conn.source_device_name || conn.source_device_id);
    setText("topo-conn-detail-target-device", conn.target_device_name || conn.target_device_id);
    setText("topo-conn-detail-source-port", conn.source_port_label || t("viz.no_port"));
    setText("topo-conn-detail-target-port", conn.target_port_label || t("viz.no_port"));

    const formatMembers = (members) =>
      (members || []).map((m) => m.port_number || m.port_id.slice(0, 8)).join(", ");
    const hasMembers =
      conn.connection_type === "logical" &&
      ((conn.source_members?.length || 0) > 0 || (conn.target_members?.length || 0) > 0);
    toggle("topo-conn-detail-members", hasMembers);
    if (hasMembers) {
      setText("topo-conn-detail-source-members", formatMembers(conn.source_members));
      setText("topo-conn-detail-target-members", formatMembers(conn.target_members));
    }

    const cables = Array.isArray(conn.cables) ? conn.cables : [];
    const hasPath = cables.length > 0;
    toggle("topo-conn-detail-path", hasPath);
    if (hasPath) {
      setText(
        "topo-conn-detail-cables",
        cables.map((c) => c.cable_label || c.cable_id.slice(0, 8)).join("  →  ")
      );
    }

    toggle("topo-conn-detail-derived-hint", Boolean(conn.derived));

    // 仅存储连线（手动物理/逻辑）可删，派生物理连线由线路管理维护
    const deleteBtn = modal.querySelector("#topo-conn-detail-delete");
    if (deleteBtn) {
      deleteBtn.classList.toggle("hidden", Boolean(conn.derived));
      deleteBtn.onclick = async () => {
        closeModal("topology-connection-detail-modal");
        await this._deleteConnection(conn.id);
      };
    }

    // onclick 赋值幂等：模态框保持打开期间重复查看连线详情不会叠加监听
    const closeButtons = modal.querySelectorAll(
      '[data-modal-id="topology-connection-detail-modal"]'
    );
    closeButtons.forEach((btn) => {
      btn.onclick = () => this._deselectConnection();
    });

    openModal("topology-connection-detail-modal");
  }

  async _deleteConnection(connectionId) {
    const confirmed = await showConfirm(t("viz.connection_delete_confirm"));
    if (!confirmed) {
      return;
    }

    const success = await this.dataManager.deleteConnection(connectionId);
    if (success) {
      this.selectedConnectionId = null;
      await this.loadTopology();
    }
  }

  _deselectConnection() {
    if (!this.selectedConnectionId) {
      return;
    }
    this.core.connectionsGroup
      .querySelectorAll(".selected, .selected-group, .conn-delete-marker")
      .forEach((el) => {
        if (el.classList.contains("conn-delete-marker")) {
          el.remove();
        } else {
          el.classList.remove("selected", "selected-group");
        }
      });
    this.selectedConnectionId = null;
  }

  /// 手动保存（工具栏按钮）：复用 _savePositions，成功时提示
  async saveLayout() {
    await this._savePositions();
  }

  /// 显示比例控制（工具栏下拉/按钮入口）
  setZoom(scale) {
    this.core.setZoom(scale);
  }

  zoomBy(factor) {
    this.core.zoomBy(factor);
  }

  /// 视野适配全部内容（工具栏“适应画布”）
  fitView() {
    this._fitView();
    this.core._updateZoomIndicator();
  }

  toggleConnectionMode() {
    this.core.setConnectionMode(!this.core.isConnectionMode);
    return this.core.isConnectionMode;
  }

  async autoDiscover() {
    const result = await this.dataManager.autoDiscover();
    if (result) {
      await this.loadTopology();
      this.groupedLayout();
      showToast(
        `${t("viz.auto_discover_done", { count: result.added_nodes })}，${result.discovered_connections ?? 0} ${t("viz.connections_unit")}`,
        "success"
      );
    }
  }

  autoLayout() {
    this.groupedLayout();
  }

  /// 空间分组：房间 → 机柜 两级（房间-机柜-设备层级）。
  /// 按房间 id（缺失时归入"未分房间"）分组；房间内有机柜设备时
  /// 各机柜成组，其余为散件。
  _collectSpatialGroups() {
    const rooms = new Map();

    this.visibleNodes().forEach((node) => {
      const roomKey = node.room_id ? `room:${node.room_id}` : "room:none";
      if (!rooms.has(roomKey)) {
        rooms.set(roomKey, {
          key: roomKey,
          label: node.room_name || t("viz.group_no_room"),
          orgLabel: node.org_name || "",
          cabinets: new Map(),
          loose: [],
          nodes: []
        });
      }
      const room = rooms.get(roomKey);
      room.nodes.push(node);

      const cabinetKey = cabinetGroupKey(node);
      if (cabinetKey) {
        if (!room.cabinets.has(cabinetKey)) {
          room.cabinets.set(cabinetKey, {
            key: cabinetKey,
            label: node.cabinet_name || t("viz.group_no_cabinet"),
            nodes: []
          });
        }
        room.cabinets.get(cabinetKey).nodes.push(node);
      } else {
        room.loose.push(node);
      }
    });

    const list = [...rooms.values()];
    list.forEach((room) => {
      room.cabinets = [...room.cabinets.values()];
    });
    return list;
  }

  /// 节点包围盒（读节点对象；DOM 领先时由 _syncPositionsFromDom 先行同步）
  _nodeRect(node) {
    return {
      x: node.x ?? 100,
      y: node.y ?? 100,
      width: node.width ?? 200,
      height: node.height ?? 100
    };
  }

  /// 分组节点包围盒（含容器留白），空分组返回 null
  _groupBox(nodes, padding, header) {
    if (nodes.length === 0) {
      return null;
    }
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    nodes.forEach((node) => {
      const r = this._nodeRect(node);
      minX = Math.min(minX, r.x);
      minY = Math.min(minY, r.y);
      maxX = Math.max(maxX, r.x + r.width);
      maxY = Math.max(maxY, r.y + r.height);
    });
    return {
      minX: minX - padding,
      minY: minY - padding - header,
      maxX: maxX + padding,
      maxY: maxY + padding
    };
  }

  /// 区域容器：房间外框 + 机柜内框。
  /// 房间内存在机柜设备时才绘制机柜容器；无机柜时不显示机柜层级。
  _renderContainers() {
    const container = this.core.containersGroup;
    container.innerHTML = "";

    this._collectSpatialGroups().forEach((room) => {
      const roomBox = this._groupBox(room.nodes, ROOM_PADDING, ROOM_HEADER);
      if (!roomBox) {
        return;
      }

      const roomG = this._drawContainerRect(roomBox, {
        kind: "room",
        key: room.key,
        padding: ROOM_PADDING,
        header: ROOM_HEADER,
        label: room.label,
        sublabel: room.orgLabel,
        count: room.nodes.length
      });
      container.appendChild(roomG);

      room.cabinets.forEach((cabinet) => {
        const cabBox = this._groupBox(cabinet.nodes, CABINET_PADDING, CABINET_HEADER);
        if (!cabBox) {
          return;
        }
        const cabG = this._drawContainerRect(cabBox, {
          kind: "cabinet",
          key: cabinet.key,
          padding: CABINET_PADDING,
          header: CABINET_HEADER,
          label: cabinet.label,
          sublabel: "",
          count: cabinet.nodes.length
        });
        container.appendChild(cabG);
      });
    });
  }

  _drawContainerRect(box, opts) {
    const g = document.createElementNS(SVG_NS, "g");
    g.classList.add("topology-container-group", `container-${opts.kind}`);
    // 容器拖动识别：kind + 分组键（与节点 dataset 的 roomKey/cabinetKey 对应）
    g.dataset.containerKind = opts.kind;
    g.dataset.containerKey = opts.key;

    const rect = document.createElementNS(SVG_NS, "rect");
    rect.classList.add("topology-container", `container-${opts.kind}`);
    rect.setAttribute("x", box.minX);
    rect.setAttribute("y", box.minY);
    rect.setAttribute("width", box.maxX - box.minX);
    rect.setAttribute("height", box.maxY - box.minY);
    rect.setAttribute("rx", opts.kind === "room" ? 10 : 8);
    g.appendChild(rect);

    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("topology-container-label");
    label.textContent = opts.label;
    label.setAttribute("x", box.minX + 14);
    label.setAttribute("y", box.minY + 20);
    label.setAttribute("dominant-baseline", "middle");
    g.appendChild(label);

    // 副标题：房间容器带组织名，机柜容器仅显示数量
    const sub = opts.sublabel
      ? `${opts.sublabel} · ${opts.count} ${t("viz.device_unit")}`
      : `${opts.count} ${t("viz.device_unit")}`;
    const count = document.createElementNS(SVG_NS, "text");
    count.classList.add("topology-container-count");
    count.textContent = sub;
    count.setAttribute("x", box.minX + 14);
    count.setAttribute("y", box.minY + 38);
    count.setAttribute("dominant-baseline", "middle");
    g.appendChild(count);

    return g;
  }

  /// 将 DOM 中的实时位置同步回节点对象（拖拽后推挤分组前调用）
  _syncPositionsFromDom() {
    this.nodes.forEach((node) => {
      const el = this.core.elementsGroup.querySelector(
        `[data-device-id="${CSS.escape(node.device_id)}"]`
      );
      const rect = el?.querySelector("rect");
      if (!rect) {
        return;
      }
      node.x = Math.round(parseFloat(rect.getAttribute("x")));
      node.y = Math.round(parseFloat(rect.getAttribute("y")));
      node.width = parseFloat(rect.getAttribute("width")) || node.width || 200;
      node.height = parseFloat(rect.getAttribute("height")) || node.height || 100;
    });
  }

  /// 推开包围盒相交的分组（先房间级、再房间内机柜级），
  /// 返回是否发生移动。fixedKey 指定的房间分组保持不动。
  _separateOverlappingGroups(fixedKey = null) {
    const rooms = this._collectSpatialGroups();
    let moved = this._separateGroupBoxes(
      rooms.map((room) => ({
        key: room.key,
        nodes: room.nodes,
        padding: ROOM_PADDING,
        header: ROOM_HEADER
      })),
      ROOM_GAP,
      fixedKey
    );

    rooms.forEach((room) => {
      const separated = this._separateGroupBoxes(
        room.cabinets.map((cab) => ({
          key: cab.key,
          nodes: cab.nodes,
          padding: CABINET_PADDING,
          header: CABINET_HEADER
        })),
        CABINET_GAP
      );
      moved = moved || separated;
    });

    return moved;
  }

  /// 分组两两推挤：包围盒相交时沿穿透量较小的轴整体平移分组内全部节点
  _separateGroupBoxes(entries, gap, fixedKey = null) {
    const boxes = entries
      .map((entry) => ({
        ...entry,
        box: this._groupBox(entry.nodes, entry.padding, entry.header)
      }))
      .filter((entry) => entry.box);
    if (boxes.length < 2) {
      return false;
    }

    let moved = false;

    const shiftGroup = (entry, dx, dy) => {
      if (dx === 0 && dy === 0) {
        return;
      }
      entry.nodes.forEach((node) => {
        node.x = Math.round((node.x || 0) + dx);
        node.y = Math.round((node.y || 0) + dy);
      });
      entry.box.minX += dx;
      entry.box.maxX += dx;
      entry.box.minY += dy;
      entry.box.maxY += dy;
      moved = true;
    };

    for (let iter = 0; iter < 40; iter++) {
      let adjusted = false;
      for (let i = 0; i < boxes.length; i++) {
        for (let j = i + 1; j < boxes.length; j++) {
          const a = boxes[i];
          const b = boxes[j];
          const overlapX = Math.min(a.box.maxX, b.box.maxX) - Math.max(a.box.minX, b.box.minX);
          const overlapY = Math.min(a.box.maxY, b.box.maxY) - Math.max(a.box.minY, b.box.minY);
          if (overlapX <= 0 || overlapY <= 0) {
            continue;
          }

          adjusted = true;
          this._separateOverlappingPair(a, b, overlapX, overlapY, gap, fixedKey, shiftGroup);
        }
      }
      if (!adjusted) {
        break;
      }
    }

    return moved;
  }

  /// 单对相交分组的推开：沿穿透量较小的轴整体平移，固定组不动
  _separateOverlappingPair(a, b, overlapX, overlapY, gap, fixedKey, shiftGroup) {
    const aFixed = a.key === fixedKey;
    const bFixed = b.key === fixedKey;

    if (overlapX <= overlapY) {
      // 水平推开：左边的向左、右边的向右
      const aShift = separateShift(aFixed, bFixed, overlapX + gap);
      const bShift = separateShift(bFixed, aFixed, overlapX + gap);
      const aLeft = (a.box.minX + a.box.maxX) / 2 <= (b.box.minX + b.box.maxX) / 2;
      shiftGroup(a, aLeft ? -aShift : aShift, 0);
      shiftGroup(b, aLeft ? bShift : -bShift, 0);
    } else {
      // 垂直推开：上边的向上、下边的向下
      const aShift = separateShift(aFixed, bFixed, overlapY + gap);
      const bShift = separateShift(bFixed, aFixed, overlapY + gap);
      const aTop = (a.box.minY + a.box.maxY) / 2 <= (b.box.minY + b.box.maxY) / 2;
      shiftGroup(a, 0, aTop ? -aShift : aShift);
      shiftGroup(b, 0, aTop ? bShift : -bShift);
    }
  }

  /// 自动布局：按 房间 → 机柜 分组排布。
  /// 房间内机柜横向排列（柜内设备网格），散件设备在机柜行下方网格排布；
  /// 房间块按网格铺开，保证房间/机柜容器互不覆盖。
  groupedLayout() {
    if (this.nodes.length === 0) {
      showToast(t("viz.no_nodes_to_layout"), "warning");
      return;
    }

    const NODE_W = 200;
    const NODE_H = 100;
    const GRID_X = 40;
    const GRID_Y = 40;
    const CAB_COLS = 3;
    const LOOSE_COLS = 4;
    const CAB_GAP = 60;
    const ROOM_COLS = 3;
    const ROOM_GAP_X = ROOM_GAP;
    const ROOM_GAP_Y = 100;

    const groups = this._collectSpatialGroups();

    // 1) 房间内布局（局部坐标，内容区从 (0, 0) 开始）
    const blocks = groups.map((room) => {
      let cursorX = 0;
      let cabinetsHeight = 0;

      room.cabinets.forEach((cabinet) => {
        const cols = Math.min(cabinet.nodes.length, CAB_COLS);
        const rows = Math.ceil(cabinet.nodes.length / CAB_COLS);
        const cabW = cols * (NODE_W + GRID_X) - GRID_X + CABINET_PADDING * 2;
        const cabH = rows * (NODE_H + GRID_Y) - GRID_Y + CABINET_PADDING * 2 + CABINET_HEADER;

        cabinet.nodes.forEach((node, i) => {
          const col = i % CAB_COLS;
          const row = Math.floor(i / CAB_COLS);
          node.x = cursorX + CABINET_PADDING + col * (NODE_W + GRID_X);
          node.y = CABINET_PADDING + CABINET_HEADER + row * (NODE_H + GRID_Y);
          node.width = NODE_W;
          node.height = NODE_H;
        });

        cabinetsHeight = Math.max(cabinetsHeight, cabH);
        cursorX += cabW + CAB_GAP;
      });

      // 散件区：机柜行下方（无机柜时从 (0, 0) 开始）
      const looseY = room.cabinets.length > 0 ? cabinetsHeight + ROOM_PADDING : 0;
      room.loose.forEach((node, i) => {
        const col = i % LOOSE_COLS;
        const row = Math.floor(i / LOOSE_COLS);
        node.x = col * (NODE_W + GRID_X);
        node.y = looseY + row * (NODE_H + GRID_Y);
        node.width = NODE_W;
        node.height = NODE_H;
      });

      const box = this._groupBox(room.nodes, ROOM_PADDING, ROOM_HEADER);
      return { room, box };
    });

    // 2) 房间块网格铺开（全局坐标）
    let cursorX = 100;
    let cursorY = 100;
    let rowHeight = 0;
    let column = 0;

    blocks.forEach(({ room, box }) => {
      if (!box) {
        return;
      }
      if (column >= ROOM_COLS) {
        column = 0;
        cursorX = 100;
        cursorY += rowHeight + ROOM_GAP_Y;
        rowHeight = 0;
      }

      const dx = Math.round(cursorX - box.minX);
      const dy = Math.round(cursorY - box.minY);
      room.nodes.forEach((node) => {
        node.x = Math.round(node.x) + dx;
        node.y = Math.round(node.y) + dy;
      });

      const blockW = box.maxX - box.minX;
      const blockH = box.maxY - box.minY;
      cursorX += blockW + ROOM_GAP_X;
      rowHeight = Math.max(rowHeight, blockH);
      column += 1;
    });

    this._renderCurrentView();
    this.saveLayout();
  }

  openDeviceDetail(deviceId) {
    if (this.callbacks?.onDeviceDetail) {
      this.callbacks.onDeviceDetail(deviceId);
    }
  }

  /** 按分组键查找容器名称（房间或机柜）。 */
  _findGroupLabel(kind, key) {
    for (const room of this._collectSpatialGroups()) {
      if (kind === "room" && room.key === key) {
        return room.label;
      }
      if (kind === "cabinet") {
        const cab = room.cabinets.find((c) => c.key === key);
        if (cab) {
          return cab.label;
        }
      }
    }
    return "";
  }

  /**
   * 容器（房间/机柜分组框）坐标模态框：暂时只配置容器左上角 x/y，
   * 保存时与容器拖动一致——组内全部设备整体平移后重排并保存坐标。
   */
  async openContainerCoordinateModal(kind, key, box) {
    if (!box) {
      return;
    }
    const modal = await loadModal("topology-container-modal");
    if (!modal) {
      return;
    }

    const nameLabel = modal.querySelector("#topology-container-name");
    if (nameLabel) {
      const kindLabel = kind === "room" ? t("viz.container_room") : t("viz.container_cabinet");
      nameLabel.textContent = `${kindLabel}: ${this._findGroupLabel(kind, key)}`;
    }

    const xInput = modal.querySelector("#topology-container-x");
    const yInput = modal.querySelector("#topology-container-y");
    xInput.value = Math.round(box.x);
    yInput.value = Math.round(box.y);

    const saveBtn = modal.querySelector("#topology-container-save-btn");
    saveBtn.onclick = async () => {
      const x = Number(xInput.value);
      const y = Number(yInput.value);
      if (!Number.isFinite(x) || !Number.isFinite(y) || x < 0 || y < 0) {
        showToast(t("common.check_input"), "warning");
        return;
      }
      closeModal("topology-container-modal");
      await this.moveContainerOrigin(kind, key, Math.round(x), Math.round(y));
    };

    openModal("topology-container-modal");
  }

  /** 将容器左上角移动到指定坐标：组内设备同步平移，重排后保存。 */
  async moveContainerOrigin(kind, key, targetX, targetY) {
    const members = this.visibleNodes().filter((node) => {
      if (kind === "cabinet") {
        return cabinetGroupKey(node) === key;
      }
      return (node.room_id ? `room:${node.room_id}` : "room:none") === key;
    });
    if (members.length === 0) {
      return;
    }

    // 以组内设备实际包围盒推算当前容器原点，保证与拖动语义一致
    const box = this._groupBox(
      members,
      kind === "room" ? ROOM_PADDING : CABINET_PADDING,
      kind === "room" ? ROOM_HEADER : CABINET_HEADER
    );
    if (!box) {
      return;
    }
    const dx = targetX - box.minX;
    const dy = targetY - box.minY;
    if (dx === 0 && dy === 0) {
      return;
    }

    members.forEach((node) => {
      node.x = Math.round((node.x || 0) + dx);
      node.y = Math.round((node.y || 0) + dy);
    });

    this._renderCurrentView();
    await this._savePositions();
  }

  /** 设备坐标更新（设备模态框保存回调）：移动节点并按拖动落定流程保存。 */
  async updateDevicePosition(deviceId, x, y) {
    const element = this.core.elementsGroup.querySelector(
      `[data-device-id="${CSS.escape(deviceId)}"]`
    );
    if (!element) {
      return;
    }
    this.core._setElementPosition(element, x, y);
    this.core._snapElementToGrid(element);
    this._handleNodeDragEnd(deviceId);
  }

  _fitView() {
    if (this.nodes.length === 0) {
      this.core.setViewBox(0, 0, 3000, 2000);
      return;
    }
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
    // 视野包含区域容器（房间/机柜分组框）
    const containerRects = this.core.containersGroup.querySelectorAll(".topology-container");
    if (containerRects.length > 0) {
      containerRects.forEach((rect) => {
        minX = Math.min(minX, parseFloat(rect.getAttribute("x")));
        minY = Math.min(minY, parseFloat(rect.getAttribute("y")));
        maxX = Math.max(
          maxX,
          parseFloat(rect.getAttribute("x")) + parseFloat(rect.getAttribute("width"))
        );
        maxY = Math.max(
          maxY,
          parseFloat(rect.getAttribute("y")) + parseFloat(rect.getAttribute("height"))
        );
      });
    } else {
      this.visibleNodes().forEach((n) => {
        const r = this._nodeRect(n);
        minX = Math.min(minX, r.x);
        minY = Math.min(minY, r.y);
        maxX = Math.max(maxX, r.x + r.width);
        maxY = Math.max(maxY, r.y + r.height);
      });
    }
    const padding = 60;
    this.core.setViewBox(
      minX - padding,
      minY - padding,
      maxX - minX + padding * 2,
      maxY - minY + padding * 2
    );
  }
}
