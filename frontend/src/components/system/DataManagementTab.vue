<script setup lang="ts">
/**
 * 数据管理页签（移植自旧版 data-management 分区 + systemManager.js）：
 * Agent 上报服务 / SNMP Trap 接收 / 导入导出（CSV 校验含 magic bytes
 * 双重防线）/ 日志管理四张卡片。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import { downloadBlobResult } from "../../utils/download";
import ModalShell from "../ModalShell.vue";
import AgentServiceCard from "./AgentServiceCard.vue";
import SnmpTrapCard from "./SnmpTrapCard.vue";
import LogsManagementCard from "./LogsManagementCard.vue";

/** 单表导入结果（后端 json! 动态构造，未生成 ts-rs 类型） */
interface ImportResultRow {
  table?: string;
  inserted?: number;
  updated?: number;
}

/** CSV 导入响应载荷 */
interface ImportCsvResponse {
  results?: ImportResultRow[] | null;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();

const readonly = isSectionReadonly("system");

// ==================== 导入导出 ====================

// 导入文件大小上限（与后端一致）
const MAX_IMPORT_SIZE = 50 * 1024 * 1024;

const importFileInput = ref<HTMLInputElement | null>(null);
const importing = ref(false);
// 导入结果明细弹窗（后端逐表返回插入/更新行数）
const importResultVisible = ref(false);
const importResults = ref<ImportResultRow[]>([]);

const exportType = ref("all");
const exporting = ref(false);

// 前端校验上传文件的真实类型：扩展名 + magic bytes（zip 为 PK 头，
// csv 须为不含 NUL 的 UTF-8 文本），与后端校验形成双重防线
async function validateImportFile(file: File): Promise<boolean> {
  const name = file.name.toLowerCase();
  const isZipName = name.endsWith(".zip");
  const isCsvName = name.endsWith(".csv");
  if (!isZipName && !isCsvName) {
    showToast(t("import_export.invalid_file_type"), "error");
    return false;
  }
  if (file.size > MAX_IMPORT_SIZE) {
    showToast(t("import_export.file_too_large"), "error");
    return false;
  }
  const head = new Uint8Array(await file.slice(0, 4).arrayBuffer());
  const isZipContent = head[0] === 0x50 && head[1] === 0x4b;
  if (isZipContent !== isZipName) {
    showToast(t("import_export.invalid_file_type"), "error");
    return false;
  }
  if (isCsvName) {
    const text = await file.slice(0, 64 * 1024).text();
    if (text.includes("\u0000")) {
      showToast(t("import_export.invalid_file_type"), "error");
      return false;
    }
  }
  return true;
}

function triggerImportCsv(): void {
  const input = importFileInput.value;
  if (!input) {
    return;
  }
  // 清空上次选择，保证重复选择同一文件也触发 change
  input.value = "";
  input.click();
}

async function handleImportFile(event: Event): Promise<void> {
  // 模板 change 事件目标恒为文件输入框（边界断言）
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file || importing.value) {
    return;
  }
  if (!(await validateImportFile(file))) {
    return;
  }
  importing.value = true;
  try {
    const formData = new FormData();
    formData.append("file", file);
    showToast(t("import_export.importing"), "info");
    // 导入 CSV 模块数据（ZIP 内多个 表名.csv，或按表头识别的单个 CSV）
    const result = await apiPost<ImportCsvResponse>("/api/system/import-export/import/csv", formData);
    if (result.success) {
      importResults.value = result.data?.results ?? [];
      if (importResults.value.length > 0) {
        importResultVisible.value = true;
      }
      showToast(t("import_export.import_success"), "success");
    } else {
      showToast(`${t("import_export.import_failed")}: ${result.message}`, "error");
    }
  } finally {
    importing.value = false;
  }
}

async function downloadTemplate(): Promise<void> {
  const result = await apiGet("/api/system/import-export/template?type=all");
  if (!result.success) {
    showToast(`${t("import_export.download_template_failed")}: ${result.message}`, "error");
    return;
  }
  downloadBlobResult(result, "template.zip");
}

