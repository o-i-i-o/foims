<script setup>
/**
 * 证书签发弹窗（移植自旧版 cert-generate-modal.html）：签发 CA 下拉
 * （无私钥的 CA 仅供信任导出不可签发，默认选站点根 CA）+ CN/主体/
 * 有效期/SAN。SAN 逗号分隔，空转 null。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQueryClient } from "@tanstack/vue-query";
import { apiPost } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import CertSubjectFields from "./CertSubjectFields.vue";
import ModalShell from "../ModalShell.vue";

const props = defineProps({
  /** 可用 CA 列表（来自 ["certificates"] 查询的 cas 段） */
  caList: { type: Array, default: () => [] }
});

const emit = defineEmits(["close", "saved"]);

const { t } = useI18n();
const queryClient = useQueryClient();

// CA 下拉选项文案：根 CA（自生成）在前由列表序保证；无私钥的 CA 标注不可签发
function caOptionLabel(ca) {
  const source = ca.source === "root" ? t("cert.ca_source_root") : t("cert.ca_source_imported");
  const name = ca.name || ca.id;
  return ca.has_key ? `${name}（${source}）` : `${name}（${source}，${t("cert.ca_no_key")}）`;
}

// 默认选中：根 CA 且有私钥 → 否则第一个有私钥的 CA → 无可用 CA 留空
const defaultCaId = computed(() => {
  const usable = props.caList.filter((ca) => ca.has_key);
  return usable.find((ca) => ca.source === "root")?.id || usable[0]?.id || "";
});

const caId = ref(defaultCaId.value);
const commonName = ref("");
const validityDays = ref("3650");
const subjectAltNames = ref("");
const subject = ref({ organization: "", organizational_unit: "", country: "", state: "", locality: "" });

const formRef = ref(null);
const saving = ref(false);

function optionalField(value) {
  const s = (value || "").trim();
  return s ? s : null;
}

async function handleSubmit() {
  const form = formRef.value;
  if (form && !form.checkValidity()) {
    form.reportValidity();
    return;
  }
  if (saving.value) {
    return;
  }
  const selectedCa = caId.value.trim();
  if (!selectedCa) {
    showToast(t("cert.ca_required"), "warning");
    return;
  }
  const cn = commonName.value.trim();
  if (!cn) {
    showToast(t("cert.common_name_required"), "warning");
    return;
  }
  const sans = subjectAltNames.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const body = {
    common_name: cn,
    organization: optionalField(subject.value.organization),
    organizational_unit: optionalField(subject.value.organizational_unit),
    country: optionalField(subject.value.country),
    state: optionalField(subject.value.state),
    locality: optionalField(subject.value.locality),
    validity_days: Number.parseInt(validityDays.value, 10) || null,
    subject_alt_names: sans.length > 0 ? sans : null,
    ca_id: selectedCa
  };
  saving.value = true;
  try {
    const result = await apiPost("/api/system/certificate/generate", body);
    if (result.success) {
      showToast(result.message, "success");
      queryClient.invalidateQueries({ queryKey: ["certificates"] });
      emit("saved");
      emit("close");
    } else {
      showToast(result.message, "error");
    }
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <ModalShell size="md" @close="emit('close')">
    <template #title>{{ t("cert.generate_title") }}</template>
    <form ref="formRef" @submit.prevent="handleSubmit">
      <div class="form-group">
        <label for="cert-ca-select">{{ t("cert.ca_select") }}<span class="required">*</span></label>
        <select id="cert-ca-select" v-model="caId" required>
          <option v-if="caList.length === 0" value="">{{ t("cert.ca_none") }}</option>
          <option v-for="ca in caList" :key="ca.id" :value="ca.id" :disabled="!ca.has_key">
            {{ caOptionLabel(ca) }}
          </option>
        </select>
      </div>
      <div class="form-group">
        <label for="cert-common-name">{{ t("cert.common_name") }}<span class="required">*</span></label>
        <input id="cert-common-name" v-model="commonName" type="text" required autocomplete="off">
      </div>
      <CertSubjectFields v-model="subject" prefix="cert" />
      <div class="form-group">
        <label for="cert-validity">{{ t("cert.validity") }}<span class="required">*</span></label>
        <input id="cert-validity" v-model="validityDays" type="number" min="1" max="36500" required>
      </div>
      <div class="form-group">
        <label for="cert-san">{{ t("cert.subject_alt_names") }}</label>
        <input id="cert-san" v-model="subjectAltNames" type="text" :placeholder="t('cert.san_placeholder')" autocomplete="off">
      </div>
    </form>
    <template #footer>
      <button type="button" class="btn btn-primary" :disabled="saving" @click="handleSubmit">
        {{ t("cert.generate") }}
      </button>
    </template>
  </ModalShell>
</template>
