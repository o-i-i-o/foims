<script setup lang="ts">
/**
 * 用户管理页签（移植自旧版 main.html users 分区 + modules/userManager.js）：
 * 分页列表（8 列 + 表头排序）+ 新增/编辑（两步弹窗含 2FA 管理）+ 删除确认。
 * 列表与弹窗数据源共享 ["users"] 查询键；auditor/user 只读隐藏写控件。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQueryClient } from "@tanstack/vue-query";
import { apiDelete } from "../../api/client";
import { formatDateTime } from "../../utils/formatter";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import { onThSort, seqIndex, sortClass, usePagedList } from "../../composables/usePagedList";
import ConfirmModal from "../ConfirmModal.vue";
import NavIcon from "../NavIcon.vue";
import Pagination from "../Pagination.vue";
import UserModal from "./UserModal.vue";
import type { User } from "../../types/api/generated/User";

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

const list = usePagedList<User>({
  tag: "users",
  defaultPageSize: 20,
  enabled: computed(() => props.active),
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/users?page=${page}&page_size=${pageSize}&sort_by=${sortBy}&sort_order=${sortOrder}`
});

// 角色显示（三权分立 + 超管）
function roleLabel(role: string): string {
  const keys: Record<string, string> = {
    admin: "user.role_admin",
    sysadmin: "user.role_sysadmin",
    secadmin: "user.role_secadmin",
    auditor: "user.role_auditor"
  };
  return t(keys[role] || "user.role_user");
}

// ==================== 新增/编辑弹窗 ====================

const modalUser = ref<User | null>(null);
const showUserModal = ref(false);

function openCreate(): void {
  modalUser.value = null;
  showUserModal.value = true;
}

function openEdit(user: User): void {
  modalUser.value = user;
  showUserModal.value = true;
}

function handleModalSaved(): void {
  queryClient.invalidateQueries({ queryKey: ["users"] });
}

// ==================== 删除（二次确认） ====================

const deleteTarget = ref<User | null>(null);
const deleting = ref(false);

function handleDeleteConfirmed(): void {
  const id = deleteTarget.value?.id;
  deleteTarget.value = null;
  if (id != null) {
    void doDelete(id);
  }
}

async function doDelete(id: string): Promise<void> {
  if (deleting.value) {
    return;
  }
  deleting.value = true;
  try {
    const result = await apiDelete(`/api/users/${id}`);
    if (result.success) {
      showToast(t("user.delete_user") + t("common.success"), "success");
    } else {
      showToast(`${t("user.delete_user") + t("common.failed")}：${result.message}`, "error");
    }
  } finally {
    deleting.value = false;
  }
  queryClient.invalidateQueries({ queryKey: ["users"] });
}
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="table-container">
      <table class="data-table" :aria-label="t('user.list')">
        <thead>
          <tr>
            <th class="index-column">{{ t("common.index") }}</th>
            <th :class="sortClass(list, 'username')" @click="onThSort($event, list, 'username')">{{ t("auth.username") }}</th>
            <th :class="sortClass(list, 'email')" @click="onThSort($event, list, 'email')">{{ t("user.email") }}</th>
            <th class="col-center" :class="sortClass(list, 'role')" @click="onThSort($event, list, 'role')">{{ t("user.role") }}</th>
            <th class="col-center" :class="sortClass(list, 'enabled')" @click="onThSort($event, list, 'enabled')">{{ t("user.status") }}</th>
            <th class="col-center" :class="sortClass(list, 'two_factor_enabled')" @click="onThSort($event, list, 'two_factor_enabled')">2FA</th>
            <th class="col-center" :class="sortClass(list, 'created_at')" @click="onThSort($event, list, 'created_at')">{{ t("common.created_at") }}</th>
            <th>
              <button v-if="!readonly" type="button" class="btn btn-primary btn-sm" @click="openCreate">
                {{ t("user.add_user") }}
              </button>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="list.isError" class="empty-row">
            <td colspan="8" class="text-center">{{ list.errorMessage }}</td>
          </tr>
          <tr v-else-if="list.items.length === 0" class="empty-row">
            <td colspan="8" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="(user, index) in list.items" v-else :key="user.id">
            <td class="index-column">{{ seqIndex(list, index) }}</td>
            <td>{{ user.username }}</td>
            <td>{{ user.email }}</td>
            <td class="col-center">{{ roleLabel(user.role) }}</td>
            <td class="col-center">
              <span class="status-badge" :class="user.enabled ? 'status-active' : 'status-inactive'">
                {{ user.enabled ? t("user.status_enabled") : t("user.status_disabled") }}
              </span>
            </td>
            <td class="col-center">
              <span class="two-factor-badge" :class="user.two_factor_enabled ? 'two-factor-enabled' : 'two-factor-disabled'">
                {{ user.two_factor_enabled ? t("user.two_factor_enabled") : t("user.two_factor_disabled") }}
              </span>
            </td>
            <td class="col-center">{{ formatDateTime(user.created_at) }}</td>
            <td v-if="!readonly" class="col-center">
              <button
                type="button"
                class="icon-btn btn-edit"
                :data-tooltip="t('common.edit')"
                :aria-label="t('common.edit')"
                @click="openEdit(user)"
              >
                <NavIcon name="edit" />
              </button>
              <button
                type="button"
                class="icon-btn btn-danger btn-delete"
                :data-tooltip="t('common.delete')"
                :aria-label="t('common.delete')"
                @click="deleteTarget = user"
              >
                <NavIcon name="trash" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <Pagination
      :page="list.page"
      :total-pages="list.totalPages"
      :total="list.total"
      @page-change="(p) => (list.page = p)"
      @page-size-change="(s) => list.changePageSize(s)"
    />

    <UserModal v-if="showUserModal" :user="modalUser" @close="showUserModal = false" @saved="handleModalSaved" />
    <ConfirmModal
      v-if="deleteTarget"
      :message="t('user.delete_confirm')"
      danger
      @confirm="handleDeleteConfirmed"
      @cancel="deleteTarget = null"
    />
  </div>
</template>
