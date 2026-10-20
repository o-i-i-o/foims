<script setup>
/**
 * 通用分页控件（移植自旧版 utils/pagination.js renderPagination）：
 * 上一页/下一页 + 页码省略序列 + 总数信息，总页数 > 7 时出现跳页框。
 * 根元素为 .pagination-container，调用方置于 .table-container 之后
 * （布局层 sticky 页脚规则依赖该相邻结构）。
 * 未实现每页条数选择器：日志等现有页面页大小固定，待出现可变页大小页面时补充。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

const props = defineProps({
  page: { type: Number, required: true },
  totalPages: { type: Number, required: true },
  total: { type: Number, default: 0 }
});

const emit = defineEmits(["page-change"]);

const { t } = useI18n();

// 页码序列：null 表示省略号（与旧版 generatePageNumbers 一致）
const pages = computed(() => {
  const { page, totalPages } = props;
  if (totalPages <= 7) {
    return Array.from({ length: totalPages }, (_, i) => i + 1);
  }
  const delta = 2;
  const list = [1];
  if (page > delta + 1) {
    list.push(null);
  }
  const start = Math.max(2, page - delta);
  const end = Math.min(totalPages - 1, page + delta);
  for (let i = start; i <= end; i++) {
    list.push(i);
  }
  if (page < totalPages - delta - 1) {
    list.push(null);
  }
  list.push(totalPages);
  return list;
});

const showJump = computed(() => props.totalPages > 7);
const jumpPage = ref(props.page);

// 翻页后同步跳页框回显值
watch(
  () => props.page,
  (value) => {
    jumpPage.value = value;
  }
);

function go(target) {
  if (target >= 1 && target <= props.totalPages && target !== props.page) {
    emit("page-change", target);
  }
}

function doJump() {
  const target = Number.parseInt(jumpPage.value, 10);
  if (Number.isFinite(target)) {
    go(target);
  }
}

const infoText = computed(() =>
  t("common.page_info", {
    total: props.total || 0,
    page: props.page,
    total_pages: props.totalPages
  })
);
</script>

<template>
  <div class="pagination-container">
    <div class="pagination-wrapper">
      <div class="pagination">
        <div class="pagination-left">
          <button
            type="button"
            class="pagination-btn pagination-prev"
            :disabled="page <= 1"
            @click="go(page - 1)"
          >
            {{ t("common.prev_page") }}
          </button>
          <div class="pagination-pages">
            <template v-for="(item, index) in pages" :key="index">
              <span v-if="item === null" class="pagination-ellipsis">...</span>
              <button
                v-else
                type="button"
                class="pagination-page"
                :class="{ active: item === page }"
                @click="go(item)"
              >
                {{ item }}
              </button>
            </template>
          </div>
          <button
            type="button"
            class="pagination-btn pagination-next"
            :disabled="page >= totalPages"
            @click="go(page + 1)"
          >
            {{ t("common.next_page") }}
          </button>
          <span v-if="showJump" class="pagination-jump">
            <span class="pagination-jump-label">{{ t("common.goto") }}</span>
            <input
              v-model="jumpPage"
              type="number"
              class="pagination-jump-input"
              min="1"
              :max="totalPages"
              @keydown.enter.prevent="doJump"
            >
            <span class="pagination-jump-label">{{ t("common.page") }}</span>
            <button type="button" class="pagination-btn pagination-jump-btn" @click="doJump">
              {{ t("common.confirm") }}
            </button>
          </span>
        </div>
        <div class="pagination-right">
          <span class="pagination-info">{{ infoText }}</span>
        </div>
      </div>
    </div>
  </div>
</template>
