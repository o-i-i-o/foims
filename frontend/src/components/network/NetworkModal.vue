<script setup lang="ts">
/**
 * 网段新增/编辑弹窗：
 * 名称/区域/双栈 CIDR/网关/DNS/描述；网关校验（格式合法、与地址族一致、
 * 必须落在对应 CIDR 网段内），DNS 以逗号或空白分隔且最多 5 条。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { isIpInCidr, isValidIPv4, isValidIPv6 } from "../../utils/network";
import { queryKeys } from "../../utils/queryKeys";
import ModalShell from "../ModalShell.vue";
import type { Paged } from "../../types/api";
import type { Network } from "../../types/api/generated/Network";
import type { NetworkCreate } from "../../types/api/generated/NetworkCreate";
import type { NetworkRegion } from "../../types/api/generated/NetworkRegion";

// null = 新增；对象 = 编辑回显
const props = withDefaults(defineProps<{ network?: Network | null }>(), { network: null });

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

/** 表单状态：DNS 以逗号/空白分隔文本编辑，提交时解析为数组 */
interface NetworkFormState {
  name: string;
  regionId: string;
  ipv4Cidr: string;
  ipv6Cidr: string;
  ipv4Gateway: string;
  ipv6Gateway: string;
  ipv4Dns: string;
  ipv6Dns: string;
  description: string;
}

/** DNS 数组回显为文本：未配置（null/非数组）返回空串 */
function joinDns(dns: string[] | null | undefined): string {
  return Array.isArray(dns) ? dns.join(", ") : "";
}

const form = reactive<NetworkFormState>({
  name: props.network?.name ?? "",
  regionId: props.network?.network_region_id ?? "",
  ipv4Cidr: props.network?.ipv4_cidr ?? "",
  ipv6Cidr: props.network?.ipv6_cidr ?? "",
  ipv4Gateway: props.network?.ipv4_gateway ?? "",
  ipv6Gateway: props.network?.ipv6_gateway ?? "",
  ipv4Dns: joinDns(props.network?.ipv4_dns),
  ipv6Dns: joinDns(props.network?.ipv6_dns),
  description: props.network?.description ?? ""
});

