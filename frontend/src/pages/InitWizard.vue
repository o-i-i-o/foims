<script setup>
/**
 * 初始化向导（移植自旧版 web/static/js/modules/foims_init/ + init_index.html）：
 * 五步流程 PostgreSQL 检查 → 数据库配置 → 数据库初始化 → 管理员账户 → 完成。
 *
 * 网络层复用 apiClient（/api/init/* 与 /api/auth/init-status 均为公共端点，
 * 永不 reject，message 已就地翻译）。契约与旧版一致：
 * - /api/init/check-pgsql 返回顶层 { installed, running, message, error }
 * - 其余端点为 { success, message, data } 标准包裹
 * - 旧版 escapeHtml/innerHTML 动态渲染改为模板插值（自动转义）
 * - 旧版「重新检查」整页 reload 改为重跑检查请求
 */
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../api/client.js";
import { changeLanguage, i18n } from "../i18n/index.js";
import { showToast } from "../composables/useToast.js";
import AppToast from "../components/AppToast.vue";

const { t } = useI18n();

// ==================== 流程状态 ====================
const currentStep = ref(1);
const initMode = ref("create");

function goToStep(stepNum) {
  currentStep.value = stepNum;
}

// ==================== 全局反馈 ====================
// 加载遮罩：引用计数支持并发请求各自开关
const loadingCount = ref(0);
const showLoading = () => {
  loadingCount.value += 1;
};
const hideLoading = () => {
  loadingCount.value = Math.max(0, loadingCount.value - 1);
};

// 页内错误条：5 秒自动隐藏，新提示重置计时（与旧版 showError 语义一致）
const errorMessage = ref("");
const errorVisible = ref(false);
let errorHideTimer = null;
function showError(message) {
  errorMessage.value = message;
  errorVisible.value = true;
  if (errorHideTimer) {
    clearTimeout(errorHideTimer);
  }
  errorHideTimer = setTimeout(() => {
    errorVisible.value = false;
    errorHideTimer = null;
  }, 5000);
}

// 各检查步骤共用的兜底错误文案键（与旧版 T_KEY_* 一致）
const unknownError = computed(() => t("init.unknown_error"));
const networkError = computed(() => t("init.network_error"));

/**
 * 翻译服务端返回的 error 字段（后端约定为 i18n key）。
 * 键在字典中缺失时 t() 原样返回 key，此时回退到通用未知错误文案，
 * 避免向用户展示裸键名。
 */
function translateErrorField(errorKey) {
  if (!errorKey) {
    return unknownError.value;
  }
  const translated = t(errorKey);
  return translated === errorKey ? unknownError.value : translated;
}

// ==================== 语言切换 ====================
const currentLanguage = ref(i18n.global.locale.value);
// 动态状态块文案在请求时定稿：语言切换后按旧版语义重新拉取
watch(currentLanguage, (lang) => {
  changeLanguage(lang);
  checkPostgreSQL();
  if (currentStep.value >= 3) {
    checkDatabaseStatus();
  }
});

// ==================== 第 1 步：PostgreSQL 检查 ====================
// loading | success | error（loading 时容器内渲染检查中转圈）
const pgCheck = ref("loading");
const pgMessage = ref("");
const pgError = ref("");
const pgInstalled = ref(false);
const pgRunning = ref(false);

async function checkPostgreSQL() {
  showLoading();
  pgCheck.value = "loading";
  const result = await apiGet("/api/init/check-pgsql");
  hideLoading();
  if (result.installed && result.running) {
    pgCheck.value = "success";
    pgMessage.value = result.message || t("init.pg_connected");
  } else {
    pgCheck.value = "error";
    pgInstalled.value = Boolean(result.installed);
    pgRunning.value = Boolean(result.running);
    pgError.value = translateErrorField(result.error);
  }
}

// ==================== 第 2 步：数据库配置 ====================
const dbConfig = reactive({
  type: "pgsql",
  host: "",
  port: "",
  database: "",
  username: "",
  password: ""
});
// 连接测试状态反馈块：null 隐藏 | { detail }
const dbConfigStatus = ref(null);
const dbConfigSubmitting = ref(false);

