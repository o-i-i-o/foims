<script setup lang="ts">
/**
 * 通用简单列表弹窗：适合工位/机柜/信息点/机位这类
 * 无复杂交互的只读列表。自动追加序号列，空数据显示 no_data。
 */
import { useI18n } from "vue-i18n";
import ModalShell from "./ModalShell.vue";

defineProps<{
  /** 标题（通常为 "名称 - 列表类型"） */
  title: string;
  /** 数据列定义（不含序号列），label 为已翻译文案 */
  columns: string[];
  /** 行数据，每行为与 columns 等长的字符串数组 */
  rows: string[][];
}>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
</script>

<template>
  <ModalShell :title="title" size="md" @close="emit('close')">
    <div class="table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t("common.index") }}</th>
            <th v-for="column in columns" :key="column">{{ column }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="rows.length === 0">
            <td :colspan="columns.length + 1" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <template v-else>
            <tr v-for="(row, index) in rows" :key="index">
              <td class="index-column">{{ index + 1 }}</td>
              <td v-for="(cell, cellIndex) in row" :key="cellIndex">{{ cell }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </ModalShell>
</template>
