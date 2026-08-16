/**
 * 资源下拉选项加载工具。
 *
 * 各 loadXxxForSelect 统一基于 fillSelect 实现：请求 API → 清空 →
 * 占位项 → 逐项填充 → 恢复原选中值；失败时置不可选的提示项。
 */
import { apiGet } from "./apiClient.js";
import { t } from "./i18n.js";

/** 从 API 响应中提取列表（兼容裸数组与 {items} 分页结构）。 */
function extractItems(result) {
  if (!result.success || !result.data) return [];
  if (Array.isArray(result.data)) return result.data;
  if (Array.isArray(result.data.items)) return result.data.items;
  return [];
}

/** 构造单个 <option>。 */
function buildOption(value, label, disabled = false) {
  const option = document.createElement("option");
  option.value = value;
  option.textContent = label;
  option.disabled = disabled;
  return option;
}

/**
 * 通用下拉填充。
 *
 * @param {string} selectId 目标 select 元素 id
 * @param {string|null} url API 地址（返回数组或 {items} 结构）；传入 null
 *   且提供 opts.items 时不发请求（用于已预取或纯占位场景）
 * @param {Object} [opts]
 * @param {Array} [opts.items] 预取的候选项（优先于 url 请求）
 * @param {string} [opts.placeholderKey] 空值占位项的 i18n 键
 * @param {string} [opts.emptyKey] 过滤后无数据时的提示项 i18n 键
 * @param {Function} [opts.filter] 项过滤谓词（item => boolean）
 * @param {Function} [opts.itemToLabel] 自定义展示文本（item => string，默认 item.name）
 * @param {string} [opts.errorLabel] 失败日志中的资源名
 */
export async function fillSelect(selectId, url, opts = {}) {
  const {
    items: prefetched,
    placeholderKey,
    emptyKey,
    filter,
    itemToLabel,
    errorLabel = "选项"
  } = opts;
  const select = document.getElementById(selectId);
  if (!select) return;

  const currentValue = select.value;
  select.replaceChildren();
  if (placeholderKey) {
    select.appendChild(buildOption("", t(placeholderKey)));
  }

  let fetched = [];
  if (prefetched) {
    fetched = prefetched;
  } else if (url) {
    try {
      fetched = extractItems(await apiGet(url));
    } catch (error) {
      console.error(`加载${errorLabel}失败:`, error);
      select.appendChild(buildOption("", t("common.load_failed"), true));
      return;
    }
  }

  const items = fetched.filter(filter || (() => true));
  for (const item of items) {
    select.appendChild(buildOption(item.id, itemToLabel ? itemToLabel(item) : item.name));
  }

  if (emptyKey && items.length === 0) {
    select.appendChild(buildOption("", t(emptyKey), true));
  }

  if (currentValue) {
    select.value = currentValue;
  }
}

/** 加载网络区域选项（无空值占位，空列表时提示先建区域）。 */
export function loadNetworkTypeOptions(selectId = "network-type") {
  return fillSelect(selectId, "/api/resources/network-regions?page_size=1000", {
    emptyKey: "network.add_region_first",
    errorLabel: "网络区域"
  });
}

/**
 * 加载房间选项。
 *
 * onlyOffice 仅保留办公室；includeVisualization 时同步填充可视化
 * 房间选择器（room-select，排除数据中心/弱电井）。
 */
export async function loadRoomsForSelect(selectId = "workstation-room", options = {}) {
  const { onlyOffice = false, includeVisualization = true } = options;
  await fillSelect(selectId, "/api/resources/rooms?page_size=1000", {
    placeholderKey: onlyOffice ? "room.select_office" : "room.select_room",
    emptyKey: onlyOffice ? "room.no_office_data" : "room.no_room_data",
    errorLabel: "房间",
    filter: (room) => {
      const roomType = room.room_type ? room.room_type.toLowerCase() : "";
      return onlyOffice ? roomType === "office" : true;
    }
  });

  if (!includeVisualization) return;
  await fillSelect("room-select", "/api/resources/rooms?page_size=1000", {
    errorLabel: "可视化房间",
    filter: (room) => {
      const roomType = room.room_type ? room.room_type.toLowerCase() : "";
      return roomType !== "data_center" && roomType !== "telecom_closet";
    }
  });
}

/** 加载机房类房间（数据中心/弱电井）选项。 */
export function loadDataCenterRoomsForSelect(selectId = "cabinet-room") {
  return fillSelect(selectId, "/api/resources/rooms?page_size=1000", {
    placeholderKey: "room.select_datacenter",
    emptyKey: "room.no_datacenter_data",
    errorLabel: "机房",
    filter: (room) => {
      const roomType = room.room_type ? room.room_type.toLowerCase() : "";
      return roomType === "data_center" || roomType === "telecom_closet";
    }
  });
}