/**
 * 第 2 步「连接测试」：充当下一步按钮。
 * 成功 -> 后端已写入配置并切换连接，进入第 3 步（表创建流程）；
 * 失败 -> 展示后端分类错误详情，并提示检查输入或先用 init-pgsql.sh 建库。
 */
async function submitDbConfig() {
  if (dbConfigSubmitting.value) {
    return;
  }
  const payload = {
    type: "pgsql",
    host: dbConfig.host.trim(),
    port: dbConfig.port.trim(),
    database: dbConfig.database.trim(),
    username: dbConfig.username.trim(),
    password: dbConfig.password
  };
  if (!payload.host || !payload.port || !payload.database || !payload.username || !payload.password) {
    showError(t("init.db_fill_required"));
    return;
  }
  dbConfigSubmitting.value = true;
  showLoading();
  const result = await apiPost("/api/init/db/test-connection", payload);
  hideLoading();
  dbConfigSubmitting.value = false;
  if (result.success) {
    dbConfigStatus.value = null;
    showToast(t("init.db_test_pass"), "success");
    goToStep(3);
    checkDatabaseStatus();
  } else {
    // 失败时模板附加通用操作指引；分类文案已含指引的重复展示无害
    dbConfigStatus.value = { detail: result.message || unknownError.value };
  }
}

// ==================== 第 3 步：数据库初始化 ====================
const dbStatus = ref(null); // { connected, has_data, required_tables_exist, error }
const sqlFile = ref(null);
const verificationStep2 = ref("");
const initSubmitting = ref(false);

/** 检查数据库连接与表结构状态，按四分支渲染状态块（下一步在全部分支均可用） */
async function checkDatabaseStatus() {
  showLoading();
  const result = await apiGet("/api/init/db-status");
  hideLoading();
  // success 但 data 为 null 视同失败：不读取空对象
  if (result.success && result.data) {
    dbStatus.value = result.data;
  } else {
    showError(t("init.check_db_status_failed"));
  }
}

// 状态块配色分支：connected 且有数据 / 表结构不全 -> 警告，连通 -> 成功，连不通 -> 错误
const dbStatusKind = computed(() => {
  const status = dbStatus.value;
  if (!status) {
    return "loading";
  }
  if (!status.connected) {
    return "error";
  }
  if (status.has_data || !status.required_tables_exist) {
    return "warning";
  }
  return "success";
});

function onSqlFileChange(e) {
  sqlFile.value = e.target.files[0] || null;
}

/** 第三步表单提交：根据初始化方式新建或导入数据库（导入含 .sql 备份文件分支） */
async function submitInitMode() {
  if (initSubmitting.value) {
    return;
  }
  if (!verificationStep2.value) {
    showError(t("init.enter_captcha"));
    return;
  }
  initSubmitting.value = true;
  showLoading();
  let result;
  if (initMode.value === "import" && sqlFile.value) {
    const formData = new FormData();
    formData.append("verification", verificationStep2.value);
    formData.append("sql_file", sqlFile.value);
    result = await apiPost("/api/init/db/import-file", formData);
  } else {
    const endpoint = initMode.value === "create" ? "/api/init/db/create" : "/api/init/db/import";
    result = await apiPost(endpoint, { verification: verificationStep2.value });
  }
  hideLoading();
  initSubmitting.value = false;
  if (result.success) {
    let message = t("init.db_init_success");
    if (result.data?.backup_file) {
      message += `\n${t("init.backup_file")} ${result.data.backup_file}`;
    }
    showToast(message, "success");
    setTimeout(() => goToStep(4), 1000);
  } else {
    showError(`${t("init.operation_failed")}: ${result.message || unknownError.value}`);
  }
}

// ==================== 第 4 步：管理员账户 ====================
const admin = reactive({
  username: "admin",
  password: "",
  confirmPassword: "",
  email: "",
  verification: ""
});
const adminSubmitting = ref(false);

