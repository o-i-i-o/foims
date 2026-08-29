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
  if (!result.success || !result.data) {
    return [];
  }
  if (Array.isArray(result.data)) {
    return result.data;
  }
  if (Array.isArray(result.data.items)) {
    return result.data.items;
  }
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
 * 等待目标元素出现（模态框注入前的短暂窗口）。
 * 调用方常把填充函数与 openModal 并行执行，模态框 DOM 可能尚未注入。
 * 用 setTimeout 轮询而非 rAF：隐藏标签页 rAF 完全暂停会让超时判定失效。
 */
function waitForElement(id, timeoutMs = 3000) {
  const direct = document.getElementById(id);
  if (direct) {
    return Promise.resolve(direct);
  }

  return new Promise((resolve) => {
    const start = performance.now();
    const check = () => {
      const el = document.getElementById(id);
      if (el || performance.now() - start > timeoutMs) {
        resolve(el);
      } else {
        setTimeout(check, 50);
      }
    };
    check();
  });
}

/** 解析下拉目标：字符串按 id 等待元素出现（模态框注入窗口），元素引用直接使用。 */
function resolveSelectTarget(target) {
  if (typeof target !== "string") {
    return Promise.resolve(target);
  }
  return waitForElement(target);
}

/** 生成日志用的目标描述（元素引用没有 id 可打）。 */
function describeSelectTarget(target) {
  return typeof target === "string" ? `#${target}` : "目标 select 元素";
}

/**
 * 下拉选项请求短期缓存（10s TTL）：弹窗短时间内重复打开时不再全量重拉
 * page_size=1000 的选项列表。资源写操作成功后由 apiClient 广播的
 * ipma:data-mutation 事件即时失效（见 apiClient.js），避免"刚建的资源
 * 10 秒内选不到、编辑回显被清空"的新鲜度回归。
 */
const OPTION_FETCH_TTL = 10 * 1000;
const optionFetchCache = new Map();

/**
 * 拉取下拉选项原始响应（带 10s TTL 共享缓存）。
 * 导出供调用方需要"先取数据再做分支渲染"的场景复用同一份缓存。
 */
export function fetchOptionItems(url) {
  const hit = optionFetchCache.get(url);
  if (hit && Date.now() - hit.time < OPTION_FETCH_TTL) {
    return hit.promise;
  }
  const promise = apiGet(url).catch((error) => {
    optionFetchCache.delete(url);
    throw error;
  });
  optionFetchCache.set(url, { time: Date.now(), promise });
  return promise;
}

document.addEventListener("ipma:data-mutation", () => {
  optionFetchCache.clear();
});

/**
 * 通用下拉填充。
 *
 * @param {string|Element} selectTarget 目标 select 元素 id 或元素引用
 *   （元素引用用于动态行内没有 id 的 select，如房间网段配置行）
 * @param {string|null} url API 地址（返回数组或 {items} 结构）；传入 null
 *   且提供 opts.items 时不发请求（用于已预取或纯占位场景）
 * @param {Object} [opts]
 * @param {Array} [opts.items] 预取的候选项（优先于 url 请求）
 * @param {string} [opts.placeholderKey] 空值占位项的 i18n 键
 * @param {string} [opts.emptyKey] 过滤后无数据时的提示项 i18n 键
 * @param {Function} [opts.filter] 项过滤谓词（item => boolean）
 * @param {Function} [opts.itemToLabel] 自定义展示文本（item => string，默认 item.name）
 * @param {string} [opts.errorLabelKey] 失败日志中资源名的 i18n 键
 */
// 各下拉的加载序号：级联快速切换时旧响应晚到会覆盖/追加过期选项，
// 序号不符则丢弃本次结果。字符串目标（元素 id）与元素目标分别用
// Map/WeakMap 计数——元素可能无 id（如房间多网段的行内 select），
// 混用会把全部无 id 元素并到同一计数器互相作废
const fillSelectSeqById = new Map();
const fillSelectSeqByEl = new WeakMap();

function bumpFillSelectSeq(selectTarget) {
  if (typeof selectTarget === "string") {
    const seq = (fillSelectSeqById.get(selectTarget) || 0) + 1;
    fillSelectSeqById.set(selectTarget, seq);
    return () => fillSelectSeqById.get(selectTarget) !== seq;
  }
  if (selectTarget) {
    const seq = (fillSelectSeqByEl.get(selectTarget) || 0) + 1;
    fillSelectSeqByEl.set(selectTarget, seq);
    return () => fillSelectSeqByEl.get(selectTarget) !== seq;
  }
  return () => false;
}

