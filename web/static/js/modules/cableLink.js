import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  getElementValue,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import { fillSelect } from "../utils/resources.js";

const tableState = createSortState("updated_at", "desc");
let currentPage = 1;
let currentPageSize = DEFAULT_PAGE_SIZE;

const ENDPOINT_TYPE_LABELS = {
  net_outlet: t("cable_link.endpoint_net_outlet") || t("net_outlet.name"),
  // 设备端口与设备接口在前端整合为「设备接口」，列表统一显示
  device_port: t("cable_link.endpoint_device_interface"),
  device_interface: t("cable_link.endpoint_device_interface"),
  patch_panel: t("cable_link.endpoint_patch_panel")
};

// 各端点类型对应的级联「范围」选择器配置
// 信息点：房间 → 信息点；配线架：机柜 → 配线架；
// 设备接口：房间 → 机柜（可选「不限机柜」）→ 设备 → 端点（接口/端口）
const ENDPOINT_SCOPE = {
  net_outlet: { scope: "room", labelKey: "cable_link.scope_room" },
  device_interface: { scope: "room", labelKey: "cable_link.scope_room", deviceFlow: true },
  patch_panel: { scope: "cabinet", labelKey: "cable_link.scope_cabinet" }
};

// 设备接口（整合）下拉值的前缀，提交时据此还原真实 endpoint_type
const MERGED_TYPE_PREFIX = {
  device_interface: "device_interface:",
  device_port: "device_port:"
};

const LINK_TYPE_LABELS = {
  ethernet: t("cable_link.link_type_ethernet"),
  fiber: t("cable_link.link_type_fiber"),
  console: t("cable_link.link_type_console")
};

function getEndpointTypeLabel(type) {
  return ENDPOINT_TYPE_LABELS[type] || type;
}

function getLinkTypeLabel(type) {
  return LINK_TYPE_LABELS[type] || type;
}

export async function loadCableLinksData(page = currentPage, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(
      `/api/resources/cable-links?page=${page}&page_size=${currentPageSize}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`
    );
    const data = result.success ? result.data : { items: [], total: 0 };
    const items = data.items || data;
    const startIndex = (page - 1) * currentPageSize;

    renderTable("#cable-links-table", {
      data: items,
      columns: [
        {
          field: "id",
          render: (v, row, index) => startIndex + index + 1,
          className: "index-column"
        },
        {
          field: "a_endpoint_type",
          render: (v) => getEndpointTypeLabel(v),
          className: "col-center"
        },
        {
          field: "a_endpoint_label",
          render: (v) => escapeHtml(v) || "-"
        },
        {
          field: "b_endpoint_type",
          render: (v) => getEndpointTypeLabel(v),
          className: "col-center"
        },
        {
          field: "b_endpoint_label",
          render: (v) => escapeHtml(v) || "-"
        },
        { field: "link_type", render: (v) => getLinkTypeLabel(v), className: "col-center" },
        { field: "cable_label", render: (v) => escapeHtml(v) || "-" },
        { field: "length_m", render: (v) => (v != null ? `${v}m` : "-"), className: "col-center" },
        {
          field: "tested",
          render: (v) => (v ? t("cable_link.tested_yes") : t("cable_link.tested_no")),
          className: "col-center"
        },
        {
          field: "id",
          render: (v) => `
          ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit", attrs: `data-id="${v}"` })}
          ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-delete", attrs: `data-id="${v}"` })}
        `
        }
      ],
      emptyMessage: t("common.no_data")
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#cable-links-table", data, loadCableLinksData, {
        pageSize: currentPageSize,
        onPageSizeChange: (size) => {
          currentPageSize = size;
          loadCableLinksData(1);
        }
      });
    }
    updateSortIcons("cable-links-table", tableState);
  } catch (error) {
    handleError(error, t("cable_link.load_failed"), () => {
      renderTable("#cable-links-table", {
        data: [],
        columns: [],
        emptyMessage: t("common.load_failed_retry")
      });
    });
  }
}

export function initCableLinkSortEvents() {
  initSortEvents("cable-links-table", tableState, loadCableLinksData);
}

