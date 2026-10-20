<script setup>
/**
 * 证书主体五字段（CA 生成 / 证书签发弹窗共用）：defineModel 双绑
 * { organization, organizational_unit, country, state, locality }；
 * country 限 2 字符国家码。prefix 用于生成各弹窗内唯一的输入框 id。
 */
import { useI18n } from "vue-i18n";

const { t } = useI18n();

const model = defineModel({ type: Object, required: true });

defineProps({
  /** 输入框 id 前缀（ca / cert），避免两个弹窗同页 id 冲突 */
  prefix: { type: String, required: true }
});
</script>

<template>
  <div class="form-group">
    <label :for="`${prefix}-organization`">{{ t("cert.organization") }}</label>
    <input
      :id="`${prefix}-organization`"
      v-model="model.organization"
      type="text"
      :placeholder="t('cert.organization_placeholder')"
      autocomplete="off"
    >
  </div>
  <div class="form-group">
    <label :for="`${prefix}-organizational-unit`">{{ t("cert.organizational_unit") }}</label>
    <input
      :id="`${prefix}-organizational-unit`"
      v-model="model.organizational_unit"
      type="text"
      :placeholder="t('cert.ou_placeholder')"
      autocomplete="off"
    >
  </div>
  <div class="form-group">
    <label :for="`${prefix}-country`">{{ t("cert.country") }}</label>
    <input
      :id="`${prefix}-country`"
      v-model="model.country"
      type="text"
      :placeholder="t('cert.country_placeholder')"
      maxlength="2"
      autocomplete="off"
    >
  </div>
  <div class="form-group">
    <label :for="`${prefix}-state`">{{ t("cert.state") }}</label>
    <input :id="`${prefix}-state`" v-model="model.state" type="text" :placeholder="t('cert.state_placeholder')" autocomplete="off">
  </div>
  <div class="form-group">
    <label :for="`${prefix}-locality`">{{ t("cert.locality") }}</label>
    <input :id="`${prefix}-locality`" v-model="model.locality" type="text" :placeholder="t('cert.locality_placeholder')" autocomplete="off">
  </div>
</template>