/** 在容器内渲染房间继承的网段清单（机柜表单用）。 */
export async function loadRoomNetworksForCabinet(
  roomId,
  containerId = "cabinet-inherited-networks"
) {
  const container = document.getElementById(containerId);
  if (!container) return;

  const showHint = (key) => {
    container.replaceChildren();
    const p = document.createElement("p");
    p.className = "text-muted";
    p.textContent = t(key);
    container.appendChild(p);
  };

  if (!roomId) {
    showHint("cabinet.select_room_first");
    return;
  }

  try {
    const roomResult = await apiGet(`/api/resources/rooms/${roomId}`);

    if (!roomResult.success || !roomResult.data) {
      showHint("common.load_failed_retry");
      return;
    }

    const networks = roomResult.data.networks || [];
    if (networks.length === 0) {
      showHint("cabinet.no_network_config");
      return;
    }

    const list = document.createElement("ul");
    list.className = "list-group";
    for (const network of networks) {
      const li = document.createElement("li");
      li.className = "list-group-item";
      const name = document.createElement("span");
      name.className = "font-weight-bold";
      name.textContent = network.name;
      const region = document.createElement("span");
      region.className = "text-muted";
      region.textContent = `(${network.network_region})`;
      li.append(name, region);
      for (const [label, cidr] of [
        ["IPv4", network.ipv4_cidr],
        ["IPv6", network.ipv6_cidr]
      ]) {
        if (!cidr) continue;
        const div = document.createElement("div");
        div.className = "small";
        div.textContent = `${label}: ${cidr}`;
        li.appendChild(div);
      }
      list.appendChild(li);
    }
    container.replaceChildren(list);
  } catch (error) {
    console.error("加载房间网段配置失败:", error);
    showHint("common.load_failed_retry");
  }
}

/** 组织树展平为带缩进深度的列表（下拉展示用）。 */
function flattenOrgTree(nodes, depth = 0) {
  const result = [];
  for (const node of nodes) {
    result.push({ id: node.id, name: node.name, org_type: node.org_type, depth });
    if (node.children && node.children.length > 0) {
      result.push(...flattenOrgTree(node.children, depth + 1));
    }
  }
  return result;
}

/** 加载组织选项（树形按深度缩进展示）。 */
export function loadOrgsForSelect(selectId = "room-org-id") {
  return (async () => {
    let flatOrgs = [];
    try {
      const result = await apiGet("/api/resources/organizations/tree");
      if (result.success && result.data) {
        flatOrgs = flattenOrgTree(result.data);
      }
    } catch (error) {
      console.error("加载组织选项失败:", error);
    }
    await fillSelect(selectId, null, {
      items: flatOrgs,
      placeholderKey: "organization.select_org",
      errorLabel: "组织",
      itemToLabel: (org) =>
        `\u00A0\u00A0\u00A0\u00A0`.repeat(org.depth) + `${org.name} (${org.org_type})`
    });
  })();
}

/** 加载设备模板选项。 */
export function loadDeviceTemplatesForSelect(selectId) {
  return fillSelect(selectId, "/api/resources/device-templates", {
    placeholderKey: "device.select_template",
    errorLabel: "设备模板"
  });
}

/** 加载工位选项（可按房间过滤）。 */
export function loadWorkstationsForSelect(selectId, roomId = null) {
  const url = roomId
    ? `/api/resources/workstations?room_id=${roomId}&page_size=1000`
    : "/api/resources/workstations?page_size=1000";
  return fillSelect(selectId, url, {
    placeholderKey: "device.select_workstation",
    errorLabel: "工位"
  });
}

/** 加载机柜选项（可按房间过滤）。 */
export function loadCabinetsForSelect(selectId, roomId = null) {
  const params = new URLSearchParams({ page_size: "1000" });
  if (roomId) params.set("room_id", roomId);
  return fillSelect(selectId, `/api/resources/cabinets?${params.toString()}`, {
    placeholderKey: "device.select_cabinet",
    errorLabel: "机柜"
  });
}

/** 加载机位选项（必须先选定机柜；roomId 仅用于回显无机柜的历史机位）。 */
export function loadPositionsForSelect(selectId, cabinetId = null, roomId = null) {
  if (!cabinetId && !roomId) {
    // 无过滤条件时仅展示占位项
    return fillSelect(selectId, null, {
      placeholderKey: "device.select_position",
      errorLabel: "机位"
    });
  }
  const params = new URLSearchParams({ page_size: "1000" });
  if (cabinetId) params.set("cabinet_id", cabinetId);
  else params.set("room_id", roomId);
  return fillSelect(selectId, `/api/resources/positions?${params.toString()}`, {
    placeholderKey: "device.select_position",
    errorLabel: "机位"
  });
}
