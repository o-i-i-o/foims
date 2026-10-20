<script setup>
/**
 * 证书导入弹窗（移植自旧版 cert-import-modal.html）：证书与私钥必选、
 * 附带 CA 可选（仅供终端导出信任，不参与校验）。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQueryClient } from "@tanstack/vue-query";
import { apiPost } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import ModalShell from "../ModalShell.vue";

const emit = defineEmits(["close", "saved"]);

const { t } = useI18n();
const queryClient = useQueryClient();

const certInput = ref(null);
const keyInput = ref(null);
const caInput = ref(null);
const saving = ref(false);

async function handleSubmit() {
  if (saving.value) {
    return;
  }
  const certFile = certInput.value?.files?.[0];
  const keyFile = keyInput.value?.files?.[0];
  if (!certFile || !keyFile) {
    showToast(t("cert.file_required"), "warning");
    return;
  }
  const fd = new FormData();
  fd.append("cert", certFile);
  fd.append("key", keyFile);
  const caFile = caInput.value?.files?.[0];
  if (caFile) {
    fd.append("ca", caFile);
  }

  saving.value = true;
  try {
    const result = await apiPost("/api/system/certificate/import", fd);
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
  <ModalShell size="sm" @close="emit('close')">
    <template #title>{{ t("cert.import_title") }}</template>
    <form @submit.prevent="handleSubmit">
      <div class="form-group">
        <label for="cert-file">{{ t("cert.cert_file") }}<span class="required">*</span></label>
        <input id="cert-file" ref="certInput" type="file" accept=".pem,.crt" required>
      </div>
      <div class="form-group">
        <label for="key-file">{{ t("cert.key_file") }}<span class="required">*</span></label>
        <input id="key-file" ref="keyInput" type="file" accept=".pem,.key" required>
      </div>
      <div class="form-group">
        <label for="cert-ca-file">{{ t("cert.ca_file") }}</label>
        <input id="cert-ca-file" ref="caInput" type="file" accept=".pem,.crt,.cer">
      </div>
    </form>
    <template #footer>
      <button type="button" class="btn btn-primary" :disabled="saving" @click="handleSubmit">
        {{ t("cert.import") }}
      </button>
    </template>
  </ModalShell>
</template>