export async function fillSelect(selectTarget, url, opts = {}) {
  const {
    items: prefetched,
    placeholderKey,
    emptyKey,
    filter,
    itemToLabel,
    errorLabelKey = "common.option"
  } = opts;
  const isStale = bumpFillSelectSeq(selectTarget);

  let fetched = [];
  if (prefetched) {
    fetched = prefetched;
  } else if (url) {
    try {
      fetched = extractItems(await fetchOptionItems(url));
      if (isStale()) {
        return; // 已有更新的加载在途，丢弃过期结果
      }
    } catch (error) {
      console.error(`加载${t(errorLabelKey)}失败:`, error);
      const errSelect = await resolveSelectTarget(selectTarget);
      if (errSelect) {
        errSelect.replaceChildren(buildOption("", t("common.load_failed"), true));
      }
      return;
    }
  }

  // 数据就绪后再查询目标元素：入口即查会在"与 openModal 并行"的调用方式下
  // 因模态框 DOM 未注入而静默丢弃（曾导致设备模板下拉为空、编辑回显丢失）
  const select = await resolveSelectTarget(selectTarget);
  if (!select) {
    console.warn(`fillSelect: ${describeSelectTarget(selectTarget)} 不存在，已跳过填充`);
    return;
  }
  if (isStale()) {
    return;
  }

  const currentValue = select.value;

  const items = fetched.filter(filter || (() => true));
  const options = [placeholderKey ? buildOption("", t(placeholderKey)) : null]
    .filter(Boolean)
    .concat(items.map((item) => buildOption(item.id, itemToLabel ? itemToLabel(item) : item.name)));

  if (emptyKey && items.length === 0) {
    options.push(buildOption("", t(emptyKey), true));
  }

  // 一次性替换全部选项，避免逐项 append 触发多次重排
  select.replaceChildren(...options);

  if (currentValue) {
    select.value = currentValue;
  }
}

/** 加载网络区域选项（无空值占位，空列表时提示先建区域）。 */
export function loadNetworkRegionOptions(selectId = "network-region") {
  return fillSelect(selectId, "/api/resources/network-regions?page_size=1000", {
    emptyKey: "network.add_region_first",
    errorLabelKey: "common.network_region"
  });
}

/**
 * 房间类型分类。
 *
 * 办公类（工位管理范围）：办公室、大厅、前台；机房类（机柜管理范围）：
 * 机房、弱电井。"其他"无固定性质，两类筛选均包含。
 */
const OFFICE_ROOM_TYPE_LIST = "office,lobby,reception,other";
const DATA_CENTER_ROOM_TYPE_LIST = "data_center,telecom_closet,other";

/** 构造房间列表 API 地址（org_id / room_type 过滤由后端执行）。 */
function buildRoomsUrl(orgId, roomTypes) {
  const params = new URLSearchParams({ page_size: "1000" });
  if (orgId) {
    params.set("org_id", orgId);
  }
  if (roomTypes) {
    params.set("room_type", roomTypes);
  }
  return `/api/resources/rooms?${params.toString()}`;
}

/**
 * 加载房间选项（可按组织与房间类型过滤）。
 *
 * onlyOffice 仅保留办公类房间（含"其他"）。
 */
export async function loadRoomsForSelect(selectId = "workstation-room", options = {}) {
  const { onlyOffice = false, orgId = null, roomType = null } = options;
  const typeList = onlyOffice ? OFFICE_ROOM_TYPE_LIST : roomType;
  await fillSelect(selectId, buildRoomsUrl(orgId, typeList), {
    placeholderKey: onlyOffice ? "room.select_office" : "room.select_room",
    emptyKey: onlyOffice ? "room.no_office_data" : "room.no_room_data",
    errorLabelKey: "common.room"
  });
}

/**
 * 拉取房间列表数据（与 loadRoomsForSelect 共享下拉缓存）。
 * 供需要房间附加字段（如 org_id）的调用方复用，避免重复请求。
 */
export async function fetchRoomsForOptions(orgId = null, onlyOffice = true) {
  const url = buildRoomsUrl(orgId, onlyOffice ? OFFICE_ROOM_TYPE_LIST : null);
  return extractItems(await fetchOptionItems(url));
}

