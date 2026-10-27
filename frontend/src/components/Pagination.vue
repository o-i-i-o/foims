<script setup lang="ts">
/**
 * 通用分页控件：
 * 上一页/下一页 + 页码省略序列 + 总数信息，总页数 > 7 时出现跳页框。
 * 根元素为 .pagination-container，调用方置于 .table-container 之后
 * （布局层 sticky 页脚规则依赖该相邻结构）。
 * 传入 pageSize（>0）时在总数信息后渲染每页条数选择器
 * （每页条数切换后由调用方重置页码重拉）。
 * 悬停翻页按钮/页码时 emit("hover-page") 上抛目标页，
 * 调用方接 usePagedList.prefetch 实现预取（不接则无预取行为）。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

const props = withDefaults(
  defineProps<{
    page: number;
    totalPages: number;
    total?: number;
    /** 每页条数：>0 时渲染选择器并回显当前值（默认选项 10/20/50/100） */
    pageSize?: number;
    pageSizeOptions?: number[];
  }>(),
  { total: 0, pageSize: 0, pageSizeOptions: () => [10, 20, 50, 100] }
);

const emit = defineEmits<{
  "page-change": [target: number];
  "page-size-change": [size: number];
  /** 悬停预取：上抛悬停的目标页（调用方接 usePagedList.prefetch） */
  "hover-page": [target: number];
}>();

const { t } = useI18n();

// 页码序列：null 表示省略号
const pages = computed<(number | null)[]>(() => {
  const { page, totalPages } = props;
  if (totalPages <= 7) {
    return Array.from({ length: totalPages }, (_, i) => i + 1);
  }
  const delta = 2;
  const list: Array<number | null> = [1];
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
// 输入框运行时值可能为字符串（number 输入框解析失败回退原文），故为宽类型
const jumpPage = ref<number | string>(props.page);

// 翻页后同步跳页框回显值
watch(
  () => props.page,
  (value) => {
    jumpPage.value = value;
  }
);

function go(target: number) {
  if (target >= 1 && target <= props.totalPages && target !== props.page) {
    emit("page-change", target);
  }
}

/** 悬停预取：hover 翻页按钮/页码时上抛目标页（当前页与越界目标不触发） */
function hoverPrefetch(target: number) {
  if (target >= 1 && target <= props.totalPages && target !== props.page) {
    emit("hover-page", target);
  }
}

function doJump() {
  const target = Number.parseInt(String(jumpPage.value), 10);
  if (Number.isFinite(target)) {
    go(target);
  }
}

/** 每页条数下拉 change：从事件目标读取选中值上抛 */
function onPageSizeChange(event: Event) {
  const target = event.target;
  if (target instanceof HTMLSelectElement) {
    emit("page-size-change", Number(target.value));
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
            @mouseenter="hoverPrefetch(page - 1)"
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
                @mouseenter="hoverPrefetch(item)"
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
            @mouseenter="hoverPrefetch(page + 1)"
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
          <span v-if="pageSize > 0" class="pagination-size">
            <select
              class="pagination-size-selector"
              :value="pageSize"
              :title="t('common.page_size')"
              @change="onPageSizeChange"
            >
              <option v-for="size in pageSizeOptions" :key="size" :value="size">{{ size }} / {{ t("common.page") }}</option>
            </select>
          </span>
        </div>
      </div>
    </div>
  </div>
</template>
