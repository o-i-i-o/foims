/**
 * 分页列表与表头排序公共工具（自 ResourcesPage 抽出，供多分区页面复用）：
 * usePagedList 工厂（排序 + 越界回退 + 可变页大小）、跨页连续序号、
 * 表头三态类与点击排序守卫（对应旧版 initSortEvents 的 closest 语义）。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../api/client.js";

export const PAGE_SIZE_OPTIONS = [10, 20, 50, 100];

/**
 * 分页列表封装：默认 created_at 倒序（旧版按名称排序的列表经 defaultSort
 * 指定），排序/页大小切换重置页码；空页且页码越界（末页数据删除后）自动
 * 回退最后一页。
 */
export function usePagedList({
  tag,
  defaultPageSize,
  enabled,
  buildUrl,
  extraKey,
  defaultSortBy = "created_at",
  defaultSortOrder = "desc"
}) {
  const { t } = useI18n();
  const page = ref(1);
  const pageSize = ref(defaultPageSize);
  const sort = reactive({ by: defaultSortBy, order: defaultSortOrder });

  const query = useQuery({
    queryKey: computed(() => [
      tag,
      {
        page: page.value,
        pageSize: pageSize.value,
        sortBy: sort.by,
        sortOrder: sort.order,
        ...extraKey?.value
      }
    ]),
    queryFn: async () => {
      const result = await apiGet(buildUrl(page.value, pageSize.value, sort.by, sort.order));
      if (!result.success) {
        // apiClient 已将失败归一为 { success:false } 不抛异常，
        // 此处显式抛出让查询进入 error 态（isError/error.message 可渲染）
        throw new Error(result.message || t("common.load_failed"));
      }
      return result.data;
    },
    enabled
  });

  watch(
    () => query.data.value,
    (data) => {
      if (!data || !Array.isArray(data.items) || data.items.length !== 0 || page.value <= 1) {
        return;
      }
      const totalPages = data.total_pages || Math.ceil((data.total || 0) / pageSize.value);
      if (totalPages > 0 && page.value > totalPages) {
        page.value = totalPages;
      }
    }
  );

  function toggleSort(key) {
    if (sort.by === key) {
      sort.order = sort.order === "asc" ? "desc" : "asc";
    } else {
      sort.by = key;
      sort.order = "asc";
    }
    page.value = 1;
  }

  function changePageSize(size) {
    pageSize.value = size;
    page.value = 1;
  }

  return reactive({
    page,
    pageSize,
    sort,
    toggleSort,
    changePageSize,
    pageSizeOptions: PAGE_SIZE_OPTIONS,
    items: computed(() => (Array.isArray(query.data.value?.items) ? query.data.value.items : [])),
    total: computed(() => query.data.value?.total ?? 0),
    totalPages: computed(() => query.data.value?.total_pages ?? 1),
    isError: computed(() => query.isError.value),
    errorMessage: computed(() => query.error.value?.message || t("common.load_failed"))
  });
}

/** 序号列：跨页连续（(page-1)*pageSize + 行内序） */
export function seqIndex(list, index) {
  return (list.page - 1) * list.pageSize + index + 1;
}

/** 排序列头公共类绑定（sorted/asc/desc 三态，与 LogsPage 表头一致） */
export function sortClass(list, key) {
  return {
    sorted: list.sort.by === key,
    asc: list.sort.by === key && list.sort.order === "asc",
    desc: list.sort.by === key && list.sort.order === "desc"
  };
}

/**
 * 表头点击排序：表头内嵌过滤输入框、搜索弹层等控件时不触发排序
 * （旧版 initSortEvents 的 closest 守卫语义）
 */
export function onThSort(event, list, key) {
  if (event.target.closest("input, select, textarea, button, a, .th-search-popover")) {
    return;
  }
  list.toggleSort(key);
}