async function exportCsvData(): Promise<void> {
  if (exporting.value) {
    return;
  }
  exporting.value = true;
  try {
    // 按模块导出 CSV 数据（ZIP 打包，每张业务表一个 表名.csv）
    const result = await apiGet(
      `/api/system/import-export/export/csv?type=${encodeURIComponent(exportType.value)}`
    );
    if (!result.success) {
      showToast(`${t("import_export.export_failed")}: ${result.message}`, "error");
      return;
    }
    downloadBlobResult(
      result,
      `foims-export-${exportType.value}-${new Date().toISOString().slice(0, 10)}.zip`
    );
  } finally {
    exporting.value = false;
  }
}

async function exportDatabase(): Promise<void> {
  const result = await apiGet("/api/system/import-export/export/database");
  if (!result.success) {
    showToast(`${t("import_export.export_db_failed")}: ${result.message}`, "error");
    return;
  }
  downloadBlobResult(result, `foims_backup_${new Date().toISOString().slice(0, 10)}.sql`);
  showToast(t("import_export.export_db_success"), "success");
}
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="system-config-section">
      <AgentServiceCard :active="props.active" />
      <SnmpTrapCard :active="props.active" />

      <!-- 导入导出 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🗃️</span>
            <span>{{ t("data-management.data_management") }}</span>
          </div>
        </header>
        <div class="data-management-container">
          <section class="section">
            <h3>{{ t("data-management.csv_import") }}</h3>
            <div v-if="!readonly" class="import-actions">
              <button type="button" class="btn btn-primary btn-sm" :disabled="importing" @click="triggerImportCsv">
                {{ t("data-management.import_csv") }}
              </button>
              <button type="button" class="btn btn-secondary btn-sm" @click="downloadTemplate">
                {{ t("data-management.download_template") }}
              </button>
            </div>
            <h3>{{ t("data-management.csv_export") }}</h3>
            <div v-if="!readonly" class="export-actions">
              <select id="export-type" v-model="exportType" class="form-control" aria-label="Select export module">
                <option value="all">{{ t("data-management.module_all") }}</option>
                <option value="organization">{{ t("data-management.module_organization") }}</option>
                <option value="network">{{ t("data-management.module_network") }}</option>
                <option value="room">{{ t("data-management.module_room") }}</option>
                <option value="cabinet">{{ t("data-management.module_cabinet") }}</option>
                <option value="device">{{ t("data-management.module_device") }}</option>
                <option value="cable-link">{{ t("data-management.module_cable_link") }}</option>
                <option value="visualization">{{ t("data-management.module_visualization") }}</option>
              </select>
              <button type="button" class="btn btn-primary btn-sm" :disabled="exporting" @click="exportCsvData">
                {{ t("data-management.export_csv") }}
              </button>
            </div>
          </section>
          <section class="section">
            <h3>{{ t("data-management.db_export") }}</h3>
            <div v-if="!readonly" class="export-actions">
              <button type="button" class="btn btn-primary btn-sm" @click="exportDatabase">
                {{ t("data-management.export_database") }}
              </button>
            </div>
          </section>
        </div>
        <!-- 隐藏文件选择框：导入按钮触发点击 -->
        <input ref="importFileInput" type="file" accept=".zip,.csv" hidden @change="handleImportFile">
      </section>

      <LogsManagementCard :active="props.active" />
    </div>

    <ModalShell
      v-if="importResultVisible"
      :title="t('data-management.import_result')"
      size="lg"
      @close="importResultVisible = false"
    >
      <div class="import-result-list">
        <div v-for="(item, index) in importResults" :key="index" class="import-result-item">
          {{ t("import_export.result_item", { table: item.table || "", inserted: item.inserted ?? 0, updated: item.updated ?? 0 }) }}
        </div>
      </div>
    </ModalShell>
  </div>
</template>