// 区域下拉选项：与区域列表共享失效前缀，弹窗打开时才加载
const regionsQuery = useQuery({
  queryKey: ["network-regions", "options"],
  queryFn: async (): Promise<Paged<NetworkRegion> | null> => {
    const result = await apiGet<Paged<NetworkRegion>>(
      "/api/resources/network-regions?page_size=1000"
    );
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const regionOptions = computed<NetworkRegion[]>(() => regionsQuery.data.value?.items ?? []);

const saving = ref(false);

/** DNS 文本解析：逗号/空白分隔，超 5 条提示并返回 null */
function parseDnsList(dnsStr: string): string[] | null {
  if (!dnsStr?.trim()) {
    return null;
  }
  const dnsList = dnsStr
    .split(/[,\s]+/)
    .map((dns) => dns.trim())
    .filter((dns) => dns.length > 0);
  if (dnsList.length > 5) {
    showToast(t("network.dns_limit"), "warning");
    return null;
  }
  return dnsList;
}

async function submit() {
  const name = form.name.trim();
  if (!name) {
    showToast(t("network.name_required"), "warning");
    return;
  }
  if (!form.regionId) {
    showToast(t("network.region_required"), "warning");
    return;
  }
  if (!form.ipv4Cidr.trim() && !form.ipv6Cidr.trim()) {
    showToast(t("network.cidr_required"), "warning");
    return;
  }

  // 网关校验：格式合法、与地址族一致，且必须落在对应 CIDR 网段内
  const gatewayChecks: {
    family: string;
    gateway: string;
    cidr: string;
    isValid: (ip: string) => boolean;
  }[] = [
    { family: "4", gateway: form.ipv4Gateway, cidr: form.ipv4Cidr, isValid: isValidIPv4 },
    { family: "6", gateway: form.ipv6Gateway, cidr: form.ipv6Cidr, isValid: isValidIPv6 }
  ];
  for (const { family, gateway, cidr, isValid } of gatewayChecks) {
    const gatewayTrimmed = gateway ? gateway.trim() : "";
    if (!gatewayTrimmed) {
      continue;
    }
    if (!isValid(gatewayTrimmed)) {
      showToast(t("network.gateway_invalid", { family }), "warning");
      return;
    }
    if (!cidr?.trim()) {
      showToast(t("network.gateway_requires_cidr", { family }), "warning");
      return;
    }
    if (!isIpInCidr(gatewayTrimmed, cidr.trim())) {
      showToast(t("network.gateway_not_in_cidr", { family }), "warning");
      return;
    }
  }

  const ipv4Dns = parseDnsList(form.ipv4Dns);
  const ipv6Dns = parseDnsList(form.ipv6Dns);
  // parseDnsList 超限时返回 null 且已提示；原文非空时中止提交
  if (ipv4Dns === null && form.ipv4Dns && form.ipv4Dns.trim()) {
    return;
  }
  if (ipv6Dns === null && form.ipv6Dns && form.ipv6Dns.trim()) {
    return;
  }

  saving.value = true;
  // 覆盖型保存：未填项显式 null（与原实现键集一致）
  const body: NetworkCreate = {
    name,
    network_region_id: form.regionId,
    ipv4_cidr: form.ipv4Cidr.trim() || null,
    ipv6_cidr: form.ipv6Cidr.trim() || null,
    ipv4_gateway: form.ipv4Gateway.trim() || null,
    ipv6_gateway: form.ipv6Gateway.trim() || null,
    ipv4_dns: ipv4Dns,
    ipv6_dns: ipv6Dns,
    description: form.description || null
  };
  const result = props.network
    ? await apiPut(`/api/resources/networks/${props.network.id}`, body)
    : await apiPost("/api/resources/networks", body);
  saving.value = false;

  if (!result.success) {
    showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    return;
  }
  queryClient.invalidateQueries({ queryKey: queryKeys.networks });
  showToast(t("network.save_success"), "success");
  emit("saved");
  emit("close");
}
</script>

<template>
  <ModalShell :title="props.network ? t('network.edit_network') : t('network.add_network')" size="md" @close="emit('close')">
    <form @submit.prevent="submit">
      <div class="form-row">
        <div class="form-group">
          <label for="network-name">
            <span>{{ t("network.name") }}</span><span class="required">*</span>
          </label>
          <input id="network-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group">
          <label for="network-region">
            <span>{{ t("network.region") }}</span><span class="required">*</span>
          </label>
          <select id="network-region" v-model="form.regionId" required>
            <option v-if="regionOptions.length === 0" value="">
              {{ regionsQuery.isError.value ? t("common.network_region") : t("network.add_region_first") }}
            </option>
            <option v-for="option in regionOptions" :key="option.id" :value="option.id">
              {{ option.name }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="network-ipv4-cidr">{{ t("network.ipv4_cidr") }}</label>
          <input id="network-ipv4-cidr" v-model="form.ipv4Cidr" type="text" placeholder="e.g., 192.168.1.0/24">
        </div>
        <div class="form-group">
          <label for="network-ipv6-cidr">{{ t("network.ipv6_cidr") }}</label>
          <input id="network-ipv6-cidr" v-model="form.ipv6Cidr" type="text" placeholder="e.g., 2001:db8::/32">
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="network-ipv4-gateway">{{ t("network.ipv4_gateway") }}</label>
          <input id="network-ipv4-gateway" v-model="form.ipv4Gateway" type="text" placeholder="e.g., 192.168.1.1">
        </div>
        <div class="form-group">
          <label for="network-ipv6-gateway">{{ t("network.ipv6_gateway") }}</label>
          <input id="network-ipv6-gateway" v-model="form.ipv6Gateway" type="text" placeholder="e.g., 2001:db8::1">
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="network-ipv4-dns">{{ t("network.ipv4_dns") }}</label>
          <input id="network-ipv4-dns" v-model="form.ipv4Dns" type="text" placeholder="e.g., 8.8.8.8, 8.8.4.4">
        </div>
        <div class="form-group">
          <label for="network-ipv6-dns">{{ t("network.ipv6_dns") }}</label>
          <input id="network-ipv6-dns" v-model="form.ipv6Dns" type="text" placeholder="e.g., 2001:4860:4860::8888, 2001:4860:4860::8844">
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
      <button type="button" class="btn-primary" :disabled="saving" @click="submit">
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