/** 加载机房类房间（数据中心/弱电井/其他）选项。 */
export function loadDataCenterRoomsForSelect(selectId = "cabinet-room", orgId = null) {
  return fillSelect(selectId, buildRoomsUrl(orgId, DATA_CENTER_ROOM_TYPE_LIST), {
    placeholderKey: "room.select_datacenter",
    emptyKey: "room.no_datacenter_data",
    errorLabelKey: "room.type_datacenter"
  });
}

/**
 * 可视化页房间选择器。
 *
 * roomTypes 为 "office"（工位可视化全类别）、"datacenter"（机柜可视化全类别）
 * 或具体类型（如 "office"/"other"），可叠加组织过滤。
 */
export function loadVisualizationRoomsForSelect(selectId, roomTypes, orgId = null) {
  let resolved = roomTypes;
  if (roomTypes === "office") {
    resolved = OFFICE_ROOM_TYPE_LIST;
  } else if (roomTypes === "datacenter") {
    resolved = DATA_CENTER_ROOM_TYPE_LIST;
  }
  return fillSelect(selectId, buildRoomsUrl(orgId, resolved), {
    placeholderKey: "room.select_room",
    emptyKey: "room.no_room_data",
    errorLabelKey: "common.room"
  });
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
      errorLabelKey: "common.organization",
      itemToLabel: (org) =>
        `${"\u00A0\u00A0\u00A0\u00A0".repeat(org.depth)}${org.name} (${org.org_type})`
    });
  })();
}

/** 获取组织及其全部后代的 id 集合（拓扑按组织筛选用；树中未命中时退化为自身）。 */
export async function getOrgSubtreeIds(orgId) {
  const ids = new Set();
  try {
    const result = await apiGet("/api/resources/organizations/tree");
    if (result.success && Array.isArray(result.data)) {
      const walk = (nodes, inside) => {
        nodes.forEach((node) => {
          const hit = inside || node.id === orgId;
          if (hit) {
            ids.add(node.id);
          }
          if (node.children && node.children.length > 0) {
            walk(node.children, hit);
          }
        });
      };
      walk(result.data, false);
    }
  } catch (error) {
    console.error("加载组织树失败:", error);
  }
  if (ids.size === 0) {
    ids.add(orgId);
  }
  return ids;
}

/** 加载设备模板选项。 */
export function loadDeviceTemplatesForSelect(selectId) {
  return fillSelect(selectId, "/api/resources/device-templates", {
    placeholderKey: "device.select_template",
    errorLabelKey: "common.template"
  });
}

/**
 * 加载工位选项（按房间过滤）。
 *
 * 工位必属于房间：未选定房间时不发请求，仅展示"请先选择房间"占位项，
 * 避免出现脱离房间上下文的全量工位列表。
 */
export function loadWorkstationsForSelect(selectId, roomId = null) {
  if (!roomId) {
    return fillSelect(selectId, null, {
      placeholderKey: "device.select_room_first",
      errorLabelKey: "common.workstation"
    });
  }
  return fillSelect(selectId, `/api/resources/workstations?room_id=${roomId}&page_size=1000`, {
    placeholderKey: "device.select_workstation",
    errorLabelKey: "common.workstation"
  });
}

/**
 * 加载机柜选项（按房间过滤）。
 *
 * 机柜必属于房间：未选定房间时不发请求，仅展示"请先选择房间"占位项，
 * 避免出现脱离房间上下文的全量机柜列表。
 */
export function loadCabinetsForSelect(selectId, roomId = null) {
  if (!roomId) {
    return fillSelect(selectId, null, {
      placeholderKey: "device.select_room_first",
      errorLabelKey: "common.cabinet"
    });
  }
  const params = new URLSearchParams({ room_id: roomId, page_size: "1000" });
  return fillSelect(selectId, `/api/resources/cabinets?${params.toString()}`, {
    placeholderKey: "device.select_cabinet",
    errorLabelKey: "common.cabinet"
  });
}

/** 加载机位选项（必须先选定机柜；roomId 仅用于回显无机柜的历史机位）。 */
export function loadPositionsForSelect(selectId, cabinetId = null, roomId = null) {
  if (!cabinetId && !roomId) {
    // 无过滤条件时仅展示占位项
    return fillSelect(selectId, null, {
      placeholderKey: "device.select_position",
      errorLabelKey: "common.position"
    });
  }
  const params = new URLSearchParams({ page_size: "1000" });
  if (cabinetId) {
    params.set("cabinet_id", cabinetId);
  } else {
    params.set("room_id", roomId);
  }
  return fillSelect(selectId, `/api/resources/positions?${params.toString()}`, {
    placeholderKey: "device.select_position",
    errorLabelKey: "common.position"
  });
}
