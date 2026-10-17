<script setup>
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { api } from './useLauncher';
import PlayerAvatar from './PlayerAvatar.vue';

const props = defineProps({
  account: { type: Object, default: null },
  username: { type: String, default: 'Explorer' },
  locked: Boolean,
});
const emit = defineEmits(['changed', 'notification']);
const dialog = ref(null);
const view = ref('list');
const busy = ref(false);
const error = ref('');
const offlineName = ref('');
const yggMode = ref('littleskin');
const yggRoot = ref('');
const yggUsername = ref('');
const yggPassword = ref('');
const device = ref(null);
const windowsAccountLoginSupported = ref(null);
let pollTimer;
let generation = 0;
const accounts = computed(() => {
  const saved = props.account?.accounts || [];
  if (saved.length || props.account?.mode !== 'offline') return saved;
  return [
    {
      id: 'current-offline-account',
      mode: 'offline',
      name: props.username || 'Explorer',
      username: props.username || 'Explorer',
      active: true,
      virtual: true,
    },
  ];
});
const sortedAccounts = computed(() =>
  [...accounts.value].sort((a, b) => Number(Boolean(b.active)) - Number(Boolean(a.active))),
);
const modeNames = {
  offline: '离线账户',
  microsoft: 'Microsoft 账户',
  littleskin: 'LittleSkin 账户',
  yggdrasil: '外置登录账户',
};

function open() {
  error.value = '';
  view.value = 'list';
  offlineName.value = props.username || 'Explorer';
  if (!dialog.value?.open) dialog.value?.showModal();
  void api.refreshAccountSkins().then((result) => emit('changed', result)).catch(() => {});
}
async function cancelDevice() {
  generation += 1;
  clearTimeout(pollTimer);
  const current = device.value;
  device.value = null;
  if (current?.sessionId) {
    try {
      await api.cancelLogin();
    } catch {
      /* the session may already be complete */
    }
  }
}
async function close() {
  await cancelDevice();
  dialog.value?.close();
  error.value = '';
  emit('changed');
}
function goBack() {
  error.value = '';
  if (view.value === 'microsoft') void cancelDevice();
  view.value = 'list';
}
function showError(reason) {
  error.value = String(reason?.message || reason || '操作失败，请重试。');
}
async function addOffline() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await api.addOfflineAccount(offlineName.value.trim());
    await api.save({ username: offlineName.value.trim() });
    emit('changed', result);
    await close();
    emit('notification', '离线账户已添加并切换');
  } catch (reason) {
    showError(reason);
  } finally {
    busy.value = false;
  }
}
async function addYggdrasil() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await api.addYggdrasilAccount(
      yggMode.value,
      yggMode.value === 'littleskin' ? '' : yggRoot.value.trim(),
      yggUsername.value.trim(),
      yggPassword.value,
    );
    yggPassword.value = '';
    emit('changed', result);
    await close();
    emit('notification', '账户验证成功，已添加并切换');
  } catch (reason) {
    showError(reason);
  } finally {
    busy.value = false;
  }
}
async function selectAccount(account) {
  if (busy.value || account.active) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await api.selectSavedAccount(account.id);
    if (account.mode === 'offline') await api.save({ username: account.username || account.name });
    emit('changed', result);
    await close();
    emit('notification', `已切换到${modeNames[account.mode] || '账户'}：${account.name}`);
  } catch (reason) {
    showError(reason);
  } finally {
    busy.value = false;
  }
}
async function removeAccount(account) {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await api.removeAccount(account.id);
    const active = result?.accounts?.find((item) => item.active);
    if (active?.mode === 'offline') await api.save({ username: active.username || active.name });
    emit('changed', result);
  } catch (reason) {
    showError(reason);
  } finally {
    busy.value = false;
  }
}
async function startMicrosoft() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await api.startLogin();
    device.value = result;
    await api.openAuthWindow(result.sessionId);
    const currentGeneration = ++generation;
    void pollMicrosoft(currentGeneration);
  } catch (reason) {
    showError(reason);
  } finally {
    busy.value = false;
  }
}
async function startWindowsMicrosoft() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const result = await api.loginWithWindowsAccount();
    emit('changed', result);
    await close();
    emit('notification', 'Microsoft 账户登录成功');
  } catch (reason) {
    showError(reason);
  } finally {
    busy.value = false;
  }
}
async function pollMicrosoft(currentGeneration) {
  if (currentGeneration !== generation || !device.value) return;
  try {
    const result = await api.pollLogin(device.value.sessionId);
    if (result.status === 'pending') {
      pollTimer = setTimeout(
        () => pollMicrosoft(currentGeneration),
        Math.max(1, Number(result.retryAfter) || 5) * 1000,
      );
      return;
    }
    device.value = null;
    emit('changed', result);
    await close();
    emit('notification', 'Microsoft 账户登录成功');
  } catch (reason) {
    if (currentGeneration !== generation) return;
    device.value = null;
    showError(reason);
  }
}
function copyUserCode() {
  if (device.value?.userCode) navigator.clipboard?.writeText(device.value.userCode).catch(() => {});
}
onMounted(() => {
  window.addEventListener('launcher:account-open', open);
  void api.windowsAccountLoginSupported()
    .then((supported) => {
      windowsAccountLoginSupported.value = Boolean(supported);
    })
    .catch(() => {
      windowsAccountLoginSupported.value = false;
    });
});
onBeforeUnmount(() => {
  window.removeEventListener('launcher:account-open', open);
  void cancelDevice();
});
</script>

