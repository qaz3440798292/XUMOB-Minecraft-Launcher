<script setup>
import { computed } from 'vue';
import { useApp } from './useApp';
import AccountDialog from './AccountDialog.vue';
import DownloadCenter from './DownloadCenter.vue';
import DownloadDetails from './DownloadDetails.vue';
import VersionSelect from './VersionSelect.vue';
import SearchSelect from './SearchSelect.vue';
import ServerList from './ServerList.vue';
import PlayerAvatar from './PlayerAvatar.vue';
import { loaderMark } from './loaderIcons';
import { resolveForgeRequirement } from './forgeRequirement';
const {
  memoryMax,
  versionSettingsMemoryMax,
  draftMemoryMax,
  memoryStyle,
  memoryUsed,
  memoryWarning,
  memoryNote,
  accountName,
  accountLabel,
  accountModeLabel,
  api,
  labels,
  model,
  settings,
  state,
  downloadSpeed,
  version,
  currentView,
  loaded,
  busy,
  running,
  message,
  directoryOptions,
  versionOptions,
  versionFilterOptions,
  filteredVersions,
  selectedVersionIds,
  versionBatchMode,
  toggleVersionBatchMode,
  allVisibleVersionsSelected,
  toggleVersionSelection,
  toggleVisibleVersions,
  deleteSelectedVersions,
  versionSearch,
  versionLoader,
  filteredDirectories,
  directorySearch,
  javaOptions,
  selectedJava,
  javaSummary,
  javaDetail,
  javaPath,
  filteredMods,
  selectedModNames,
  allVisibleModsSelected,
  toggleModSelection,
  toggleVisibleMods,
  deleteSelectedMods,
  manageMod,
  mods,
  modSearch,
  checkingModUpdates,
  updatingMod,
  modUpdates,
  modUpdateWarnings,
  hasCheckedModUpdates,
  updateCount,
  modUpdateBusy,
  checkModUpdates,
  updateMod,
  updateMods,
  logs,
  follow,
  draft,
  homeMemory,
  scanning,
  launching,
  repairBusy,
  toastText,
  toastTitle,
  toastIconUrl,
  toastError,
  toastVisible,
  confirmation,
  answerConfirm,
  checkResult,
  launchLabel,
  logStatus,
  gb,
  safely,
  toast,
  refresh,
  scanJava,
  changeVersion,
  versionSettingsTarget,
  versionSettingsDraft,
  versionSettingsItem,
  componentInfo,
  componentLoading,
  componentApplying,
  componentLoader,
  componentVersion,
  componentOptifine,
  componentOptifineVersion,
  componentVersions,
  optifineVersions,
  optifineIncompatible,
  optifineForgeRequirement,
  changeComponentLoader,
  applyVersionLoader,
  versionJavaOptions,
  openVersionSettings,
  openServerList,
  serverListVersion,
  closeVersionSettings,
  saveVersionSettings,
  resetVersionSettings,
  changeDirectory,
  addDirectory,
  chooseFolder,
  saveSettings,
  onHomeMemory,
  saveHomeMemory,
  chooseJava,
  selectView,
  editAccount,
  checkEnvironment,
  closeCheck,
  repairFiles,
  launchGame,
  clearLogs,
  contentChanged,
  drag,
} = useApp();
function dismissToast() {
  toastVisible.value = false;
}
function clearToastIcon() {
  toastIconUrl.value = '';
}
const downloadActive = computed(() => ['downloading', 'downloading_java', 'repairing'].includes(state.value.phase));
const downloadPercent = computed(() => {
  const progress = state.value.progress || {};
  const byteBased = progress.unit === 'bytes';
  const done = Number(byteBased ? progress.bytes : progress.fileIndex) || 0;
  const total = Number(byteBased ? progress.totalBytes : progress.fileCount) || 0;
  return total > 0 ? Math.min(100, Math.floor((done / total) * 100)) : 0;
});
function formatDownloadBytes(value) {
  const bytes = Math.max(0, Number(value) || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}
function formatDownloadSpeed(value) {
  return `${formatDownloadBytes(value)}/s`;
}
const isolationOptions = [
  { value: 'auto', label: '自动识别（推荐）', meta: '沿用已有模组、配置与存档' },
  { value: 'true', label: '版本独立目录', meta: '每个版本单独保存游戏文件' },
  { value: 'false', label: '共享游戏根目录', meta: '多个版本共用模组与配置' },
];
</script>
<template>
  <svg
    class="sprite"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <symbol
      id="i-play"
      viewBox="0 0 24 24"
    >
      <path d="m8 5 11 7-11 7Z" />
    </symbol>
    <symbol
      id="i-grid"
      viewBox="0 0 24 24"
    >
      <rect
        x="4"
        y="4"
        width="6"
        height="6"
        rx="1"
      />
      <rect
        x="14"
        y="4"
        width="6"
        height="6"
        rx="1"
      />
      <rect
        x="4"
        y="14"
        width="6"
        height="6"
        rx="1"
      />
      <rect
        x="14"
        y="14"
        width="6"
        height="6"
        rx="1"
      />
    </symbol>
    <symbol
      id="i-box"
      viewBox="0 0 24 24"
    >
      <path d="m12 3 9 5-9 5-9-5Zm-9 5v10l9 5 9-5V8M12 13v10" />
    </symbol>
    <symbol
      id="i-server"
      viewBox="0 0 24 24"
    >
      <rect x="4" y="4" width="16" height="6" rx="1.5" />
      <rect x="4" y="14" width="16" height="6" rx="1.5" />
      <path d="M8 7h.01M8 17h.01M12 7h5M12 17h5" />
    </symbol>
    <symbol
      id="i-settings"
      viewBox="0 0 24 24"
    >
      <path d="M4 7h16M4 17h16M8 4v6m8 4v6" />
      <circle
        cx="8"
        cy="7"
        r="2"
      />
      <circle
        cx="16"
        cy="17"
        r="2"
      />
    </symbol>
    <symbol
      id="i-terminal"
      viewBox="0 0 24 24"
    >
      <path d="m5 7 5 5-5 5m8 0h6" />
    </symbol>
    <symbol
      id="i-folder"
      viewBox="0 0 24 24"
    >
      <path d="M3 6h7l2 3h9v11H3Z" />
    </symbol>
    <symbol
      id="i-arrow"
      viewBox="0 0 24 24"
    >
      <path d="M5 12h14m-6-6 6 6-6 6" />
    </symbol>
    <symbol
      id="i-check"
      viewBox="0 0 24 24"
    >
      <path d="m5 12 4 4L19 6" />
    </symbol>
    <symbol
      id="i-refresh"
      viewBox="0 0 24 24"
    >
      <path d="M20 8a9 9 0 1 0 1 8M20 3v6h-6" />
    </symbol>
    <symbol
      id="i-java"
      viewBox="0 0 24 24"
    >
      <path d="M5 10h11v6a5 5 0 0 1-10 0v-6m11 1h2a3 3 0 0 1 0 6h-3M5 22h13M9 2v4m4-3v3" />
    </symbol>
    <symbol
      id="i-memory"
      viewBox="0 0 24 24"
    >
      <path d="M3 6h18v12H3ZM6 18v3m4-3v3m4-3v3m4-3v3M7 10v4m5-4v4m5-4v4" />
    </symbol>
    <symbol
      id="i-download"
      viewBox="0 0 24 24"
    >
      <path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5" />
    </symbol>
    <symbol
      id="i-search"
      viewBox="0 0 24 24"
    >
      <circle
        cx="10"
        cy="10"
        r="6"
      />
      <path d="m15 15 6 6" />
    </symbol>
    <symbol
      id="i-info"
      viewBox="0 0 24 24"
    >
      <circle
        cx="12"
        cy="12"
        r="9"
      />
      <path d="M12 11v6m0-11v1" />
    </symbol>
    <symbol
      id="i-win-minimize"
      viewBox="0 0 24 24"
    >
      <path d="M6 12h12" />
    </symbol>
    <symbol
      id="i-win-maximize"
      viewBox="0 0 24 24"
    >
      <rect
        x="6.5"
        y="6.5"
        width="11"
        height="11"
        rx="1"
      />
    </symbol>
    <symbol
      id="i-win-close"
      viewBox="0 0 24 24"
    >
      <path d="m7 7 10 10M17 7 7 17" />
    </symbol>
  </svg>
  <div class="app-shell">
    <aside class="sidebar">
      <div
        class="brand drag"
        aria-label="XUMOB Minecraft Launcher"
        title="XUMOB Minecraft Launcher"
        @mousedown="drag"
      >
        <svg
          class="brand-mark"
          viewBox="0 0 44 44"
          aria-hidden="true"
        >
          <path
            fill="#b6df86"
            d="m22 3 18 10-18 10L4 13Z"
          />
          <path
            fill="#5f9669"
            d="m4 13 18 10v20L4 33Z"
          />
          <path
            fill="#8eb87d"
            d="m22 23 18-10v20L22 43Z"
          />
          <path
            fill="#152c20"
            d="m22 14 6 3-6 3-6-3Z"
          />
        </svg>
        <div>
          <strong>XUMOB</strong>
          <small>Minecraft Launcher</small>
        </div>
      </div>
      <div class="nav-label">你的游戏库</div>
      <nav aria-label="主导航">
        <button
          class="nav-item"
          :class="{ active: currentView === 'home' }"
          data-view="home"
          @click="selectView('home')"
        >
          <svg><use href="#i-play" /></svg>
          <span>开始游戏</span>
          <span class="nav-dot"></span>
        </button>
        <button
          class="nav-item"
          :class="{ active: currentView === 'versions' }"
          data-view="versions"
          @click="selectView('versions')"
        >
          <svg><use href="#i-grid" /></svg>
          <span>游戏库</span>
          <small id="version-count">{{ model.versions.length }}</small>
        </button>
        <button
          class="nav-item"
          :class="{ active: currentView === 'mods' }"
          data-view="mods"
          @click="selectView('mods')"
        >
          <svg><use href="#i-box" /></svg>
          <span>模组列表</span>
          <small id="nav-mod-count">{{ version?.modCount || 0 }}</small>
        </button>
        <button
          class="nav-item"
          :class="{ active: currentView === 'server-list' }"
          data-view="server-list"
          :disabled="!version"
          @click="safely(() => openServerList(settings.versionId))"
        >
          <svg><use href="#i-server" /></svg>
          <span>服务器</span>
        </button>
        <button
          id="nav-downloads"
          class="nav-item"
          :class="{ active: ['downloads', 'download-details'].includes(currentView) }"
          data-view="downloads"
          @click="selectView('downloads')"
        >
          <svg><use href="#i-download" /></svg>
          <span>下载中心</span>
        </button>
        <div class="nav-divider"></div>
        <button
          class="nav-item"
          :class="{ active: currentView === 'settings' }"
          data-view="settings"
          @click="selectView('settings')"
        >
          <svg><use href="#i-settings" /></svg>
          <span>启动设置</span>
        </button>
        <button
          class="nav-item"
          :class="{ active: currentView === 'logs' }"
          data-view="logs"
          @click="selectView('logs')"
        >
          <svg><use href="#i-terminal" /></svg>
          <span>运行日志</span>
        </button>
      </nav>
      <div class="sidebar-bottom">
        <button
          class="folder-shortcut"
          id="open-game"
          :disabled="!settings.gameRoot"
          @click="safely(() => api.openFolder('game'))"
        >
          <svg><use href="#i-folder" /></svg>
          <span>打开游戏文件夹</span>
          <span>↗</span>
        </button>
        <div class="build-label">
          <span class="status-dot"></span>
          本地整合包启动器
          <small>v0.7.0</small>
        </div>
      </div>
    </aside>
    <div class="workspace">
      <header
        class="topbar drag"
        @mousedown="drag"
      >
        <div class="breadcrumb">
          工作台
          <span>/</span>
          <strong id="breadcrumb-title">{{ labels[currentView] }}</strong>
        </div>
        <div class="top-right no-drag">
          <button
            class="account-chip"
            id="edit-account"
            @click="safely(editAccount)"
          >
            <PlayerAvatar :account="model.account" />
            <span>
              <strong id="account-name">{{ accountName }}</strong>
              <small>{{ accountLabel }}</small>
            </span>
          </button>
          <div class="window-controls">
            <button
              data-window="minimize"
              @click="safely(() => api.windowAction('minimize'))"
              aria-label="最小化"
            >
              <svg
                viewBox="0 0 24 24"
                aria-hidden="true"
                focusable="false"
              >
                <use href="#i-win-minimize" />
              </svg>
            </button>
            <button
              data-window="maximize"
              @click="safely(() => api.windowAction('maximize'))"
              aria-label="最大化"
            >
              <svg
                viewBox="0 0 24 24"
                aria-hidden="true"
                focusable="false"
              >
                <use href="#i-win-maximize" />
              </svg>
            </button>
            <button
              data-window="close"
              @click="safely(() => api.windowAction('close'))"
              class="close-window"
              aria-label="关闭"
            >
              <svg
                viewBox="0 0 24 24"
                aria-hidden="true"
                focusable="false"
              >
                <use href="#i-win-close" />
              </svg>
            </button>
          </div>
        </div>
      </header>
      <main class="main-content">
        <section
          id="view-home"
          class="view"
          :class="{ active: currentView === 'home' }"
        >
          <div class="page-heading">
            <div>
              <div class="eyebrow">LET'S PLAY</div>
              <h1>
                欢迎回来，冒险家
                <span class="heading-dot">.</span>
              </h1>
              <p>管理你的本地版本和整合包，开始下一段冒险。</p>
            </div>
            <button
              class="quiet-button"
              id="refresh-home"
              @click="
                safely(async () => {
                  await refresh();
                  toast('本地配置已刷新');
                })
              "
            >
              <svg><use href="#i-refresh" /></svg>
              刷新
            </button>
          </div>
          <div class="library-toolbar">
            <div>
              <label for="home-directory">当前游戏目录</label>
              <SearchSelect
                id="home-directory"
                label="切换游戏目录"
                :model-value="settings.gameRoot"
                :options="directoryOptions"
                :disabled="busy || running"
                @change="(path) => safely(() => changeDirectory(path))"
              />
            </div>
            <button
              id="add-home-directory"
              :disabled="busy || running"
              @click="safely(chooseFolder)"
              class="outline-button"
            >
              <svg><use href="#i-folder" /></svg>
              添加游戏目录
            </button>
          </div>
          <div class="home-grid">
            <article class="hero-card">
              <img
                class="hero-art"
                src="/landscape.svg"
                alt="月光下的方块山林与营地"
              />
              <div class="hero-shade"></div>
              <div class="hero-top">
                <span
                  class="label-tag"
                  id="hero-kind"
                >
                  {{ version ? (version.modded ? '本地整合包' : '原版游戏') : '你的游戏库' }}
                </span>
              </div>
              <div class="hero-copy">
                <div class="hero-kicker">
                  <span></span>
                  YOUR NEXT ADVENTURE
                </div>
                <h2 id="hero-title">{{ version?.id || '所有世界，从这里出发' }}</h2>
                <p id="hero-description">
                  {{
                    version
                      ? '读取当前版本配置，每个世界都有自己的启动设置。'
                      : '添加游戏目录，管理原版与不同加载器的整合包。'
                  }}
                </p>
                <div class="hero-tags">
                  <span id="hero-minecraft">
                    {{ version ? `Minecraft ${version.minecraft}` : '原版 / 整合包' }}
                  </span>
                  <span id="hero-loader">{{ version?.loader || '自动识别加载器' }}</span>
                  <span id="hero-mods">
                    {{ version ? `${version.modCount} 个模组` : '独立版本设置' }}
                  </span>
                </div>
              </div>
              <div class="hero-bottom">
                <span>
                  <span class="small-square"></span>
                  游戏目录
                  <span id="isolation-label">
                    {{ version ? (version.isolated ? '版本独立' : '共享根目录') : '自动识别' }}
                  </span>
                </span>
                <button
                  class="hero-folder"
                  id="hero-open-folder"
                  :disabled="!settings.gameRoot"
                  @click="safely(() => api.openFolder('game'))"
                  aria-label="打开游戏文件夹"
                >
                  <svg><use href="#i-folder" /></svg>
                </button>
              </div>
            </article>
            <article class="launch-card">
              <div class="card-eyebrow">
                启动面板
                <svg><use href="#i-play" /></svg>
              </div>
              <label
                class="field-label"
                for="home-version"
              >
                游戏版本
              </label>
              <VersionSelect
                id="home-version"
                label="选择游戏版本"
                :model-value="settings.versionId"
                :options="versionOptions"
                :disabled="busy || running"
                @change="(id) => safely(() => changeVersion(id))"
              />
              <div class="profile-row">
                <PlayerAvatar
                  :account="model.account"
                  small
                />
                <div>
                  <strong id="profile-name">{{ accountName }}</strong>
                  <small>{{ accountModeLabel }}</small>
                </div>
                <button
                  id="change-account"
                  @click="safely(editAccount)"
                  class="link-button"
                >
                  修改
                </button>
              </div>
              <div class="launch-memory">
                <span>游戏内存</span>
                <strong id="home-memory-value">
                  {{ gb(homeMemory) }}
                  <small>GB</small>
                </strong>
              </div>
              <input
                type="range"
                id="home-memory"
                min="1024"
                :max="memoryMax"
                step="256"
                :value="homeMemory"
                :disabled="busy || running"
                @input="onHomeMemory"
                @change="safely(saveHomeMemory)"
                aria-label="游戏内存"
              />
              <div class="range-hints">
                <span>1 GB</span>
                <span id="memory-max-hint">{{ `最多 ${gb(memoryMax)} GB` }}</span>
              </div>
              <div
                class="memory-capacity"
                aria-live="polite"
              >
                <div
                  class="memory-capacity-bar"
                  aria-hidden="true"
                >
                  <i
                    class="memory-system"
                    :style="memoryStyle(memoryUsed)"
                  ></i>
                  <i
                    class="memory-game"
                    :style="memoryStyle(homeMemory)"
                  ></i>
                </div>
                <div class="memory-capacity-labels">
                  <span>总内存 {{ gb(model.system.totalMB) }} GB</span>
                  <span>当前可用 {{ gb(model.system.freeMB || 0) }} GB</span>
                </div>
                <p
                  v-if="memoryWarning(homeMemory)"
                  class="memory-warning"
                >
                  {{ memoryWarning(homeMemory) }}
                </p>
                <p
                  v-else
                  class="memory-caption"
                >
                  {{ memoryNote }}
                </p>
              </div>
              <button
                id="launch-button"
                class="launch-button"
                :class="{ running }"
                :disabled="busy || launching || !version"
                @click="safely(launchGame)"
              >
                <svg><use href="#i-play" /></svg>
                <span>{{ launchLabel }}</span>
                <kbd>{{ running ? 'STOP' : busy ? 'WAIT' : 'PLAY' }}</kbd>
              </button>
              <div
                class="launch-state"
                id="launch-state"
              >
                <span class="status-dot"></span>
                <span>{{ message }}</span>
              </div>
            </article>
          </div>
          <div class="section-caption">
            <span>运行环境</span>
            <button
              id="check-environment"
              :disabled="busy || running || !version"
              @click="safely(checkEnvironment)"
              class="link-button"
            >
              检查环境
              <svg><use href="#i-arrow" /></svg>
            </button>
          </div>
          <div class="environment-grid">
            <button
              class="environment-card"
              id="java-card"
              @click="safely(() => selectView('settings'))"
            >
              <span class="environment-icon">
                <svg><use href="#i-java" /></svg>
              </span>
              <div>
                <small>JAVA 运行环境</small>
                <strong id="java-summary">{{ javaSummary }}</strong>
                <span id="java-detail">{{ javaDetail }}</span>
              </div>
              <span class="card-arrow">↗</span>
            </button>
            <button
              class="environment-card"
              id="memory-card"
              @click="safely(() => selectView('settings'))"
            >
              <span class="environment-icon gold">
                <svg><use href="#i-memory" /></svg>
              </span>
              <div>
                <small>内存分配</small>
                <strong id="memory-summary">{{ `${gb(homeMemory)} GB` }}</strong>
                <span id="system-memory">{{ `系统总内存 ${gb(model.system.totalMB)} GB` }}</span>
              </div>
              <span class="card-arrow">↗</span>
            </button>
            <button
              class="environment-card"
              id="mods-card"
              @click="safely(() => selectView('mods'))"
            >
              <span class="environment-icon blue">
                <svg><use href="#i-box" /></svg>
              </span>
              <div>
                <small>模组与内容</small>
                <strong id="mods-summary">{{ `${version?.modCount || 0} 个模组` }}</strong>
                <span>查看当前版本的本地文件</span>
              </div>
              <span class="card-arrow">↗</span>
            </button>
          </div>
          <div class="home-footer">
            <div>
              <svg><use href="#i-info" /></svg>
              <span>每个版本独立保存启动设置，可在运行日志中查看加载进度。</span>
            </div>
            <button
              id="home-show-logs"
              @click="safely(() => selectView('logs'))"
              class="link-button"
            >
              查看日志 ↗
            </button>
          </div>
        </section>
        <section
          id="view-versions"
          class="view"
          :class="{ active: currentView === 'versions' }"
        >
          <div class="page-heading">
            <div>
              <div class="eyebrow">YOUR COLLECTION</div>
              <h1>
                游戏库
                <span class="heading-dot">.</span>
              </h1>
              <p>切换游戏目录，管理不同版本与整合包。</p>
            </div>
            <button
              id="import-folder"
              :disabled="busy || running"
              @click="safely(chooseFolder)"
              class="outline-button"
            >
              <svg><use href="#i-folder" /></svg>
              添加游戏目录
            </button>
          </div>
          <div
            v-if="settings.directories.length > 2"
            class="list-toolbar"
          >
            <label class="search-field">
              <svg><use href="#i-search" /></svg>
              <input
                v-model="directorySearch"
                placeholder="搜索游戏目录名称或路径…"
                aria-label="搜索游戏目录"
              />
            </label>
            <span class="muted">
              {{ filteredDirectories.length }} / {{ settings.directories.length }} 个目录
            </span>
          </div>
          <div
            id="directory-list"
            class="directory-list"
          >
            <button
              v-for="directory in filteredDirectories"
              :key="directory.path"
              class="directory-item"
              :class="{ selected: directory.path === settings.gameRoot }"
              @click="safely(() => changeDirectory(directory.path))"
            >
              <svg><use href="#i-folder" /></svg>
              <span>
                <strong>{{ directory.name }}</strong>
                <small>{{ directory.path }}</small>
              </span>
              <span
                v-if="directory.path === settings.gameRoot"
                class="directory-current"
              >
                当前目录
              </span>
              <span v-else>→</span>
            </button>
            <div
              v-if="!filteredDirectories.length"
              class="empty-state"
            >
              {{ directorySearch.trim() ? '没有匹配的游戏目录。' : '还没有添加游戏目录。' }}
            </div>
          </div>
          <div id="directory-suggestions">
            <button
              v-for="root in model.suggestions"
              :key="root"
              class="suggested-directory"
              @click="safely(() => addDirectory(root))"
            >
              发现已有游戏目录：{{ root }} · 点击添加
            </button>
          </div>
          <div
            id="version-list"
            class="version-list"
          >
            <div
              v-if="!model.versions.length"
              class="empty-state"
            >
              你的游戏库还没有版本。
              <br />
              点击“添加游戏目录”，添加已经安装好的原版或整合包。
            </div>
            <div
              v-else
              class="version-list-toolbar"
            >
              <label class="search-field">
                <svg><use href="#i-search" /></svg>
                <input
                  v-model="versionSearch"
                  placeholder="搜索名称、游戏版本或加载器…"
                  aria-label="搜索游戏版本"
                />
              </label>
              <SearchSelect
                id="version-filter"
                label="筛选版本类型"
                v-model="versionLoader"
                :options="versionFilterOptions"
                placeholder="筛选版本类型…"
              />
              <span class="muted">
                {{ filteredVersions.length }} / {{ model.versions.length }} 个版本
              </span>
              <button
                class="outline-button"
                :aria-pressed="versionBatchMode"
                :disabled="busy || running"
                @click="toggleVersionBatchMode"
              >
                {{ versionBatchMode ? '完成' : '批量操作' }}
              </button>
              <label v-if="versionBatchMode" class="bulk-select">
                <input
                  type="checkbox"
                  :checked="allVisibleVersionsSelected"
                  :disabled="!filteredVersions.length || busy || running"
                  @change="toggleVisibleVersions"
                />
                全选当前筛选
              </label>
              <button
                v-if="versionBatchMode && selectedVersionIds.length"
                class="danger-button"
                :disabled="busy || running"
                @click="safely(deleteSelectedVersions)"
              >
                删除已选 {{ selectedVersionIds.length }} 项
              </button>
            </div>
            <article
              v-for="(item, index) in filteredVersions"
              :key="item.id"
              class="version-item"
              :class="{ selected: item.id === settings.versionId }"
              :style="{ '--row-delay': `${Math.min(index, 12) * 18}ms` }"
            >
              <span
                v-if="item.id === settings.versionId"
                class="selected-pill"
              >
                当前版本
              </span>
              <div class="version-item-top">
                <input
                  v-if="versionBatchMode"
                  type="checkbox"
                  class="version-select-checkbox"
                  :aria-label="`选择客户端 ${item.id}`"
                  :checked="selectedVersionIds.includes(item.id)"
                  :disabled="busy || running"
                  @click.stop
                  @change="toggleVersionSelection(item.id)"
                />
                <div class="version-icon">
                  <img
                    v-if="item.iconData"
                    class="pack-icon"
                    :src="item.iconData"
                    :alt="`${item.id} 整合包图标`"
                    loading="lazy"
                    decoding="async"
                  />
                  <img
                    v-else-if="loaderMark(item)?.src"
                    class="loader-mark"
                    :class="`loader-${loaderMark(item).key}`"
                    :src="loaderMark(item).src"
                    :alt="loaderMark(item).alt"
                    loading="lazy"
                    decoding="async"
                  />
                  <svg v-else><use href="#i-box" /></svg>
                </div>
                <div>
                  <h3>{{ item.id }}</h3>
                  <p>{{ item.error || `Minecraft ${item.minecraft}` }}</p>
                </div>
              </div>
              <div
                v-if="!item.error"
                class="version-meta"
              >
                <span>{{ item.loader }}</span>
                <span>{{ item.modCount }} 个模组</span>
                <span>Java {{ item.javaRequired }}+</span>
              </div>
              <div class="version-item-actions">
                <button
                  class="outline-button version-settings-button"
                  :disabled="Boolean(item.error) || busy || running"
                  @click="safely(() => openVersionSettings(item.id))"
                >
                  版本设置
                  <span class="version-settings-state">
                    {{ item.hasCustomSettings ? '独立' : '跟随全局' }}
                  </span>
                </button>
                <button
                  :class="item.id === settings.versionId ? 'outline-button' : 'primary-small'"
                  :disabled="Boolean(item.error)"
                  @click="
                    safely(async () => {
                      if (item.id !== settings.versionId) await changeVersion(item.id);
                      selectView('home');
                    })
                  "
                >
                  {{ item.id === settings.versionId ? '前往启动' : '选择版本' }}
                  <svg><use href="#i-arrow" /></svg>
                </button>
              </div>
            </article>
            <div
              v-if="model.versions.length && !filteredVersions.length"
              class="empty-state"
            >
              {{ versionSearch.trim() ? '没有匹配的游戏版本。' : '当前筛选条件下没有版本。' }}
            </div>
          </div>
          <div class="info-strip">
            <svg><use href="#i-info" /></svg>
            <span>
              按版本配置识别原版、Fabric、Quilt、Forge、NeoForge
              等已安装版本，不限制整合包名称或游戏版本。
            </span>
          </div>
        </section>
        <section
          id="view-version-settings"
          class="view"
          :class="{ active: currentView === 'version-settings' }"
        >
          <button class="quiet-button version-settings-back" @click="closeVersionSettings">
            <svg><use href="#i-arrow" /></svg>
            返回游戏库
          </button>
          <div class="page-heading version-settings-page-heading">
            <div>
              <div class="eyebrow">INSTANCE SETTINGS</div>
              <h1>
                版本设置
                <span class="heading-dot">.</span>
              </h1>
              <p>{{ versionSettingsTarget?.id }} · 为这个客户端设置独立启动参数，或继续跟随全局。</p>
            </div>
            <span class="version-settings-page-state">
              {{ versionSettingsDraft.custom ? '独立设置' : '跟随全局设置' }}
            </span>
          </div>
          <div v-if="versionSettingsTarget" class="version-settings-layout">
            <article class="settings-card">
              <h3>设置范围</h3>
              <label class="version-settings-toggle">
                <input type="checkbox" v-model="versionSettingsDraft.custom" />
                <span>
                  <strong>为这个版本使用独立设置</strong>
                  <small>关闭时，所有启动参数都会采用启动设置中的全局默认值。</small>
                </span>
              </label>
              <div class="version-loader-settings">
                <div class="version-loader-heading">
                  <div>
                    <h3>游戏加载器</h3>
                    <p>直接修改此客户端的 Forge、Fabric、NeoForge、Quilt 或 OptiFine 版本。</p>
                  </div>
                  <span v-if="versionSettingsItem">当前：{{ versionSettingsItem.loader }}</span>
                </div>
                <div v-if="componentLoading" class="field-help">正在读取兼容的加载器版本…</div>
                <div v-else-if="componentInfo?.error" class="field-help error-text">{{ componentInfo.error }}</div>
                <div v-else class="version-loader-controls">
                  <div>
                    <label class="field-label" for="version-component-loader">加载器</label>
                    <SearchSelect
                      id="version-component-loader"
                      label="选择游戏加载器"
                      v-model="componentLoader"
                      :options="[
                        { value: 'vanilla', label: '原版' },
                        { value: 'fabric', label: 'Fabric' },
                        { value: 'quilt', label: 'Quilt' },
                        { value: 'forge', label: 'Forge' },
                        { value: 'neoforge', label: 'NeoForge' },
                      ]"
                      @change="changeComponentLoader"
                    />
                  </div>
                  <div v-if="componentLoader !== 'vanilla'">
                    <label class="field-label" for="version-component-version">加载器版本</label>
                    <SearchSelect
                      id="version-component-version"
                      label="选择加载器版本"
                      v-model="componentVersion"
                      :options="componentVersions.map((item) => ({ value: item.id, label: `${item.name || item.id}${item.stable === false ? ' · 预览版' : ''}` }))"
                      :disabled="!componentVersions.length"
                    />
                    <small v-if="componentInfo?.components?.[componentLoader]?.error" class="field-help error-text">
                      {{ componentInfo.components[componentLoader].error }}
                    </small>
                  </div>
                  <div v-if="['vanilla', 'forge'].includes(componentLoader)" class="version-optifine-control">
                    <label class="check-row">
                      <input v-model="componentOptifine" type="checkbox" :disabled="!optifineVersions.length" />
                      <span>同时安装 OptiFine</span>
                    </label>
                    <SearchSelect
                      v-if="componentOptifine"
                      id="version-optifine-version"
                      label="选择 OptiFine 版本"
                      v-model="componentOptifineVersion"
                      :options="optifineVersions.map((item) => ({ value: item.id, label: `${item.name || item.id}${item.forge ? ` · Forge ${resolveForgeRequirement(item.forge, componentVersions)}` : ''}` }))"
                      :disabled="!optifineVersions.length"
                    />
                    <small v-if="optifineIncompatible" class="field-help error-text">该 OptiFine 需要 Forge {{ optifineForgeRequirement }}。</small>
                  </div>
                </div>
                <p class="version-loader-warning">更新前会校验兼容版本。客户端的存档、模组与配置会保留；更换加载器后，部分模组可能不兼容。</p>
                <button
                  class="primary-small"
                  :disabled="busy || running || componentApplying || componentLoading || optifineIncompatible || (componentLoader !== 'vanilla' && !componentVersions.length)"
                  @click="safely(applyVersionLoader)"
                >
                  {{ componentApplying || busy ? '正在更新加载器…' : '应用加载器到此版本' }}
                </button>
              </div>
              <div class="version-settings-fields" :class="{ inherited: !versionSettingsDraft.custom }">
                <div class="field-grid">
                  <div>
                    <label class="field-label">Java 运行环境</label>
                    <SearchSelect
                      id="version-java-choice"
                      label="版本 Java 运行环境"
                      v-model="versionSettingsDraft.javaChoice"
                      :options="versionJavaOptions"
                      :disabled="!versionSettingsDraft.custom"
                    />
                  </div>
                  <div class="version-memory-field">
                    <div class="settings-title-row memory-setting-title">
                      <label class="field-label" for="version-memory">游戏内存</label>
                      <strong>{{ `${gb(versionSettingsDraft.memoryMB)} GB` }}</strong>
                    </div>
                    <input
                      id="version-memory"
                      type="range"
                      min="1024"
                      :max="versionSettingsMemoryMax"
                      step="256"
                      v-model.number="versionSettingsDraft.memoryMB"
                      :disabled="!versionSettingsDraft.custom"
                      aria-label="此版本的游戏内存分配"
                    />
                    <div class="memory-capacity settings-memory-capacity">
                      <div class="memory-capacity-bar" aria-hidden="true">
                        <i class="memory-system" :style="memoryStyle(memoryUsed)"></i>
                        <i class="memory-game" :style="memoryStyle(versionSettingsDraft.memoryMB)"></i>
                      </div>
                      <div class="memory-capacity-labels">
                        <span>总内存 {{ gb(model.system.totalMB) }} GB · 当前可用 {{ gb(model.system.freeMB || 0) }} GB</span>
                        <span>最多 {{ gb(versionSettingsMemoryMax) }} GB</span>
                      </div>
                      <p v-if="memoryWarning(versionSettingsDraft.memoryMB)" class="memory-warning">
                        {{ memoryWarning(versionSettingsDraft.memoryMB) }}
                      </p>
                      <p class="memory-caption">绿色区域表示分配给游戏的内存；系统可用内存会定期更新。</p>
                    </div>
                  </div>
                  <div>
                    <label class="field-label">游戏目录模式</label>
                    <SearchSelect
                      id="version-isolation"
                      label="版本游戏目录模式"
                      v-model="versionSettingsDraft.isolated"
                      :options="isolationOptions"
                      :disabled="!versionSettingsDraft.custom"
                    />
                  </div>
                  <div class="field-grid field-grid-pair">
                    <div>
                      <label class="field-label" for="version-width">窗口宽度</label>
                      <input id="version-width" type="number" min="640" max="7680" step="80" v-model.number="versionSettingsDraft.width" :disabled="!versionSettingsDraft.custom" />
                    </div>
                    <div>
                      <label class="field-label" for="version-height">窗口高度</label>
                      <input id="version-height" type="number" min="360" max="4320" step="45" v-model.number="versionSettingsDraft.height" :disabled="!versionSettingsDraft.custom" />
                    </div>
                  </div>
                  <div>
                    <label class="field-label" for="version-jvm-args">附加 JVM 参数</label>
                    <input id="version-jvm-args" v-model="versionSettingsDraft.extraJvmArgs" placeholder="可留空" :disabled="!versionSettingsDraft.custom" />
                  </div>
                  <div>
                    <label class="field-label" for="version-game-args">附加游戏参数</label>
                    <input id="version-game-args" v-model="versionSettingsDraft.extraGameArgs" placeholder="可留空" :disabled="!versionSettingsDraft.custom" />
                  </div>
                </div>
                <p v-if="!versionSettingsDraft.custom" class="field-help inherited-note">
                  当前显示全局设置预览。启用独立设置后，可以只调整此客户端的选项。
                </p>
              </div>
              <div class="version-settings-page-actions">
                <button
                  v-if="versionSettingsTarget.customized"
                  class="outline-button"
                  :disabled="busy || running"
                  @click="safely(resetVersionSettings)"
                >
                  恢复跟随全局
                </button>
                <button class="primary-small" :disabled="busy || running" @click="safely(saveVersionSettings)">
                  保存版本设置
                </button>
              </div>
            </article>
            <aside class="settings-card version-settings-summary">
              <div class="eyebrow">THIS INSTANCE</div>
              <h2>{{ versionSettingsTarget.id }}</h2>
              <p>
                {{ versionSettingsDraft.custom
                  ? '此客户端使用独立配置。更改全局设置不会覆盖这些参数。'
                  : '此客户端跟随全局设置。调整全局默认值后，这里会自动采用新设置。' }}
              </p>
              <div class="version-settings-summary-list">
                <span>Java</span>
                <strong>{{ versionSettingsDraft.javaChoice === '__auto__' ? '自动匹配' : '手动选择' }}</strong>
                <span>内存</span>
                <strong>{{ (versionSettingsDraft.memoryMB / 1024).toFixed(1) }} GB</strong>
                <span>分辨率</span>
                <strong>{{ versionSettingsDraft.width }} × {{ versionSettingsDraft.height }}</strong>
              </div>
            </aside>
          </div>
        </section>
        <section
          id="view-mods"
          class="view"
          :class="{ active: currentView === 'mods' }"
        >
          <div class="page-heading">
            <div>
              <div class="eyebrow">BUILD YOUR WORLD</div>
              <h1>
                模组列表
                <span class="heading-dot">.</span>
              </h1>
              <p id="mods-subtitle">
                {{
                  settings.versionId
                    ? `${settings.versionId} · ${mods.length} 个本地模组文件`
                    : '先添加游戏目录并选择版本。'
                }}
              </p>
            </div>
            <div class="button-group">
              <button
                id="check-mod-updates"
                class="outline-button"
                :disabled="busy || running || modUpdateBusy || !mods.length"
                @click="safely(checkModUpdates)"
              >
                <svg><use href="#i-refresh" /></svg>
                {{ checkingModUpdates ? '正在检查…' : '检查更新' }}
                <span v-if="updateCount">{{ updateCount }}</span>
              </button>
              <button
                id="browse-modrinth"
                @click="selectView('downloads')"
                class="primary-small"
              >
                <svg><use href="#i-download" /></svg>
                下载模组
              </button>
              <button
                id="open-mods"
                @click="safely(() => api.openFolder('mods'))"
                class="outline-button"
              >
                <svg><use href="#i-folder" /></svg>
                打开模组文件夹
              </button>
            </div>
          </div>
          <div class="list-toolbar">
            <label class="search-field">
              <svg><use href="#i-search" /></svg>
              <input
                id="mod-search"
                v-model="modSearch"
                placeholder="搜索模组名称、ID 或文件名…"
                aria-label="搜索模组"
              />
            </label>
            <span
              id="mod-result-count"
              class="muted"
            >
              {{ `${filteredMods.length} / ${mods.length} 个文件` }}
            </span>
          </div>
          <div v-if="filteredMods.length" class="bulk-actions">
            <label class="bulk-select">
              <input
                type="checkbox"
                :checked="allVisibleModsSelected"
                @change="toggleVisibleMods"
              />
              全选当前筛选
            </label>
            <button
              v-if="selectedModNames.length"
              class="danger-button"
              :disabled="busy || running || modUpdateBusy"
              @click="safely(deleteSelectedMods)"
            >
              删除已选 {{ selectedModNames.length }} 项
            </button>
            <button
              v-if="selectedModNames.some((name) => filteredMods.some((mod) => mod.name === name && modUpdates[name]?.status === 'available'))"
              class="primary-small"
              :disabled="busy || running || modUpdateBusy"
              @click="safely(() => updateMods(filteredMods.filter((mod) => selectedModNames.includes(mod.name))))"
            >
              {{ updatingMod === '__batch__' ? '批量更新中…' : '批量更新' }}
            </button>
          </div>
          <div
            v-if="hasCheckedModUpdates || checkingModUpdates"
            class="mod-update-summary"
            role="status"
          >
            <span>
              {{
                checkingModUpdates
                  ? '正在通过下载平台识别文件并查找兼容正式版…'
                  : `检查完成 · ${updateCount} 个可更新模组`
              }}
            </span>
            <small>只匹配当前 Minecraft 版本与加载器，停用的模组也会检查。</small>
            <p
              v-for="warning in modUpdateWarnings"
              :key="warning"
              class="mod-update-warning"
            >
              {{ warning }}
            </p>
          </div>
          <div class="mod-table">
            <div class="mod-table-header">
              <span>模组文件</span>
              <span>文件大小</span>
              <span>状态</span>
              <span>管理</span>
            </div>
            <div id="mod-list">
              <div
                v-if="!filteredMods.length"
                class="empty-state"
              >
                {{ modSearch.trim() ? '没有匹配的模组文件。' : '当前版本文件夹中没有模组。' }}
              </div>
              <div
                v-for="(mod, index) in filteredMods"
                :key="mod.name"
                class="mod-row"
                :style="{ '--row-delay': `${Math.min(index, 12) * 18}ms` }"
              >
                <span class="mod-identity">
                  <input
                    type="checkbox"
                    class="mod-select-checkbox"
                    :aria-label="`选择模组 ${mod.title || mod.name}`"
                    :checked="selectedModNames.includes(mod.name)"
                    :disabled="busy || running || modUpdateBusy"
                    @change="toggleModSelection(mod.name)"
                  />
                  <img
                    v-if="mod.iconData"
                    :src="mod.iconData"
                    :alt="`${mod.title} 图标`"
                    loading="lazy"
                  />
                  <span
                    v-else
                    class="mod-icon-fallback"
                  >
                    <svg><use href="#i-box" /></svg>
                  </span>
                  <span class="mod-name-copy">
                    <strong>{{ mod.title || mod.name }}</strong>
                    <small>{{ mod.modId ? `${mod.modId} · ${mod.name}` : mod.name }}</small>
                    <small
                      v-if="modUpdates[mod.name]"
                      class="mod-update-detail"
                      :class="modUpdates[mod.name].status"
                      :title="modUpdates[mod.name].message"
                    >
                      {{
                        modUpdates[mod.name].provider === 'curseforge'
                          ? 'CurseForge · '
                          : modUpdates[mod.name].provider === 'modrinth'
                            ? 'Modrinth · '
                            : ''
                      }}
                      {{
                        modUpdates[mod.name].status === 'available'
                          ? `${modUpdates[mod.name].currentVersion || '当前版本'} → ${modUpdates[mod.name].latestVersion}`
                          : modUpdates[mod.name].message
                      }}
                    </small>
                  </span>
                </span>
                <span>{{ (mod.size / 1024 / 1024).toFixed(2) }} MB</span>
                <span :class="mod.enabled ? 'enabled' : 'disabled'">
                  {{ mod.enabled ? '已启用' : '已停用' }}
                </span>
                <span class="mod-actions">
                  <button
                    v-if="modUpdates[mod.name]?.status === 'available' || updatingMod === mod.name"
                    class="mod-update-button"
                    :disabled="busy || running || modUpdateBusy"
                    @click="safely(() => updateMod(mod))"
                  >
                    {{ updatingMod === mod.name ? '更新中…' : '更新' }}
                  </button>
                  <button
                    :disabled="busy || running || modUpdateBusy"
                    @click="safely(() => manageMod(mod, 'toggle'))"
                  >
                    {{ mod.enabled ? '停用' : '启用' }}
                  </button>
                  <button
                    class="danger"
                    :disabled="busy || running || modUpdateBusy"
                    @click="safely(() => manageMod(mod, 'delete'))"
                  >
                    删除
                  </button>
                </span>
              </div>
            </div>
          </div>
        </section>
        <DownloadCenter
          :active="currentView === 'downloads'"
          :settings="settings"
          :version="version"
          :state="state"
          :locked="busy || running"
          @changed="safely(contentChanged)"
          @notification="toast"
          @navigate="selectView"
        />
        <DownloadDetails
          :active="currentView === 'download-details'"
          :state="state"
          :speed="downloadSpeed"
          @back="selectView('downloads')"
        />
        <ServerList
          :active="currentView === 'server-list'"
          :version-id="serverListVersion"
          :locked="busy || running"
          @back="selectView('versions')"
          @notification="toast"
          @join="(address) => safely(() => launchGame(address))"
        />
        <section
          id="view-settings"
          class="view"
          :class="{ active: currentView === 'settings' }"
        >
          <div class="page-heading">
            <div>
              <div class="eyebrow">MAKE IT YOURS</div>
              <h1>
                启动设置
                <span class="heading-dot">.</span>
              </h1>
              <p>这里管理全局默认设置；未单独配置的版本会自动跟随。</p>
            </div>
            <button
              id="save-settings"
              :disabled="busy || running"
              @click="safely(saveSettings)"
              class="primary-small"
            >
              <svg><use href="#i-check" /></svg>
              保存设置
            </button>
          </div>
          <div class="settings-layout">
            <article class="settings-card">
              <h3>游戏与账户</h3>
              <label
                class="field-label"
                for="settings-root"
              >
                游戏目录
              </label>
              <div class="input-action">
                <input
                  id="settings-root"
                  v-model="draft.gameRoot"
                  readonly
                  placeholder="先添加一个已安装的游戏目录"
                />
                <button
                  id="choose-game-folder"
                  :disabled="busy || running"
                  @click="safely(chooseFolder)"
                >
                  选择文件夹
                </button>
              </div>
              <p class="field-help">可选择整合包根目录，或其中的 .minecraft 文件夹。</p>
              <div class="settings-two">
                <div>
                  <label
                    class="field-label"
                    for="settings-username"
                  >
                    离线用户名
                  </label>
                  <input
                    id="settings-username"
                    v-model="draft.username"
                    maxlength="16"
                    placeholder="Explorer"
                  />
                  <p class="field-help">3–16 位字母、数字或下划线。</p>
                </div>
                <div>
                  <label
                    class="field-label"
                    for="settings-version"
                  >
                    默认游戏版本
                  </label>
                  <VersionSelect
                    id="settings-version"
                    label="默认游戏版本"
                    v-model="draft.versionId"
                    :options="versionOptions"
                    :disabled="busy || running"
                    @change="(id) => safely(() => changeVersion(id))"
                  />
                </div>
              </div>
              <div class="isolation-row">
                <span>
                  <strong>游戏目录模式</strong>
                  <small>自动识别已有模组、配置和存档的位置</small>
                </span>
                <SearchSelect
                  id="settings-isolated"
                  label="游戏目录模式"
                  v-model="draft.isolated"
                  :options="isolationOptions"
                  :disabled="busy || running"
                />
              </div>
            </article>
            <article class="settings-card">
              <div class="settings-title-row">
                <h3>运行环境</h3>
                <button
                  id="scan-java"
                  :disabled="scanning"
                  :class="{ 'loading-pulse': scanning }"
                  @click="
                    safely(async () => {
                      const found = await scanJava(true);
                      toast(`发现 ${found.length} 个 Java 运行环境`);
                    })
                  "
                  class="link-button"
                >
                  <svg><use href="#i-refresh" /></svg>
                  重新检测
                </button>
              </div>
              <label
                class="field-label"
                for="settings-java"
              >
                Java 运行环境
              </label>
              <div class="input-action">
                <SearchSelect
                  id="settings-java"
                  v-model="draft.javaChoice"
                  label="Java 运行环境"
                  :options="javaOptions"
                  placeholder="搜索 Java 路径…"
                />
                <button
                  id="choose-java"
                  @click="safely(chooseJava)"
                >
                  手动选择
                </button>
              </div>
              <p
                class="field-help"
                id="java-path"
              >
                {{ javaPath }}
              </p>
              <div class="settings-title-row memory-setting-title">
                <label
                  class="field-label"
                  for="settings-memory"
                >
                  游戏内存
                </label>
                <strong id="settings-memory-value">{{ `${gb(draft.memoryMB)} GB` }}</strong>
              </div>
              <input
                type="range"
                id="settings-memory"
                min="1024"
                :max="draftMemoryMax"
                step="256"
                v-model.number="draft.memoryMB"
                :disabled="busy || running"
              />
              <div
                class="memory-capacity settings-memory-capacity"
                aria-live="polite"
              >
                <div
                  class="memory-capacity-bar"
                  aria-hidden="true"
                >
                  <i
                    class="memory-system"
                    :style="memoryStyle(memoryUsed)"
                  ></i>
                  <i
                    class="memory-game"
                    :style="memoryStyle(draft.memoryMB)"
                  ></i>
                </div>
                <div class="memory-capacity-labels">
                  <span>
                    总内存 {{ gb(model.system.totalMB) }} GB · 当前可用
                    {{ gb(model.system.freeMB || 0) }} GB
                  </span>
                  <span>最多 {{ gb(draftMemoryMax) }} GB</span>
                </div>
                <p
                  v-if="memoryWarning(draft.memoryMB)"
                  class="memory-warning"
                >
                  {{ memoryWarning(draft.memoryMB) }}
                </p>
                <p class="memory-caption">
                  灰色表示系统已用，绿色表示游戏分配。可用内存每 5 秒更新；实际启动仍须通过 64 位
                  Java 检查。
                </p>
              </div>
              <div class="settings-two resolution">
                <div>
                  <label
                    class="field-label"
                    for="settings-width"
                  >
                    窗口宽度
                  </label>
                  <input
                    id="settings-width"
                    v-model.number="draft.width"
                    type="number"
                    min="640"
                    max="7680"
                    value="1280"
                  />
                </div>
                <div>
                  <label
                    class="field-label"
                    for="settings-height"
                  >
                    窗口高度
                  </label>
                  <input
                    id="settings-height"
                    v-model.number="draft.height"
                    type="number"
                    min="360"
                    max="4320"
                    value="720"
                  />
                </div>
              </div>
            </article>
          </div>
          <article class="settings-card">
            <h3>附加启动参数</h3>
            <label
              class="field-label"
              for="settings-jvm-args"
            >
              附加 JVM 参数
            </label>
            <input
              id="settings-jvm-args"
              v-model="draft.extraJvmArgs"
              placeholder="例如 -Dexample=true"
            />
            <p class="field-help">内存、classpath 和游戏入口由启动器管理；参数只对当前版本生效。</p>
            <label
              class="field-label extra-args-label"
              for="settings-game-args"
            >
              附加游戏参数
            </label>
            <input
              id="settings-game-args"
              v-model="draft.extraGameArgs"
              placeholder="例如 --fullscreen"
            />
          </article>
          <div class="info-strip">
            <svg><use href="#i-info" /></svg>
            <span>
              离线账户适用于本地游玩和允许离线身份的服务器。所选游戏目录与设置自动保存在此 Windows
              用户的本地应用数据中，重启后自动恢复。
            </span>
          </div>
        </section>
        <section
          id="view-logs"
          class="view"
          :class="{ active: currentView === 'logs' }"
        >
          <div class="page-heading">
            <div>
              <div class="eyebrow">BEHIND THE ADVENTURE</div>
              <h1>
                运行日志
                <span class="heading-dot">.</span>
              </h1>
              <p>了解游戏的加载进度，或排查启动问题。</p>
            </div>
            <div class="button-group">
              <button
                id="open-logs"
                @click="safely(() => api.openFolder('logs'))"
                class="outline-button"
              >
                <svg><use href="#i-folder" /></svg>
                日志文件夹
              </button>
              <button
                id="clear-logs"
                @click="safely(clearLogs)"
                class="quiet-button"
              >
                清空显示
              </button>
            </div>
          </div>
          <div class="log-panel">
            <div class="log-toolbar">
              <span>
                <span class="status-dot"></span>
                <strong id="log-status">{{ logStatus }}</strong>
              </span>
              <label>
                <input
                  id="log-follow"
                  type="checkbox"
                  v-model="follow"
                />
                自动滚动
              </label>
            </div>
            <div
              id="log-output"
              class="log-output"
              tabindex="0"
            >
              <div
                v-if="!logs.length"
                class="log-empty"
              >
                <svg><use href="#i-terminal" /></svg>
                <strong>冒险的记录，从启动开始。</strong>
                <span>游戏启动后，运行日志会实时显示在这里。</span>
              </div>
              <div
                v-for="(entry, index) in logs"
                :key="entry.time + index"
                class="log-entry"
                :class="entry.level"
              >
                <time>
                  {{ new Date(entry.time).toLocaleTimeString('zh-CN', { hour12: false }) }}
                </time>
                {{ entry.text }}
              </div>
            </div>
          </div>
        </section>
      </main>
      <footer class="statusbar">
        <div>
          <span
            class="status-dot"
            id="global-dot"
            :class="{ error: state.phase === 'error', loading: busy }"
          ></span>
          <span id="global-status">{{ message }}</span>
        </div>
        <span id="status-version">
          {{
            version
              ? `${version.minecraft} · ${version.loader.toUpperCase()}`
              : 'XUMOB Minecraft Launcher'
          }}
        </span>
      </footer>
    </div>
  </div>
  <Transition name="download-float">
    <button
      v-if="downloadActive"
      class="download-floating-card"
      type="button"
      aria-label="查看下载详情"
      @click="selectView('download-details')"
    >
      <span class="download-floating-icon"><svg><use href="#i-download" /></svg></span>
      <span class="download-floating-copy">
        <strong>{{ state.phase === 'downloading_java' ? '正在下载 Java' : '正在下载资源' }}</strong>
        <span>{{ state.progress?.filename || state.message }}</span>
        <i class="download-floating-track"><b :style="{ width: `${downloadPercent}%` }"></b></i>
        <small>
          {{ downloadPercent }}%
          <template v-if="state.progress?.fileCount">· {{ state.progress.fileIndex || 0 }} / {{ state.progress.fileCount }} 项</template>
          <template v-if="state.progress?.unit === 'bytes' || Number.isFinite(state.progress?.downloadedBytes)">· {{ formatDownloadSpeed(downloadSpeed) }}</template>
        </small>
      </span>
      <span class="download-floating-arrow">›</span>
    </button>
  </Transition>
  <div
    id="toast"
    class="toast"
    :class="{ visible: toastVisible, error: toastError }"
    :role="toastError ? 'alert' : 'status'"
    aria-live="polite"
  >
    <span class="toast-icon" :class="{ 'has-image': toastIconUrl }">
      <img
        v-if="toastIconUrl"
        :src="toastIconUrl"
        :alt="toastTitle ? `${toastTitle} 图标` : '资源图标'"
        referrerpolicy="no-referrer"
        @error="clearToastIcon"
      />
      <svg v-else><use :href="toastError ? '#i-info' : '#i-check'" /></svg>
    </span>
    <span class="toast-content">
      <strong v-if="toastTitle">{{ toastTitle }}</strong>
      <span class="toast-copy">{{ toastText }}</span>
    </span>
    <button
      class="toast-close"
      type="button"
      aria-label="关闭通知"
      @click="dismissToast"
    >
      <svg viewBox="0 0 24 24"><path d="m6 6 12 12M18 6 6 18" /></svg>
    </button>
  </div>
  <div
    v-if="confirmation"
    class="confirm-backdrop"
    @click.self="answerConfirm(false)"
  >
    <section
      class="confirm-card"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
    >
      <div class="confirm-icon">
        <svg><use href="#i-info" /></svg>
      </div>
      <div class="confirm-copy">
        <h2 id="confirm-title">{{ confirmation.title }}</h2>
        <p>{{ confirmation.message }}</p>
      </div>
      <div class="confirm-actions">
        <button
          class="outline-button"
          @click="answerConfirm(false)"
        >
          取消
        </button>
        <button
          :class="confirmation.danger ? 'danger-button' : 'primary-small'"
          @click="answerConfirm(true)"
        >
          {{ confirmation.confirmLabel || '确定' }}
        </button>
      </div>
    </section>
  </div>
  <dialog id="check-dialog">
    <div class="dialog-heading">
      <div>
        <div class="eyebrow">READY CHECK</div>
        <h2>环境检查</h2>
      </div>
      <button
        class="dialog-close"
        id="close-check"
        @click="safely(closeCheck)"
        aria-label="关闭环境检查"
      >
        ×
      </button>
    </div>
    <div id="check-result">
      <template v-if="checkResult">
        <div
          class="check-summary"
          :class="{ failed: !checkResult.ok }"
        >
          {{ checkResult.ok ? '环境检查通过，可以启动游戏。' : '检查完成，以下项目需要处理。' }}
        </div>
        <div class="check-row">
          <span>Java 运行环境（要求 {{ checkResult.required }}+，64 位）</span>
          <strong>
            {{ checkResult.java.error || checkResult.java.label
            }}{{ checkResult.javaOk ? ' ✓' : '' }}
          </strong>
        </div>
        <div class="check-row">
          <span>游戏依赖</span>
          <strong>{{ checkResult.libraryCount }} 个</strong>
        </div>
        <div class="check-row">
          <span>资源文件</span>
          <strong>{{ checkResult.assetCount }} 个</strong>
        </div>
        <div class="check-row">
          <span>需要补全的文件</span>
          <strong>{{ checkResult.missing.length }} 个</strong>
        </div>
        <template v-if="checkResult.missing.length">
          <div class="missing-list">
            <template
              v-for="(file, index) in checkResult.missing.slice(0, 50)"
              :key="file.file"
            >
              {{ file.label }} · {{ file.reason }}
              <br v-if="index < Math.min(50, checkResult.missing.length) - 1" />
            </template>
            <template v-if="checkResult.missing.length > 50">
              <br />
              …
            </template>
          </div>
          <p class="field-help">仅补全版本配置中记录的基础依赖和资源，不会下载或修改模组。</p>
        </template>
      </template>
    </div>
    <div class="dialog-actions">
      <button
        class="outline-button"
        id="dismiss-check"
        @click="safely(closeCheck)"
      >
        关闭
      </button>
      <button
        class="primary-small"
        id="repair-files"
        :hidden="!checkResult?.missing.length"
        :disabled="repairBusy"
        @click="safely(repairFiles)"
      >
        补全基础依赖
      </button>
    </div>
  </dialog>
  <AccountDialog
    :account="model.account"
    :username="settings.username"
    :locked="busy || running"
    @changed="safely(refresh)"
    @notification="toast"
  />
</template>
