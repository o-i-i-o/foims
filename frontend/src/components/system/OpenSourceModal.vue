<script setup lang="ts">
/**
 * 开源组件清单弹窗：与 NOTICE 同步维护，数据以 cargo-cyclonedx
 * 生成的 SBOM 为准（见 docs/sbom/）。
 */
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

/** 开源组件分组（按许可证聚合） */
interface OpenSourceGroup {
  license: string;
  components: Array<{ name: string; url: string }>;
}

const OPEN_SOURCE_COMPONENTS: OpenSourceGroup[] = [
  {
    license: "MIT",
    components: [
      { name: "axum / axum-extra / tower / tower-http", url: "https://github.com/tokio-rs/axum" },
      { name: "validator", url: "https://github.com/Keats/validator" },
      { name: "bcrypt", url: "https://github.com/Keats/rust-bcrypt" },
      { name: "jsonwebtoken", url: "https://github.com/Keats/jsonwebtoken" },
      { name: "tracing / tracing-subscriber", url: "https://github.com/tokio-rs/tracing" },
      { name: "tokio", url: "https://github.com/tokio-rs/tokio" },
      { name: "dashmap", url: "https://github.com/xacrimon/dashmap" },
      { name: "zip", url: "https://github.com/zip-rs/zip2" },
      { name: "lettre", url: "https://lettre.rs" },
      { name: "totp-rs", url: "https://github.com/constantoine/totp-rs" },
      { name: "openidconnect", url: "https://github.com/ramosbugs/openidconnect-rs" },
      { name: "pem / flate2 / bytes", url: "" },
      { name: "rust-i18n", url: "https://github.com/longbridge/rust-i18n" }
    ]
  },
  {
    license: "MIT OR Apache-2.0",
    components: [
      { name: "sqlx", url: "https://github.com/launchbadge/sqlx" },
      { name: "serde / serde_json", url: "https://github.com/serde-rs/serde" },
      { name: "regex", url: "https://github.com/rust-lang/regex" },
      {
        name: "RustCrypto: sha2 / aes-gcm / aes / cipher / hex",
        url: "https://github.com/RustCrypto"
      },
      { name: "futures-util / async-trait", url: "https://github.com/rust-lang/futures-rs" },
      { name: "tokio-cron-scheduler", url: "https://github.com/mvniekerk/tokio-cron-scheduler" },
      { name: "ipnetwork", url: "https://github.com/achanda/ipnetwork" },
      { name: "async-snmp", url: "https://github.com/lukeod/async-snmp" },
      { name: "config / toml", url: "https://github.com/rust-cli/config-rs" },
      { name: "thiserror / chrono / time / uuid", url: "" },
      { name: "base64 / rand / arc-swap", url: "" },
      { name: "rustls", url: "https://github.com/rustls/rustls" },
      { name: "ldap3", url: "https://github.com/inejge/ldap3" },
      { name: "rcgen / x509-parser", url: "https://github.com/rustls/rcgen" },
      { name: "http / log", url: "" }
    ]
  },
  {
    license: "Unlicense OR MIT",
    components: [{ name: "csv", url: "https://github.com/BurntSushi/rust-csv" }]
  }
];
</script>

<template>
  <ModalShell :title="t('system.open_source_components')" @close="emit('close')">
    <p class="open-source-intro">{{ t("system.open_source_intro") }}</p>
    <div class="open-source-list">
      <div v-for="group in OPEN_SOURCE_COMPONENTS" :key="group.license" class="open-source-group">
        <h4 class="open-source-license">{{ group.license }}</h4>
        <ul class="open-source-items">
          <li v-for="component in group.components" :key="component.name">
            <a v-if="component.url" :href="component.url" target="_blank" rel="noopener noreferrer">{{ component.name }}</a>
            <template v-else>{{ component.name }}</template>
          </li>
        </ul>
      </div>
    </div>
  </ModalShell>
</template>
