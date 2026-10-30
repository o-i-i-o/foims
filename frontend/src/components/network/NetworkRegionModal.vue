<script setup lang="ts">
/**
 * 网络区域新增/编辑弹窗：
 * 名称 + IPv4/IPv6 CIDR 动态行（至少保留一行，末行删除改为清空）+ 描述。
 * CIDR 动态行以响应式数组维护。
 */
import { reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQueryClient } from "@tanstack/vue-query";
import { apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { queryKeys } from "../../utils/queryKeys";
import ModalShell from "../ModalShell.vue";
import type { NetworkRegion } from "../../types/api/generated/NetworkRegion";
import type { NetworkRegionCreate } from "../../types/api/generated/NetworkRegionCreate";

// null = 新增；对象 = 编辑回显
const props = withDefaults(defineProps<{ region?: NetworkRegion | null }>(), { region: null });

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

/** 表单基础字段：CIDR 动态行单独维护 */
interface RegionFormState {
  name: string;
  description: string;
}

const form = reactive<RegionFormState>({
  name: props.region?.name ?? "",
  description: props.region?.description ?? ""
});

// CIDR 动态行：初始为已存值（无则一个空行）
const ipv4Cidrs = ref<string[]>(
  props.region?.ipv4_cidrs?.length ? [...props.region.ipv4_cidrs] : [""]
);
const ipv6Cidrs = ref<string[]>(
  props.region?.ipv6_cidrs?.length ? [...props.region.ipv6_cidrs] : [""]
);

function addRow(list: string[]): void {
  list.push("");
}

/**
 * 删除一行：多行时移除；仅剩末行时清空输入，
 * 行数不减保证始终有输入位
 */
function removeRow(list: string[], index: number): void {
  if (list.length > 1) {
    list.splice(index, 1);
  } else {
    list[0] = "";
  }
}

/** 收集 CIDR 值：去空格、滤空串，全空返回 null */
function collectCidrs(list: string[]): string[] | null {
  const values = list.map((v) => v.trim()).filter((v) => v.length > 0);
  return values.length > 0 ? values : null;
}

const saveMutation = useMutation({
  mutationFn: () => {
    // 新增/编辑共用键集：编辑端 NetworkRegionUpdate 键均可空，形状兼容 Create
    const body: NetworkRegionCreate = {
      name: form.name.trim(),
      description: form.description || null,
      ipv4_cidrs: collectCidrs(ipv4Cidrs.value),
      ipv6_cidrs: collectCidrs(ipv6Cidrs.value)
    };
    return props.region
      ? apiPut(`/api/resources/network-regions/${props.region.id}`, body)
      : apiPost("/api/resources/network-regions", body);
  },
  onSuccess: (result) => {
    if (!result.success) {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
      return;
    }
    // 区域列表与网段弹窗的区域下拉共享该缓存，前缀失效一并刷新
    queryClient.invalidateQueries({ queryKey: queryKeys.networkRegions });
    showToast(t("network.region_save_success"), "success");
    emit("saved");
    emit("close");
  }
});

function submit(): void {
  if (!form.name.trim()) {
    showToast(t("network.region_name_required"), "warning");
    return;
  }
  saveMutation.mutate();
}
</script>

<template>
  <ModalShell :title="props.region ? t('network.edit_region') : t('network.add_region')" @close="emit('close')">
    <form @submit.prevent="submit">
      <div class="form-row">
        <div class="form-group">
          <label for="network-region-name">
            <span>{{ t("common.name") }}</span><span class="required">*</span>
          </label>
          <input id="network-region-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group cidr-group">
          <span class="group-label">{{ t("network.ipv4_cidrs") }}</span>
          <div class="cidr-input-list">
            <div v-for="(_, index) in ipv4Cidrs" :key="`v4-${index}`" class="cidr-input-row">
              <input v-model="ipv4Cidrs[index]" type="text" class="cidr-input" :placeholder="t('network.cidr_ipv4_example')">
              <div class="cidr-btn-group">
                <button type="button" class="btn btn-icon-sm btn-success cidr-add-btn" :aria-label="t('common.add')" @click="addRow(ipv4Cidrs)">+</button>
                <button type="button" class="btn btn-icon-sm btn-danger cidr-remove-btn" :aria-label="t('common.delete')" @click="removeRow(ipv4Cidrs, index)">×</button>
              </div>
            </div>
          </div>
        </div>
        <div class="form-group cidr-group">
          <span class="group-label">{{ t("network.ipv6_cidrs") }}</span>
          <div class="cidr-input-list">
            <div v-for="(_, index) in ipv6Cidrs" :key="`v6-${index}`" class="cidr-input-row">
              <input v-model="ipv6Cidrs[index]" type="text" class="cidr-input" :placeholder="t('network.cidr_ipv6_example')">
              <div class="cidr-btn-group">
                <button type="button" class="btn btn-icon-sm btn-success cidr-add-btn" :aria-label="t('common.add')" @click="addRow(ipv6Cidrs)">+</button>
                <button type="button" class="btn btn-icon-sm btn-danger cidr-remove-btn" :aria-label="t('common.delete')" @click="removeRow(ipv6Cidrs, index)">×</button>
              </div>
            </div>
          </div>
        </div>
      </div>
      <div class="form-group form-group-block">
        <textarea
          v-model="form.description"
          :aria-label="t('common.description')"
          :placeholder="t('common.description_placeholder')"
        />
      </div>
    </form>

    <template #footer>
      <button type="button" class="btn-primary" :disabled="saveMutation.isPending.value" @click="submit">
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