export async function editCableLink(id) {
  try {
    const result = await apiGet(`/api/resources/cable-links/${id}`);
    if (result.success) {
      openCableLinkModal(result.data);
    } else {
      showToast(`${t("cable_link.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("cable_link.load_failed"));
  }
}

export async function deleteCableLink(id) {
  await handleDelete(
    id,
    "/api/resources/cable-links",
    t("cable_link.delete_success"),
    loadCableLinksData
  );
}

// ==========================================
// 端点类型整合 + 级联选择器
// ==========================================
// 各端点类型对应一条级联链：
//   信息点 → 房间 → 信息点
//   配线架 → 机柜 → 配线架（独立表，通过 /patch-panels 拉取）
//   设备接口（整合类型）→ 房间 → 机柜（可选）→ 设备 → 端点，
//     端点下拉同时包含设备接口与设备端口（optgroup 分组），
//     选项值用前缀编码（device_interface:<id> / device_port:<id>），提交时还原真实类型。

function getSideIds(side) {
  return {
    typeSelect: `cable-link-${side}-type`,
    scopeGroup: `cable-link-${side}-scope-group`,
    scopeLabel: `cable-link-${side}-scope-label`,
    scopeSelect: `cable-link-${side}-scope`,
    flowRow: `cable-link-${side}-flow-row`,
    cabinetSelect: `cable-link-${side}-cabinet`,
    deviceSelect: `cable-link-${side}-device`,
    idSelect: `cable-link-${side}-id`
  };
}

function appendOptions(select, items) {
  items.forEach((item) => {
    const option = document.createElement("option");
    option.value = item.id;
    option.textContent = item.label;
    select.appendChild(option);
  });
}

// 范围选项：按端点类型选择 API（设备名缺失时回退显示 id）
async function loadScopeOptions(scopeType, scopeSelectId) {
  const scopeApis = {
    room: { url: "/api/resources/options/rooms", placeholderKey: "cable_link.select_room" },
    device: {
      url: "/api/resources/options/devices",
      placeholderKey: "cable_link.select_device",
      itemToLabel: (d) => d.name || d.id
    },
    cabinet: {
      url: "/api/resources/options/cabinets",
      placeholderKey: "cable_link.select_cabinet"
    }
  };
  const config = scopeApis[scopeType];
  if (!config) return;
  await fillSelect(scopeSelectId, config.url, { ...config, errorLabel: "范围" });
}

// 加载机柜选项（设备接口流程第二级：房间内的机柜，首项「不限机柜」）
async function loadCabinetOptions(side, roomId) {
  const ids = getSideIds(side);
  if (!roomId) {
    await fillSelect(ids.cabinetSelect, null, {
      placeholderKey: "cable_link.cabinet_any",
      errorLabel: "机柜"
    });
    return;
  }
  await fillSelect(ids.cabinetSelect, `/api/resources/options/cabinets?room_id=${roomId}`, {
    placeholderKey: "cable_link.cabinet_any",
    errorLabel: "机柜"
  });
}

// 加载设备选项（设备接口流程第三级：按房间或机柜过滤）
async function loadDeviceOptions(side, { roomId = null, cabinetId = null } = {}) {
  const ids = getSideIds(side);
  const query = cabinetId ? `cabinet_id=${cabinetId}` : `room_id=${roomId}`;
  await fillSelect(
    ids.deviceSelect,
    roomId || cabinetId ? `/api/resources/options/devices?${query}` : null,
    {
      placeholderKey: "cable_link.select_device",
      errorLabel: "设备",
      itemToLabel: (d) => d.name || d.id
    }
  );
}

// 动态加载端点选项（依据端点类型 + 已选范围）
async function loadEndpointOptions(endpointType, scopeValue, selectId, selectedId = null) {
  const select = elementCache.get(selectId);
  if (!select) return;

  select.innerHTML = `<option value="">${t("cable_link.select_endpoint")}</option>`;
  if (!scopeValue) return;

  try {
    if (endpointType === "net_outlet") {
      const result = await apiGet(
        `/api/resources/options/net-outlets?room_id=${scopeValue}`
      );
      const data = result.success ? result.data : {};
      const items = (data.items || data || []).map((o) => ({ id: o.id, label: o.name }));
      appendOptions(select, items);
    } else if (endpointType === "device_interface") {
      // 整合：并发拉取该设备的接口与端口，optgroup 分组，值前缀编码
      const [ifaceRes, portRes] = await Promise.all([
        apiGet(`/api/resources/devices/${scopeValue}/interfaces?page_size=1000`),
        apiGet(`/api/resources/devices/${scopeValue}/device-ports?page_size=1000`)
      ]);
      const ifaces = (ifaceRes.success ? ifaceRes.data?.items || ifaceRes.data || [] : [])
        .filter((di) => !di.physical_type || di.physical_type !== "virtual")
        .map((di) => ({ id: di.id, label: di.name || di.id }));
      const ports = (portRes.success ? portRes.data?.items || portRes.data || [] : []).map(
        (sp) => ({ id: sp.id, label: sp.port_number || sp.port_name || sp.id })
      );

      if (ifaces.length) {
        const og = document.createElement("optgroup");
        og.label = t("cable_link.endpoint_device_interface");
        ifaces.forEach((i) => {
          const o = document.createElement("option");
          o.value = `${MERGED_TYPE_PREFIX.device_interface}${i.id}`;
          o.textContent = i.label;
          og.appendChild(o);
        });
        select.appendChild(og);
      }
      if (ports.length) {
        const og = document.createElement("optgroup");
        og.label = t("cable_link.endpoint_device_port");
        ports.forEach((p) => {
          const o = document.createElement("option");
          o.value = `${MERGED_TYPE_PREFIX.device_port}${p.id}`;
          o.textContent = p.label;
          og.appendChild(o);
        });
        select.appendChild(og);
      }
    } else if (endpointType === "patch_panel") {
      const result = await apiGet(
        `/api/resources/patch-panels?cabinet_id=${scopeValue}&page_size=1000`
      );
      const data = result.success ? result.data : {};
      const items = (data.items || data || []).map((pp) => ({ id: pp.id, label: pp.name }));
      appendOptions(select, items);
    }
  } catch (e) {
    console.error("加载端点选项失败:", e);
  }

  if (selectedId) select.value = selectedId;
}

// 类型切换：更新范围选择器标签/可见性、设备接口流程级联行的显隐，并加载范围选项
async function onTypeChange(side) {
  const ids = getSideIds(side);
  const type = elementCache.get(ids.typeSelect)?.value;
  const scopeGroup = elementCache.get(ids.scopeGroup);
  const scopeLabel = elementCache.get(ids.scopeLabel);
  const scopeSelect = elementCache.get(ids.scopeSelect);
  const flowRow = elementCache.get(ids.flowRow);
  const cabinetSelect = elementCache.get(ids.cabinetSelect);
  const deviceSelect = elementCache.get(ids.deviceSelect);
  const idSelect = elementCache.get(ids.idSelect);

  if (idSelect) idSelect.innerHTML = `<option value="">${t("cable_link.select_endpoint")}</option>`;
  if (scopeSelect) scopeSelect.value = "";

  const cfg = type ? ENDPOINT_SCOPE[type] : null;
  if (!cfg) {
    if (scopeGroup) scopeGroup.style.display = "none";
    flowRow?.classList.add("hidden");
    return;
  }
  if (scopeLabel) scopeLabel.textContent = t(cfg.labelKey) || cfg.scope;
  if (scopeGroup) scopeGroup.style.display = "";
  // 设备接口流程：显示 机柜（可选）+ 设备 级联行，其余类型隐藏
  flowRow?.classList.toggle("hidden", !cfg.deviceFlow);
  if (cfg.deviceFlow) {
    if (cabinetSelect)
      cabinetSelect.innerHTML = `<option value="">${t("cable_link.cabinet_any")}</option>`;
    if (deviceSelect)
      deviceSelect.innerHTML = `<option value="">${t("cable_link.select_device")}</option>`;
  }
  await loadScopeOptions(cfg.scope, ids.scopeSelect);
}

// 范围切换：信息点/配线架直接加载端点；设备接口加载机柜与设备选项
async function onScopeChange(side) {
  const ids = getSideIds(side);
  const type = elementCache.get(ids.typeSelect)?.value;
  const scopeValue = elementCache.get(ids.scopeSelect)?.value;
  const idSelect = elementCache.get(ids.idSelect);

  if (idSelect) idSelect.innerHTML = `<option value="">${t("cable_link.select_endpoint")}</option>`;

  if (type === "device_interface") {
    await loadCabinetOptions(side, scopeValue);
    await loadDeviceOptions(side, { roomId: scopeValue });
    return;
  }
  await loadEndpointOptions(type, scopeValue, ids.idSelect);
}

// 机柜切换（设备接口流程）：按机柜过滤设备，「不限机柜」时回到房间范围
async function onCabinetChange(side) {
  const ids = getSideIds(side);
  const scopeValue = elementCache.get(ids.scopeSelect)?.value;
  const cabinetId = elementCache.get(ids.cabinetSelect)?.value;
  const idSelect = elementCache.get(ids.idSelect);

  if (idSelect) idSelect.innerHTML = `<option value="">${t("cable_link.select_endpoint")}</option>`;
  await loadDeviceOptions(side, cabinetId ? { cabinetId } : { roomId: scopeValue });
}

// 设备切换（设备接口流程）：加载该设备的接口与端口
async function onDeviceChange(side) {
  const ids = getSideIds(side);
  const deviceId = elementCache.get(ids.deviceSelect)?.value;
  if (deviceId) {
    await loadEndpointOptions("device_interface", deviceId, ids.idSelect);
  }
}

// 还原整合类型的真实 endpoint_type 与 id
function decodeEndpoint(type, rawValue) {
  if (!rawValue) return { type: "", id: "" };
  if (type === "device_interface") {
    const idx = rawValue.indexOf(":");
    if (idx > 0) {
      return { type: rawValue.slice(0, idx), id: rawValue.slice(idx + 1) };
    }
  }
  return { type, id: rawValue };
}

let changeHandlers = {};

// 编辑回填：按端点类型驱动级联选择器（类型→范围→机柜（可选）→设备→端点），
// 与新建表单完全复用，回填后所有选择器保持可编辑
async function populateEndpointCascade(side, ep) {
  const ids = getSideIds(side);
  // device_port / device_interface 统一映射为「设备接口」选项
  const displayType =
    ep.type === "device_port" || ep.type === "device_interface" ? "device_interface" : ep.type;
  elementCache.setValue(ids.typeSelect, displayType);
  await onTypeChange(side);

  if (displayType === "net_outlet") {
    elementCache.setValue(ids.scopeSelect, ep.roomId || "");
    await onScopeChange(side);
    elementCache.setValue(ids.idSelect, ep.id);
  } else if (displayType === "patch_panel") {
    elementCache.setValue(ids.scopeSelect, ep.cabinetId || "");
    await onScopeChange(side);
    elementCache.setValue(ids.idSelect, ep.id);
  } else if (displayType === "device_interface") {
    elementCache.setValue(ids.scopeSelect, ep.roomId || "");
    await onScopeChange(side);
    if (ep.cabinetId) {
      elementCache.setValue(ids.cabinetSelect, ep.cabinetId);
      await onCabinetChange(side);
    }
    elementCache.setValue(ids.deviceSelect, ep.deviceId || "");
    await onDeviceChange(side);
    // 整合类型的端点值为前缀编码（device_interface:<id> / device_port:<id>）
    const prefix =
      ep.type === "device_port"
        ? MERGED_TYPE_PREFIX.device_port
        : MERGED_TYPE_PREFIX.device_interface;
    elementCache.setValue(ids.idSelect, `${prefix}${ep.id}`);
  }
}

export async function openCableLinkModal(cableLink = null) {
  await openModal("cable-link-modal");

  const title = elementCache.get("cable-link-modal-title");
  const form = elementCache.get("cable-link-form");
  const isEdit = !!cableLink;

  // 清理旧事件监听并绑定级联处理器（类型/范围/机柜/设备）
  [
    "a-type",
    "a-scope",
    "a-cabinet",
    "a-device",
    "b-type",
    "b-scope",
    "b-cabinet",
    "b-device"
  ].forEach((key) => {
    const [side, evt] = key.split("-");
    const sel = elementCache.get(`cable-link-${key}`);
    if (sel && changeHandlers[key]) {
      sel.removeEventListener("change", changeHandlers[key]);
    }
    const handlers = {
      type: async () => {
        await onTypeChange(side);
      },
      scope: async () => {
        await onScopeChange(side);
      },
      cabinet: async () => {
        await onCabinetChange(side);
      },
      device: async () => {
        await onDeviceChange(side);
      }
    };
    changeHandlers[key] = handlers[evt];
    if (sel) sel.addEventListener("change", changeHandlers[key]);
  });

  // 编辑与新建复用同一表单：级联选择器始终可见、可编辑

  if (isEdit) {
    title.textContent = t("cable_link.edit");
    elementCache.setValue("cable-link-id", cableLink.id);
    elementCache.setValue("cable-link-link-type", cableLink.link_type || "ethernet");
    elementCache.setValue("cable-link-cable-label", cableLink.cable_label || "");
    elementCache.setValue(
      "cable-link-length",
      cableLink.length_m != null ? cableLink.length_m : ""
    );
    elementCache.setValue("cable-link-tested", cableLink.tested ? "1" : "0");
    // 按当前端点回填级联（后端已随详情返回端点所属房间/机柜/设备）
    await populateEndpointCascade("a", {
      type: cableLink.a_endpoint_type,
      id: cableLink.a_endpoint_id,
      roomId: cableLink.a_room_id,
      cabinetId: cableLink.a_cabinet_id,
      deviceId: cableLink.a_device_id
    });
    await populateEndpointCascade("b", {
      type: cableLink.b_endpoint_type,
      id: cableLink.b_endpoint_id,
      roomId: cableLink.b_room_id,
      cabinetId: cableLink.b_cabinet_id,
      deviceId: cableLink.b_device_id
    });
  } else {
    title.textContent = t("cable_link.add");
    if (form) form.reset();
    elementCache.setValue("cable-link-id", "");
    elementCache.setValue("cable-link-link-type", "ethernet");
    elementCache.setValue("cable-link-tested", "0");
    // 默认类型为信息点：初始化范围选择器（房间）
    await onTypeChange("a");
    await onTypeChange("b");
  }
}

export async function submitCableLinkForm() {
  const id = getElementValue("cable-link-id");
  const linkType = getElementValue("cable-link-link-type") || "ethernet";
  const cableLabel = getElementValue("cable-link-cable-label");
  const lengthStr = getElementValue("cable-link-length");
  const testedVal = getElementValue("cable-link-tested");
  const tested = testedVal === "1" || testedVal === "true";

  // 编辑与新建共用端点表单：解码两端点并校验
  const aType = getElementValue("cable-link-a-type");
  const aRawId = getElementValue("cable-link-a-id");
  const bType = getElementValue("cable-link-b-type");
  const bRawId = getElementValue("cable-link-b-id");

  const a = decodeEndpoint(aType, aRawId);
  const b = decodeEndpoint(bType, bRawId);

  if (!a.type || !a.id || !b.type || !b.id) {
    showToast(t("cable_link.endpoint_required"), "warning");
    return;
  }
  if (a.type === b.type && a.id === b.id) {
    showToast(t("cable_link.no_self_link"), "warning");
    return;
  }

  const payload = {
    a_endpoint_type: a.type,
    a_endpoint_id: a.id,
    b_endpoint_type: b.type,
    b_endpoint_id: b.id,
    link_type: linkType,
    cable_label: cableLabel || null,
    length_m: lengthStr ? parseFloat(lengthStr) : null,
    tested
  };

  try {
    const result = id
      ? await apiPut(`/api/resources/cable-links/${id}`, payload)
      : await apiPost("/api/resources/cable-links", payload);
    if (result.success) {
      showToast(t("cable_link.save_success"), "success");
      closeModal("cable-link-modal");
      await loadCableLinksData();
    } else {
      showToast(result.message || t("cable_link.save_failed"), "error");
    }
  } catch (error) {
    handleError(error, t("cable_link.save_failed"));
  }
}