<template>
  <dialog
    ref="dialog"
    class="account-drawer"
    aria-labelledby="account-drawer-title"
    @click.self="close"
  >
    <header class="account-drawer-header">
      <div>
        <p class="eyebrow">ACCOUNT CENTER</p>
        <h2 id="account-drawer-title">账户管理</h2>
        <p class="account-intro">管理已保存的游戏账户，同一类型也可以添加多个账户。</p>
      </div>
      <button
        class="dialog-close"
        type="button"
        aria-label="关闭"
        @click="close"
      >
        ×
      </button>
    </header>

    <main class="account-drawer-body">
      <template v-if="view === 'list'">
        <section class="account-section">
          <div class="account-section-title">
            <h3>已保存的账户</h3>
            <span>{{ accounts.length }} 个</span>
          </div>
          <div
            v-if="!accounts.length"
            class="account-empty"
          >
            还没有账户，添加一个账户后即可开始游戏。
          </div>
          <article
            v-for="item in sortedAccounts"
            :key="item.id"
            class="saved-account"
            :class="{ active: item.active }"
          >
            <PlayerAvatar
              :account="{ mode: item.mode, profile: { id: item.uuid, skinUrl: item.skinUrl } }"
              small
            />
            <div class="saved-account-info">
              <strong>{{ item.name }}</strong>
              <span>
                {{ modeNames[item.mode] || '游戏账户' }}
                <template v-if="item.mode === 'yggdrasil'">· {{ item.apiRoot }}</template>
              </span>
              <small v-if="item.active">当前使用</small>
            </div>
            <button
              v-if="!item.active"
              class="outline-button account-switch"
              type="button"
              :disabled="locked || busy"
              @click="selectAccount(item)"
            >
              切换
            </button>
            <button
              v-if="!item.virtual"
              class="account-remove"
              type="button"
              :disabled="locked || busy"
              :aria-label="`移除 ${item.name}`"
              @click="removeAccount(item)"
            >
              移除
            </button>
          </article>
        </section>
        <section class="account-section add-account-section">
          <div class="account-section-title"><h3>添加账户</h3></div>
          <button
            class="account-method"
            type="button"
            :disabled="locked"
            @click="
              view = 'offline';
              error = '';
            "
          >
            <span class="account-method-icon">◌</span>
            <span>
              <strong>离线账户</strong>
              <small>使用自定义游戏名称</small>
            </span>
            <b>›</b>
          </button>
          <button
            class="account-method"
            type="button"
            :disabled="locked"
            @click="
              view = 'microsoft';
              error = '';
            "
          >
            <span class="account-method-icon">M</span>
            <span>
              <strong>Microsoft 账户</strong>
              <small>{{ windowsAccountLoginSupported ? '通过 Windows 系统账户登录' : '通过微软设备代码登录' }}</small>
            </span>
            <b>›</b>
          </button>
          <button
            class="account-method"
            type="button"
            :disabled="locked"
            @click="
              yggMode = 'littleskin';
              yggRoot = '';
              view = 'yggdrasil';
              error = '';
            "
          >
            <span class="account-method-icon">L</span>
            <span>
              <strong>LittleSkin 账户</strong>
              <small>使用 LittleSkin 外置认证</small>
            </span>
            <b>›</b>
          </button>
          <button
            class="account-method"
            type="button"
            :disabled="locked"
            @click="
              yggMode = 'yggdrasil';
              view = 'yggdrasil';
              error = '';
            "
          >
            <span class="account-method-icon">Y</span>
            <span>
              <strong>外部账户</strong>
              <small>连接其他 Yggdrasil 认证服务器</small>
            </span>
            <b>›</b>
          </button>
        </section>
        <p class="account-storage-note">
          登录凭据使用 Windows 系统加密保存；密码只用于验证，不会保存在启动器中。
        </p>
      </template>

      <template v-else-if="view === 'offline'">
        <button
          class="account-back"
          type="button"
          @click="goBack"
        >
          ← 返回账户列表
        </button>
        <section class="account-section account-form">
          <h3>添加离线账户</h3>
          <p class="account-intro">输入 3–16 位字母、数字或下划线作为游戏名称。</p>
          <label>
            游戏名称
            <input
              v-model="offlineName"
              maxlength="16"
              autocomplete="off"
              placeholder="例如：Explorer"
              @keydown.enter="addOffline"
            />
          </label>
          <div class="dialog-actions">
            <button
              class="outline-button"
              type="button"
              @click="goBack"
            >
              取消
            </button>
            <button
              class="primary-small"
              type="button"
              :disabled="busy || locked"
              @click="addOffline"
            >
              添加并切换
            </button>
          </div>
        </section>
      </template>

      <template v-else-if="view === 'yggdrasil'">
        <button
          class="account-back"
          type="button"
          @click="goBack"
        >
          ← 返回账户列表
        </button>
        <section class="account-section account-form">
          <h3>
            {{ yggMode === 'littleskin' ? '添加 LittleSkin 账户' : '添加外部 Yggdrasil 账户' }}
          </h3>
          <p class="account-intro">输入账户名和密码进行服务器验证。密码不会被保存。</p>
          <label v-if="yggMode === 'yggdrasil'">
            Yggdrasil API 地址
            <input
              v-model="yggRoot"
              type="url"
              autocomplete="url"
              placeholder="https://auth.example.com/"
            />
          </label>
          <div
            v-else
            class="account-fixed-api"
          >
            <span>认证服务器</span>
            <strong>https://littleskin.cn/api/yggdrasil/</strong>
            <small>LittleSkin 官方 Yggdrasil API</small>
          </div>
          <label>
            账户名
            <input
              v-model="yggUsername"
              autocomplete="username"
              placeholder="邮箱或游戏账户名"
            />
          </label>
          <label>
            密码
            <input
              v-model="yggPassword"
              type="password"
              autocomplete="current-password"
              @keydown.enter="addYggdrasil"
            />
          </label>
          <div class="dialog-actions">
            <button
              class="outline-button"
              type="button"
              @click="goBack"
            >
              取消
            </button>
            <button
              class="primary-small"
              type="button"
              :disabled="busy || locked"
              @click="addYggdrasil"
            >
              {{ busy ? '正在验证…' : '验证并添加' }}
            </button>
          </div>
        </section>
      </template>

      <template v-else-if="view === 'microsoft'">
        <button
          class="account-back"
          type="button"
          @click="goBack"
        >
          ← 返回账户列表
        </button>
        <section class="account-section account-form">
          <h3>添加 Microsoft 账户</h3>
          <p class="account-intro">
            {{ windowsAccountLoginSupported
              ? '使用 Windows 系统账户窗口登录，微软凭据由 Windows 安全管理。'
              : '使用设备代码在浏览器中完成 Microsoft 账户登录。' }}
          </p>
          <div
            v-if="device"
            class="microsoft-device"
          >
            <span>请在浏览器中输入此代码</span>
            <button
              type="button"
              class="device-code"
              @click="copyUserCode"
            >
              {{ device.userCode }}
              <small>复制</small>
            </button>
            <a
              :href="device.verificationUri"
              target="_blank"
              rel="noreferrer"
            >
              打开微软登录页面
            </a>
            <button
              class="outline-button"
              type="button"
              @click="
                cancelDevice();
                device = null;
              "
            >
              取消登录
            </button>
          </div>
          <template v-else>
            <button
              v-if="windowsAccountLoginSupported === true"
              class="primary-small microsoft-login"
              type="button"
              :disabled="busy || locked || !account?.configured"
              @click="startWindowsMicrosoft"
            >
              {{ busy ? '正在连接 Windows 账户…' : '使用 Windows 账户登录' }}
            </button>
            <button
              v-else-if="windowsAccountLoginSupported === false"
              class="primary-small microsoft-login"
              type="button"
              :disabled="busy || locked || !account?.configured"
              @click="startMicrosoft"
            >
              使用设备代码登录
            </button>
            <p
              v-if="!account?.configured"
              class="account-hint"
            >
              当前版本尚未启用 Microsoft 登录，请联系启动器维护者。
            </p>
          </template>
        </section>
      </template>

      <p
        v-if="error"
        class="account-error"
        role="alert"
      >
        {{ error }}
      </p>
    </main>
  </dialog>
</template>
