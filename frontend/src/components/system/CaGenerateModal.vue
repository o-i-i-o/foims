<script setup lang="ts">
/**
 * 站点根 CA 生成弹窗：
 * CN 必填 + 主体五字段 + 有效期（默认 7300 天）；可选字段空串转 null。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQueryClient } from "@tanstack/vue-query";
import { generateCa } from "../../api/system";
import { showToast } from "../../composables/useToast";
import type { GenerateCaRequest } from "../../types/api/generated/GenerateCaRequest";
import CertSubjectFields from "./CertSubjectFields.vue";
import ModalShell from "../ModalShell.vue";

/** 证书主体五字段（与 CertSubjectFields 的模型结构一致） */
interface CertSubjectModel {
  organization: string;
  organizational_unit: string;
  country: string;
  state: string;
  locality: string;
}

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

const formRef = ref<HTMLFormElement | null>(null);
const commonName = ref("");
const validityDays = ref("7300");
const subject = ref<CertSubjectModel>({ organization: "", organizational_unit: "", country: "", state: "", locality: "" });

const saving = ref(false);

/** 可选字段：空串转 null（后端区分"未提供"与空值） */
function optionalField(value: string): string | null {
  const s = (value || "").trim();
  return s ? s : null;
}

async function handleSubmit(): Promise<void> {
  const form = formRef.value;
  if (form && !form.checkValidity()) {
    form.reportValidity();
    return;
  }
  if (saving.value) {
    return;
  }
  const body: GenerateCaRequest = {
    common_name: commonName.value.trim(),
    organization: optionalField(subject.value.organization),
    organizational_unit: optionalField(subject.value.organizational_unit),
    country: optionalField(subject.value.country),
    state: optionalField(subject.value.state),
    locality: optionalField(subject.value.locality),
    validity_days: Number.parseInt(validityDays.value, 10) || null
  };
  saving.value = true;
  try {
    const result = await generateCa(body);
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
    <template #title>{{ t("cert.ca_generate_title") }}</template>
    <form ref="formRef" @submit.prevent="handleSubmit">
      <div class="form-group">
        <label for="ca-common-name">{{ t("cert.common_name") }}<span class="required">*</span></label>
        <input id="ca-common-name" v-model="commonName" type="text" required autocomplete="off">
      </div>
      <CertSubjectFields v-model="subject" prefix="ca" />
      <div class="form-group">
        <label for="ca-validity">{{ t("cert.validity") }}<span class="required">*</span></label>
        <input id="ca-validity" v-model="validityDays" type="number" min="1" max="36500" required>
      </div>
    </form>
    <template #footer>
      <button type="button" class="btn-primary" :disabled="saving" @click="handleSubmit">
        {{ t("cert.ca_generate") }}
      </button>
    </template>
  </ModalShell>
</template>
