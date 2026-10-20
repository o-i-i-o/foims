<script setup>
/**
 * 线缆标签批量打印弹窗（移植自旧版 cableLink.js openCableLabelPrintModal）：
 * 列出当前页线路（A/B 端编号），默认全选，勾选计数实时显示
 * （cable_link.print_count：N 条线路 / 2N 枚标签）；确认后经 CablePrintArea
 * 一次性打印所选线路的两端标签。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { showToast } from "../../composables/useToast.js";
import ModalShell from "../ModalShell.vue";
import CablePrintArea from "./CablePrintArea.vue";

const props = defineProps({
  /** 当前页线路列表（直接取列表数据，避免二次请求） */
  items: { type: Array, required: true }
});

const emit = defineEmits(["close"]);

const { t } = useI18n();

// 默认全选（与旧版一致）
const checked = ref(props.items.map(() => true));
const selectAll = ref(true);
const printing = ref(false);

const checkedCount = computed(() => checked.value.filter(Boolean).length);

function onSelectAll() {
  checked.value = checked.value.map(() => selectAll.value);
}

function onChecked(index, event) {
  checked.value[index] = event.target.checked;
}

/** 打印所选：无勾选提示，否则调起打印并在结束后关闭弹窗 */
function printSelected() {
  if (checkedCount.value === 0) {
    showToast(t("cable_link.print_none_selected"), "warning");
    return;
  }
  printing.value = true;
}

function selectedItems() {
  return props.items.filter((_, index) => checked.value[index]);
}
</script>

<template>
  <ModalShell :title="t('cable_link.print_labels')" @close="emit('close')">
    <form @submit.prevent="printSelected">
      <div class="form-group cable-label-select-all">
        <label class="checkbox-label">
          <input v-model="selectAll" type="checkbox" @change="onSelectAll" />
          <span>{{ t("cable_link.print_select_all") }}</span>
        </label>
        <span class="hint-text">
          {{ t("cable_link.print_count", { cables: checkedCount, labels: checkedCount * 2 }) }}
        </span>
      </div>
      <div class="table-container cable-label-print-table-container">
        <table class="data-table">
          <thead>
            <tr>
              <!-- 空表头为勾选列：用视觉隐藏文本提供可访问名称 -->
              <th class="col-center">
                <span class="sr-only">{{ t("cable_link.print_select_all") }}</span>
              </th>
              <th>{{ t("cable_link.a_endpoint_no") }}</th>
              <th>{{ t("cable_link.b_endpoint_no") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, index) in props.items" :key="item.id">
              <td class="col-center">
                <input
                  type="checkbox"
                  :checked="checked[index]"
                  @change="onChecked(index, $event)"
                />
              </td>
              <td>{{ item.a_endpoint_label || "-" }}</td>
              <td>{{ item.b_endpoint_label || "-" }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </form>
    <template #footer>
      <button type="button" class="btn btn-primary" @click="printSelected">
        {{ t("cable_link.print") }}
      </button>
    </template>

    <!-- 打印区（Teleport 到 body，打印结束后关闭弹窗） -->
    <CablePrintArea v-if="printing" :items="selectedItems()" @done="emit('close')" />
  </ModalShell>
</template>
