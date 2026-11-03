<script setup lang="ts">
/**
 * 证书管理卡片：站点根 CA 池 +
 * 自生成/导入证书合并清单。证书用途（nginx / agent，agent 含系统物料）
 * 在生成/导入时声明：仅 nginx 用途提供「应用」（替换进 nginx
 * 配置），agent 用途与系统物料不提供应用、系统物料不可删除。删除直接操作
 * 服务器文件，需确认。CA 生成前需确认（覆盖风险）。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import NavIcon from "../NavIcon.vue";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { CA_DOWNLOAD_URL, applyCertificate, deleteCertificate, getCertificateInventory } from "../../api/system";
import { formatDate } from "../../utils/formatter";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import ConfirmModal from "../ConfirmModal.vue";
import CaGenerateModal from "./CaGenerateModal.vue";
import CaImportModal from "./CaImportModal.vue";
import CertGenerateModal from "./CertGenerateModal.vue";
import CertImportModal from "./CertImportModal.vue";
import type { CaInfo } from "../../types/api/generated/CaInfo";
import type { CertFileInfo } from "../../types/api/generated/CertFileInfo";

/** 证书清单行：文件信息 + 证书类型（自生成 / 导入） */
type CertRow = CertFileInfo & { kind: "generated" | "imported" };

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