/** 第四步表单提交：创建管理员账户并触发系统重启 */
async function submitAdmin() {
  if (adminSubmitting.value) {
    return;
  }
  if (admin.password !== admin.confirmPassword) {
    showError(t("init.password_mismatch"));
    return;
  }
  adminSubmitting.value = true;
  showLoading();
  const result = await apiPost("/api/init", {
    username: admin.username,
    password: admin.password,
    email: admin.email,
    role: "admin",
    verification: admin.verification
  });
  hideLoading();
  adminSubmitting.value = false;
  if (!result.success) {
    showError(`${t("init.init_failed")}: ${result.message || unknownError.value}`);
    return;
  }
  goToStep(5);
  setTimeout(handleRestart, 3000);
}

// ==================== 第 5 步：完成与重启 ====================
// 后端检测到程序未注册为 systemd 服务时不执行重启（restart_mode = manual），
// 切换为手动重启指引并隐藏自动跳转与登录入口
const manualRestart = ref(false);

async function handleRestart() {
  const restartResult = await apiPost("/api/init/restart");
  if (restartResult?.data?.restart_mode === "manual") {
    manualRestart.value = true;
    return;
  }
  await waitForBackendRestart();
}

/**
 * systemd 自退出重启后轮询后端恢复（/api/auth/init-status 两分支常驻注册），
 * 恢复后跳转登录页。重启窗口内请求失败属预期，静默重试；
 * 超时（服务未能自行拉起）提示用户后仍尝试跳转。
 */
