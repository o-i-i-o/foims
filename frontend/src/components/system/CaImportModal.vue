<script setup lang="ts">
/**
 * CA 导入弹窗（移植自旧版 ca-import-modal.html）：证书与私钥两个必选
 * 文件（PEM），FormData multipart 提交。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQueryClient } from "@tanstack/vue-query";
import { apiPost } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

const certInput = ref<HTMLInputElement | null>(null);
const keyInput = ref<HTMLInputElement | null>(null);
const saving = ref(false);

async function handleSubmit(): Promise<void> {
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

  saving.value = true;
  try {
    const result = await apiPost("/api/system/certificate/ca/import", fd);
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
    <template #title>{{ t("cert.ca_import_title") }}</template>
    <form @submit.prevent="handleSubmit">
      <div class="form-group">
        <label for="ca-file">{{ t("cert.ca_cert_file") }}<span class="required">*</span></label>
        <input id="ca-file" ref="certInput" type="file" accept=".pem,.crt,.cer" required>
      </div>
      <div class="form-group">
        <label for="ca-key-file">{{ t("cert.ca_key_file") }}<span class="required">*</span></label>
        <input id="ca-key-file" ref="keyInput" type="file" accept=".pem,.key" required>
      </div>
    </form>
    <template #footer>
      <button type="button" class="btn btn-primary" :disabled="saving" @click="handleSubmit">
        {{ t("cert.ca_import") }}
      </button>
    </template>
  </ModalShell>
</template>