const certsQuery = useQuery({
  queryKey: ["certificates"],
  queryFn: async () => {
    const result = await getCertificateInventory();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const caList = computed<CaInfo[]>(() => certsQuery.data.value?.cas ?? []);
// 单行证书记录补齐证书类型（自生成 = 站点 CA 签发，导入 = 外部证书）
function decorateCert(info: CertFileInfo, kind: "generated" | "imported"): CertRow {
  return { ...info, kind };
}

const mergedCerts = computed<CertRow[]>(() => [
  ...(certsQuery.data.value?.generated ?? []).map((info) => decorateCert(info, "generated")),
  ...(certsQuery.data.value?.imported ?? []).map((info) => decorateCert(info, "imported"))
]);

function formatCertValidity(info: CertFileInfo | CaInfo): string {
  if (!info.not_before || !info.not_after) {
    return "-";
  }
  return `${formatDate(info.not_before)} ~ ${formatDate(info.not_after)}`;
}

// 剩余天数样式：过期红 / 30 天内警示；缺省（null/undefined）不加样式
function certDaysClass(days: number | null | undefined): string {
  if (days == null) {
    return "";
  }
  if (days < 0) {
    return "cert-days-expired";
  }
  return days < 30 ? "cert-days-warning" : "";
}

function certDaysText(days: number | null | undefined): string {
  return days == null ? "-" : String(days);
}

const showCaGenerate = ref(false);
const showCaImport = ref(false);
const showCertGenerate = ref(false);
const showCertImport = ref(false);

// 行内操作确认：{ type: "apply" | "delete", item }
const confirmAction = ref<null | { type: "apply" | "delete"; item: CertRow }>(null);

function confirmMessage(): string {
  const action = confirmAction.value;
  if (!action) {
    return "";
  }
  if (action.type === "apply") {
    return t("cert.apply_confirm", { file: action.item.file_stem });
  }
  return t("cert.delete_confirm", { file: action.item.file_stem });
}

async function onActionConfirmed(): Promise<void> {
  const action = confirmAction.value;
  confirmAction.value = null;
  if (!action) {
    return;
  }
  const result =
    action.type === "apply"
      ? await applyCertificate(action.item.kind, action.item.file_stem)
      : await deleteCertificate(action.item.kind, action.item.file_stem);
  if (result.success) {
    showToast(result.message, "success");
    if (action.type === "delete") {
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
    }
  } else {
    showToast(result.message, "error");
  }
}

// CA 生成前确认：覆盖既有站点根 CA 会影响全部终端信任
const showCaGenerateConfirm = ref(false);

function requestCaGenerate(): void {
  showCaGenerateConfirm.value = true;
}

function onCaGenerateConfirmed(): void {
  showCaGenerateConfirm.value = false;
  showCaGenerate.value = true;
}

// CA 下载：CA 证书为公开数据，直接浏览器新窗口下载 PEM
function downloadCa(): void {
  window.open(CA_DOWNLOAD_URL, "_blank");
}
</script>

<template>
  <header class="config-card-header">
    <div class="config-card-title">
      <span class="icon"><NavIcon name="lock" /></span>
      <span>{{ t("cert.manager_title") }}</span>
    </div>
  </header>

  <!-- 站点根 CA 池 -->
  <div class="cert-section">
    <div class="cert-section-header">
      <h4 class="cert-section-title">{{ t("cert.ca_section") }}</h4>
      <div v-if="!readonly" class="card-header-actions">
        <button type="button" class="btn btn-primary btn-sm" @click="requestCaGenerate">
          {{ t("cert.ca_generate_title") }}
        </button>
        <button type="button" class="btn btn-secondary btn-sm" @click="showCaImport = true">
          {{ t("cert.ca_import_title") }}
        </button>
        <button type="button" class="btn btn-secondary btn-sm" @click="downloadCa">
          {{ t("cert.ca_download") }}
        </button>
      </div>
      <div v-else class="card-header-actions">
        <button type="button" class="btn btn-secondary btn-sm" @click="downloadCa">
          {{ t("cert.ca_download") }}
        </button>
      </div>
    </div>
    <div class="table-container cert-table-container">
      <table class="data-table" :aria-label="t('cert.ca_section')">
        <thead>
          <tr>
            <th>{{ t("cert.ca_col_id") }}</th>
            <th class="col-center">{{ t("cert.ca_col_type") }}</th>
            <th>{{ t("cert.col_subject") }}</th>
            <th class="col-center">{{ t("cert.col_validity") }}</th>
            <th class="col-center">{{ t("cert.col_remaining") }}</th>
            <th class="col-center">{{ t("cert.ca_col_key") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="certsQuery.isError.value" class="empty-row">
            <td colspan="6" class="text-center">{{ certsQuery.error.value?.message || t("common.load_failed") }}</td>
          </tr>
          <tr v-else-if="caList.length === 0" class="empty-row">
            <td colspan="6" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="ca in caList" v-else :key="ca.id">
            <td :title="ca.id">{{ ca.id }}</td>
            <td class="col-center">
              <span class="cert-type-badge" :class="ca.source === 'root' ? 'cert-type-generated' : 'cert-type-imported'">
                {{ ca.source === "root" ? t("cert.ca_source_root") : t("cert.ca_source_imported") }}
              </span>
            </td>
            <td>{{ ca.name || "-" }}</td>
            <td class="col-center">{{ formatCertValidity(ca) }}</td>
            <td class="col-center" :class="certDaysClass(ca.days_remaining)">{{ certDaysText(ca.days_remaining) }}</td>
            <td class="col-center">
              <span class="ca-key-badge" :class="ca.has_key ? 'ca-key-ok' : 'ca-key-missing'">
                {{ ca.has_key ? t("cert.ca_key_ok_label") : t("cert.ca_no_key_label") }}
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <!-- 证书清单 -->
  <div class="cert-section">
    <div class="cert-section-header">
      <h4 class="cert-section-title">{{ t("cert.list_section") }}</h4>
      <div class="card-header-actions">
        <template v-if="!readonly">
          <button type="button" class="btn btn-primary btn-sm" @click="showCertGenerate = true">
            {{ t("cert.generate_title") }}
          </button>
          <button type="button" class="btn btn-secondary btn-sm" @click="showCertImport = true">
            {{ t("cert.import_title") }}
          </button>
        </template>
        <button type="button" class="btn btn-secondary btn-sm" @click="certsQuery.refetch()">
          {{ t("common.refresh") }}
        </button>
      </div>
    </div>
    <div class="table-container cert-table-container">
      <table class="data-table" :aria-label="t('cert.list_section')">
        <thead>
          <tr>
            <th>{{ t("cert.col_file") }}</th>
            <th class="col-center">{{ t("cert.col_type") }}</th>
            <th class="col-center">{{ t("cert.col_purpose") }}</th>
            <th>{{ t("cert.col_subject") }}</th>
            <th class="col-center">{{ t("cert.col_validity") }}</th>
            <th class="col-center">{{ t("cert.col_remaining") }}</th>
            <th class="col-center">{{ t("common.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="certsQuery.isError.value" class="empty-row">
            <td colspan="7" class="text-center">{{ certsQuery.error.value?.message || t("common.load_failed") }}</td>
          </tr>
          <tr v-else-if="mergedCerts.length === 0" class="empty-row">
            <td colspan="7" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="info in mergedCerts" v-else :key="`${info.kind}-${info.file_stem}`">
            <td :title="info.cert_filename">{{ info.file_stem }}</td>
            <td class="col-center">
              <span class="cert-type-badge" :class="info.kind === 'generated' ? 'cert-type-generated' : 'cert-type-imported'">
                {{ info.kind === "generated" ? t("cert.type_generated") : t("cert.type_imported") }}
              </span>
            </td>
            <td class="col-center">
              <span class="cert-type-badge" :class="info.system_managed ? 'cert-type-imported' : info.purpose === 'nginx' ? 'cert-type-generated' : 'cert-type-imported'">
                {{ info.system_managed ? t("cert.purpose_system") : info.purpose === "nginx" ? t("cert.purpose_nginx") : t("cert.purpose_agent") }}
              </span>
            </td>
            <td>{{ info.subject_cn || "-" }}</td>
            <td class="col-center">{{ formatCertValidity(info) }}</td>
            <td class="col-center" :class="certDaysClass(info.days_remaining)">{{ certDaysText(info.days_remaining) }}</td>
            <td v-if="!readonly" class="col-center cert-actions-cell">
              <!-- 应用仅对 nginx 用途有意义；agent 用途与系统物料不提供 -->
              <button v-if="info.purpose === 'nginx'" type="button" class="btn btn-primary btn-sm" @click="confirmAction = { type: 'apply', item: info }">
                {{ t("cert.apply_title") }}
              </button>
              <!-- 系统物料由 agent 服务自动签发维护，禁止删除 -->
              <button v-if="!info.system_managed" type="button" class="btn btn-danger btn-sm" @click="confirmAction = { type: 'delete', item: info }">
                {{ t("common.delete") }}
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>

  <!-- CA 生成前确认（覆盖站点根 CA） -->
  <ConfirmModal
    v-if="showCaGenerateConfirm"
    :message="t('cert.ca_generate_confirm')"
    danger
    @confirm="onCaGenerateConfirmed"
    @cancel="showCaGenerateConfirm = false"
  />
  <!-- 证书应用/删除确认 -->
  <ConfirmModal
    v-if="confirmAction"
    :message="confirmMessage()"
    :danger="confirmAction.type === 'delete'"
    @confirm="onActionConfirmed"
    @cancel="confirmAction = null"
  />

  <CaGenerateModal v-if="showCaGenerate" @close="showCaGenerate = false" @saved="certsQuery.refetch()" />
  <CaImportModal v-if="showCaImport" @close="showCaImport = false" @saved="certsQuery.refetch()" />
  <CertGenerateModal v-if="showCertGenerate" :ca-list="caList" @close="showCertGenerate = false" @saved="certsQuery.refetch()" />
  <CertImportModal v-if="showCertImport" @close="showCertImport = false" @saved="certsQuery.refetch()" />
</template>