async function waitForBackendRestart() {
  const maxAttempts = 30;
  // 递归轮询替代 for+await（no-await-in-loop）：探测间隔 1s，最多 30 次
  const probe = async (attempt) => {
    try {
      const response = await fetch("/api/auth/init-status", {
        headers: { "Content-Type": "application/json" },
        cache: "no-store"
      });
      if (response.ok) {
        const result = await response.json();
        if (result?.success) {
          window.location.href = "/main.html";
          return;
        }
      }
    } catch {
      // 服务重启窗口内连接失败属预期：静默进入下一轮
    }
    if (attempt >= maxAttempts) {
      showToast(t("init.restart_timeout"), "warning");
      window.location.href = "/main.html";
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
    return probe(attempt + 1);
  };
  return probe(1);
}

// ==================== 控制台验证码 ====================
const captchaLoading = ref(false);

/** 获取并展示服务器控制台验证码（请求期间禁用全部验证码按钮） */
async function getVerificationCode() {
  if (captchaLoading.value) {
    return;
  }
  captchaLoading.value = true;
  showLoading();
  const result = await apiGet("/api/init/verification-code");
  hideLoading();
  captchaLoading.value = false;
  if (result.success) {
    showToast(result.message, "success");
  } else {
    showError(`${t("init.captcha_failed")}: ${result.message || unknownError.value}`);
  }
}

onMounted(() => {
  checkPostgreSQL();
});
</script>

<template>
  <div class="init-container">
    <div class="init-header">
      <div class="init-lang-switch">
        <select id="language-selector" v-model="currentLanguage" aria-label="Select Language">
          <option value="zh">中文</option>
          <option value="en">English</option>
        </select>
      </div>
      <h1>{{ t("init.title") }}</h1>
      <p>{{ t("init.subtitle") }}</p>
    </div>

    <div class="init-steps">
      <div class="step-indicator">
        <div
          v-for="num in 5"
          :key="num"
          class="step"
          :class="{ active: num === currentStep, completed: num < currentStep }"
          @click="num < currentStep && goToStep(num)"
        >
          <span class="step-number">{{ num }}</span>
          <span class="step-label">{{ t(`init.step_${["pg_check", "db_config", "db_init", "admin", "done"][num - 1]}`) }}</span>
        </div>
      </div>

      <div class="step-content">
        <!-- 第 1 步：PostgreSQL 检查 -->
        <div class="step-panel" :class="{ active: currentStep === 1 }">
          <h2>{{ t("init.step_pg_check") }}</h2>
          <div
            class="status-container"
            :class="pgCheck === 'success' ? 'status-success' : pgCheck === 'error' ? 'status-error' : ''"
          >
            <template v-if="pgCheck === 'loading'">
              <div class="status-loading">
                <div class="loading-spinner"></div>
                <p>{{ t("init.pg_checking") }}</p>
              </div>
            </template>
            <template v-else-if="pgCheck === 'success'">
              <div class="status-success">✓</div>
              <h3>{{ t("init.pg_running") }}</h3>
              <p>{{ pgMessage }}</p>
            </template>
            <template v-else>
              <div class="status-error">✗</div>
              <h3>{{ t("init.pg_check_failed") }}</h3>
              <p>{{ pgError }}</p>
              <div class="error-guide">
                <h4>{{ t("init.pg_suggestion") }}</h4>
                <ul>
                  <li v-if="!pgInstalled">{{ t("init.pg_install_hint") }}</li>
                  <li v-if="pgInstalled && !pgRunning">{{ t("init.pg_start_hint") }}</li>
                  <li>{{ t("init.pg_config_hint") }}</li>
                  <li>{{ t("init.pg_network_hint") }}</li>
                </ul>
              </div>
            </template>
          </div>
          <div class="form-actions">
            <button type="button" class="init-btn init-btn-primary" @click="checkPostgreSQL">
              {{ t("init.pg_recheck") }}
            </button>
            <button
              type="button"
              class="init-btn init-btn-success"
              :disabled="pgCheck !== 'success'"
              @click="goToStep(2)"
            >
              {{ t("init.next") }}
            </button>
          </div>
        </div>

        <!-- 第 2 步：数据库配置（连接测试充当下一步） -->
        <div class="step-panel" :class="{ active: currentStep === 2 }">
          <h2>{{ t("init.db_config_title") }}</h2>
          <div v-if="dbConfigStatus" class="status-container status-error">
            <div class="status-error">✗</div>
            <h3>{{ t("init.db_test_fail_title") }}</h3>
            <p>{{ dbConfigStatus.detail }}</p>
            <p>{{ t("init.db_test_fail_hint") }}</p>
          </div>
          <form id="db-config-form" class="init-form" @submit.prevent="submitDbConfig">
            <div class="init-form-group">
              <label for="db-type">{{ t("init.db_type") }}</label>
              <!-- 下拉框为未来扩展预留，当前仅支持 PostgreSQL（默认选中） -->
              <select id="db-type" v-model="dbConfig.type" name="db_type">
                <option value="pgsql">PostgreSQL</option>
              </select>
            </div>
            <div class="init-form-group">
              <label for="db-host">{{ t("init.db_host") }}</label>
              <input
                id="db-host"
                v-model="dbConfig.host"
                type="text"
                name="host"
                :placeholder="t('init.db_host_placeholder')"
                required
              />
            </div>
            <div class="init-form-group">
              <label for="db-port">{{ t("init.db_port") }}</label>
              <input
                id="db-port"
                v-model="dbConfig.port"
                type="number"
                name="port"
                placeholder="5432"
                min="1"
                max="65535"
                required
              />
            </div>
            <div class="init-form-group">
              <label for="db-name">{{ t("init.db_name") }}</label>
              <input
                id="db-name"
                v-model="dbConfig.database"
                type="text"
                name="database"
                :placeholder="t('init.db_name_placeholder')"
                required
                autocomplete="off"
              />
            </div>
            <div class="init-form-group">
              <label for="db-username">{{ t("init.db_username") }}</label>
              <input id="db-username" v-model="dbConfig.username" type="text" name="username" required autocomplete="off" />
            </div>
            <div class="init-form-group">
              <label for="db-password">{{ t("init.db_password") }}</label>
              <input id="db-password" v-model="dbConfig.password" type="password" name="password" autocomplete="new-password" required />
            </div>
            <p class="init-help-text">{{ t("init.db_config_help") }}</p>
            <div class="form-actions">
              <button type="button" class="init-btn init-btn-secondary" @click="goToStep(currentStep - 1)">
                {{ t("init.prev") }}
              </button>
              <button type="submit" class="init-btn init-btn-success" :disabled="dbConfigSubmitting">
                {{ t("init.db_test_btn") }}
              </button>
            </div>
          </form>
        </div>

        <!-- 第 3 步：数据库初始化（新建 / 导入） -->
        <div class="step-panel" :class="{ active: currentStep === 3 }">
          <h2>{{ t("init.step_db_init") }}</h2>
          <div class="status-container" :class="dbStatusKind === 'loading' ? '' : `status-${dbStatusKind}`">
            <template v-if="dbStatusKind === 'loading'">
              <div class="status-loading">
                <div class="loading-spinner"></div>
                <p>{{ t("init.checking_db_status") }}</p>
              </div>
            </template>
            <template v-else-if="dbStatusKind === 'success'">
              <div class="status-success">✓</div>
              <h3>{{ t("init.db_status_title") }}</h3>
              <p>{{ t("init.db_schema_ok") }}</p>
              <p>{{ t("init.select_init_method") }}</p>
            </template>
            <template v-else-if="dbStatusKind === 'warning' && dbStatus?.has_data">
              <div class="status-warning">⚠</div>
              <h3>{{ t("init.db_status_title") }}</h3>
              <p>{{ t("init.db_schema_ok") }}</p>
              <p class="init-warning-text">{{ t("init.db_will_reset") }}</p>
            </template>
            <template v-else-if="dbStatusKind === 'warning'">
              <div class="status-warning">⚠</div>
              <h3>{{ t("init.db_status_title") }}</h3>
              <p>{{ t("init.db_schema_incomplete") }}</p>
              <p>{{ t("init.select_init_method_full") }}</p>
            </template>
            <template v-else>
              <div class="status-error">✗</div>
              <h3>{{ t("init.db_connect_failed") }}</h3>
              <p>{{ t("init.db_connect_error") }}: {{ dbStatus?.error ? translateErrorField(dbStatus.error) : "" }}</p>
              <p>{{ t("init.will_auto_create_db") }}</p>
            </template>
          </div>

          <div class="init-tabs">
            <button
              type="button"
              class="init-tab"
              :class="{ active: initMode === 'create' }"
              @click="initMode = 'create'"
            >
              <span class="tab-icon">📝</span>
              <span class="tab-text">{{ t("init.tab_create_db") }}</span>
            </button>
            <button
              type="button"
              class="init-tab"
              :class="{ active: initMode === 'import' }"
              @click="initMode = 'import'"
            >
              <span class="tab-icon">📁</span>
              <span class="tab-text">{{ t("init.tab_import_db") }}</span>
            </button>
          </div>

          <form id="init-mode-form" class="init-form" @submit.prevent="submitInitMode">
            <div v-show="initMode === 'create'" id="create-panel">
              <div class="panel-description">
                <h4>{{ t("init.tab_create_db") }}</h4>
                <p>{{ t("init.create_db_desc") }}</p>
                <ul>
                  <li>{{ t("init.create_db_li_tables") }}</li>
                  <li>{{ t("init.create_db_li_schema") }}</li>
                  <li>{{ t("init.create_db_li_backup") }}</li>
                </ul>
              </div>
            </div>

            <div v-show="initMode === 'import'" id="import-panel">
              <div class="panel-description">
                <h4>{{ t("init.tab_import_db") }}</h4>
                <p>{{ t("init.import_db_desc") }}</p>
              </div>
              <div class="init-form-group">
                <label for="sql-file">{{ t("init.sql_backup_file") }}</label>
                <input id="sql-file" type="file" name="sql_file" accept=".sql" class="file-input" @change="onSqlFileChange" />
                <p class="init-help-text">{{ t("init.sql_backup_hint") }}</p>
              </div>
            </div>

            <div class="init-form-group">
              <label for="verification-step2">{{ t("init.console_captcha") }}</label>
              <div class="init-button-row">
                <input
                  id="verification-step2"
                  v-model="verificationStep2"
                  type="text"
                  name="verification"
                  :placeholder="t('init.console_captcha_hint')"
                  required
                />
                <button
                  type="button"
                  class="init-btn init-btn-primary"
                  :disabled="captchaLoading"
                  @click="getVerificationCode"
                >
                  {{ t("init.get_captcha") }}
                </button>
              </div>
              <p class="init-help-text">{{ t("init.console_captcha_help") }}</p>
            </div>

            <div class="form-actions">
              <button type="button" class="init-btn init-btn-secondary" @click="goToStep(currentStep - 1)">
                {{ t("init.prev") }}
              </button>
              <button type="submit" class="init-btn init-btn-success" :disabled="initSubmitting">
                {{ t("init.next") }}
              </button>
            </div>
          </form>
        </div>

        <!-- 第 4 步：管理员账户创建 -->
        <div class="step-panel" :class="{ active: currentStep === 4 }">
          <h2>{{ t("init.admin_create_title") }}</h2>
          <form id="admin-account-form" class="init-form" @submit.prevent="submitAdmin">
            <div class="init-form-group">
              <label for="admin-username">{{ t("init.username") }}</label>
              <input id="admin-username" v-model="admin.username" type="text" name="username" required autocomplete="username" />
            </div>
            <div class="init-form-group">
              <label for="admin-password">{{ t("init.password") }}</label>
              <input id="admin-password" v-model="admin.password" type="password" name="password" autocomplete="new-password" required />
            </div>
            <div class="init-form-group">
              <label for="admin-confirm-password">{{ t("init.confirm_password") }}</label>
              <input id="admin-confirm-password" v-model="admin.confirmPassword" type="password" name="confirm_password" autocomplete="new-password" required />
            </div>
            <div class="init-form-group">
              <label for="admin-email">{{ t("init.email") }}</label>
              <input id="admin-email" v-model="admin.email" type="email" name="email" placeholder="admin@example.com" required autocomplete="email" />
            </div>
            <div class="init-form-group">
              <label for="verification">{{ t("init.console_captcha") }}</label>
              <div class="init-button-row">
                <input
                  id="verification"
                  v-model="admin.verification"
                  type="text"
                  name="verification"
                  :placeholder="t('init.console_captcha_hint')"
                  required
                />
                <button
                  type="button"
                  class="init-btn init-btn-primary"
                  :disabled="captchaLoading"
                  @click="getVerificationCode"
                >
                  {{ t("init.get_captcha") }}
                </button>
              </div>
              <p class="init-help-text">{{ t("init.console_captcha_help") }}</p>
            </div>
            <div class="form-actions">
              <button type="button" class="init-btn init-btn-secondary" @click="goToStep(currentStep - 1)">
                {{ t("init.prev") }}
              </button>
              <button type="submit" class="init-btn init-btn-success" :disabled="adminSubmitting">
                {{ t("init.finish_init") }}
              </button>
            </div>
          </form>
        </div>

        <!-- 第 5 步：初始化完成 -->
        <div class="step-panel" :class="{ active: currentStep === 5 }">
          <div class="init-success">
            <h2>{{ t("init.done_title") }}</h2>
            <div class="success-icon">✓</div>
            <p>{{ t("init.done_desc") }}</p>
            <!-- systemd 重启分支：已注册单元，重启命令已下发 -->
            <p v-if="!manualRestart">{{ t("init.restarting") }}</p>
            <!-- 手动重启分支：未注册 systemd 单元，后端不下发重启 -->
            <div v-if="manualRestart">
              <p>{{ t("init.manual_restart") }}</p>
              <p class="manual-restart-hint">{{ t("init.manual_restart_hint") }}</p>
            </div>
            <div v-if="!manualRestart" class="form-actions">
              <a href="/main.html" class="init-btn init-btn-primary">{{ t("init.login_system") }}</a>
            </div>
          </div>
        </div>
      </div>
    </div>

    <div v-if="errorVisible" class="init-error-message">{{ errorMessage }}</div>

    <div v-if="loadingCount > 0" class="init-loading">
      <div class="loading-spinner"></div>
      <p>{{ t("init.processing") }}</p>
    </div>
  </div>
  <AppToast />
</template>
