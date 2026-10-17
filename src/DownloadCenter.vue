<script setup>
import { ref, computed, watch, reactive, nextTick, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api } from './useLauncher';
import SearchSelect from './SearchSelect.vue';
import { renderDescription, contentUrl, instanceLoader, isCompatible } from './contentFormat';
import './downloads.css';
import GameDownloads from './GameDownloads.vue';
const props = defineProps({
  active: Boolean,
  settings: Object,
  version: Object,
  state: Object,
  locked: Boolean,
});
const emit = defineEmits(['changed', 'notification', 'navigate']);
const hasOpened = ref(Boolean(props.active));
watch(
  () => props.active,
  (value) => {
    if (value) hasOpened.value = true;
  },
);
const query = ref('');
const kind = ref('game');
const sort = ref('relevance');
const sortOptions = [
  { value: 'relevance', label: '相关程度' },
  { value: 'downloads', label: '下载最多' },
  { value: 'updated', label: '最近更新' },
  { value: 'newest', label: '最新发布' },
];
const hits = ref([]);
const offset = ref(0);
const total = ref(0);
const searching = ref(false);
const searched = ref(false);
const error = ref('');
const summary = ref('');
const catalogError = ref('');
const catalog = ref({ gameVersions: [], loaders: [], categories: [] });
const showSnapshots = ref(false);
const matching = ref(true);
const filters = reactive({ gameVersion: '', loader: '', category: '' });
const detailFilters = reactive({ gameVersion: '', loader: '' });
const selected = ref(null);
const versions = ref([]);
const choice = ref('');
const tab = ref('description');
const detailLoading = ref(false);
const imagePreview = ref(null);
const previewCloseButton = ref(null);
let previewTrigger = null;
const preparing = ref(false);
const installing = ref(false);
const plan = ref(null);
const historyPage = ref(0);
const brokenIcons = reactive(new Set());
let searchGeneration = 0;
let detailGeneration = 0;
let planGeneration = 0;
let catalogPromise;
let listScroll = 0;
const source = ref('modrinth');
const keyConfigured = ref(false);
const packSetup = ref(false);
const localPack = ref(null);
const dragOverlay = ref(false);
const importingPack = ref(false);
let stopDropListener;
const setupPack = computed(() => localPack.value || (selected.value ? {
  provider: source.value,
  projectId: selected.value.id,
  versionId: choice.value,
  title: selected.value.title,
  iconData: selected.value.iconUrl || '',
} : null));
const sourceName = computed(() => (source.value === 'curseforge' ? 'CurseForge' : 'Modrinth'));
const types = [
  { id: 'game', label: '游戏版本' },
  { id: 'modpack', label: '整合包' },
  { id: 'mod', label: '模组' },
  { id: 'resourcepack', label: '资源包' },
  { id: 'shader', label: '光影' },
];
const knownLoaders = {
  modpack: ['fabric', 'forge', 'neoforge', 'quilt'],
  mod: ['fabric', 'forge', 'neoforge', 'quilt'],
  resourcepack: ['minecraft'],
  shader: ['iris', 'optifine'],
};
const loaderLabels = {
  fabric: 'Fabric',
  forge: 'Forge',
  neoforge: 'NeoForge',
  quilt: 'Quilt',
  minecraft: 'Minecraft',
  iris: 'Iris',
  optifine: 'OptiFine',
};
const categoryLabels = {
  adventure: '冒险',
  technology: '科技',
  optimization: '性能优化',
  utility: '实用工具',
  decoration: '装饰',
  library: '前置 / 库',
  worldgen: '世界生成',
  magic: '魔法',
  storage: '存储',
  equipment: '装备',
  mobs: '生物',
  food: '食物',
  transportation: '交通',
  management: '管理',
  social: '社交',
  'game-mechanics': '游戏机制',
  cursed: '趣味',
  realistic: '写实',
  fantasy: '奇幻',
  simplistic: '简约',
  'vanilla-like': '原版风格',
  combat: '战斗',
  audio: '音效',
  gui: '界面',
  entities: '实体',
  fonts: '字体',
  tweaks: '调整',
  blocks: '方块',
  items: '物品',
  environment: '环境',
  potato: '低配置',
  low: '低',
  medium: '中',
  high: '高',
  screenshot: '截图',
};
const cfCategoryLabels = {
  'API and Library': '前置与库',
  Library: '前置与库',
  'World Gen': '世界生成',
  Structures: '建筑结构',
  Magic: '魔法',
  Cosmetic: '装饰',
  Performance: '性能优化',
  'Server Utility': '服务器工具',
  Storage: '存储',
  Farming: '农业',
  Food: '食物',
  Mobs: '生物',
  Dimensions: '维度',
  'Armor, Tools, and Weapons': '装备与工具',
  'Map and Information': '地图与信息',
  'Bug Fixes': '问题修复',
  Redstone: '红石',
  Miscellaneous: '其他',
  Quests: '任务',
  Adventure: '冒险',
  'Adventure and RPG': '冒险与角色扮演',
  Technology: '科技',
  Tech: '科技',
  'Utility & QoL': '实用与体验改进',
  'Small / Light': '小型 / 轻量',
  'Large / Extra Large': '大型整合包',
  Optimization: '性能优化',
  Exploration: '探索',
  'Combat / PvP': '战斗 / PvP',
};
const hasInstance = computed(() => Boolean(props.version && props.settings?.gameRoot));
const installingNow = computed(() => installing.value || props.state?.phase === 'downloading');
const blocked = computed(() => installingNow.value || preparing.value);
const chosen = computed(() => versions.value.find((v) => v.id === choice.value));
const compatible = computed(
  () =>
    hasInstance.value &&
    selected.value?.clientSide !== 'unsupported' &&
    isCompatible(chosen.value, kind.value, props.version),
);
const percent = computed(() =>
  Math.min(
    100,
    Math.round(
      ((props.state?.progress?.bytes || 0) / Math.max(1, props.state?.progress?.totalBytes || 0)) *
        100,
    ),
  ),
);
const gameOptions = computed(() => {
  const data = catalog.value.gameVersions.filter(
    (v) => showSnapshots.value || v.type === 'release' || v.version === filters.gameVersion,
  );
  return [
    ...new Set(
      [props.version?.minecraft, filters.gameVersion, ...data.map((v) => v.version)].filter(
        Boolean,
      ),
    ),
  ];
});
const categories = computed(() =>
  catalog.value.categories.filter(
    (v) => v.kind === kind.value && !(knownLoaders[kind.value] || []).includes(v.name),
  ),
);
const gameVersionSelectOptions = computed(() => [
  { value: '', label: '所有游戏版本' },
  ...gameOptions.value.map((version) => ({
    value: version,
    label: version,
    meta: `Minecraft ${version}`,
  })),
]);
const loaderSelectOptions = computed(() => [
  { value: '', label: kind.value === 'resourcepack' ? 'Minecraft 资源包' : '所有加载器' },
  ...(knownLoaders[kind.value] || []).map((loader) => ({
    value: loader,
    label: loaderLabels[loader] || loader,
  })),
]);
const categorySelectOptions = computed(() => [
  { value: '', label: '所有分类' },
  ...categories.value.map((item) => ({
    value: item.name,
    label:
      categoryLabels[item.name] ||
      (source.value === 'curseforge' ? cfCategoryLabels[item.header] || item.header : item.name) ||
      item.name,
  })),
]);
const detailGameSelectOptions = computed(() => [
  { value: '', label: '所有支持的游戏版本' },
  ...detailGames.value.map((version) => ({
    value: version,
    label: version,
    meta: `Minecraft ${version}`,
  })),
]);
const detailLoaderSelectOptions = computed(() => [
  { value: '', label: kind.value === 'resourcepack' ? 'Minecraft' : '所有支持的加载器' },
  ...detailLoaders.value.map((loader) => ({
    value: loader,
    label: loaderLabels[loader] || loader,
  })),
]);
const versionSelectOptions = computed(() =>
  versions.value.length
    ? versions.value.map((item) => ({
        value: item.id,
        label: `${item.number || item.name} · ${channel(item.channel)}`,
        meta: item.date?.slice(0, 10) || '',
      }))
    : [{ value: '', label: detailLoading.value ? '正在读取版本…' : '无匹配版本' }],
);
const filterCount = computed(() => Object.values(filters).filter(Boolean).length);
const detailGames = computed(() => {
  const supported = selected.value?.gameVersions || [];
  return [
    ...new Set([
      ...catalog.value.gameVersions.map((v) => v.version).filter((v) => supported.includes(v)),
      ...supported,
    ]),
  ];
});
const detailLoaders = computed(() =>
  knownLoaders[kind.value].filter((v) => (selected.value?.loaders || []).includes(v)),
);
const description = computed(() =>
  renderDescription(selected.value?.body || selected.value?.description, source.value),
);
const resultPage = computed(() => Math.floor(offset.value / 20) + 1);
const resultPageCount = computed(() => Math.min(Math.ceil(total.value / 20), 501));
const resultPageItems = computed(() => {
  const lastPage = resultPageCount.value;
  if (lastPage <= 9) return Array.from({ length: lastPage }, (_, index) => index + 1);

  const firstVisible = Math.max(2, resultPage.value - 2);
  const lastVisible = Math.min(lastPage - 1, resultPage.value + 2);
  const pages = [1];
  if (firstVisible > 2) pages.push('ellipsis-start');
  for (let page = firstVisible; page <= lastVisible; page++) pages.push(page);
  if (lastVisible < lastPage - 1) pages.push('ellipsis-end');
  pages.push(lastPage);
  return pages;
});
const pageVersions = computed(() =>
  versions.value.slice(historyPage.value * 20, (historyPage.value + 1) * 20),
);
const projectLinks = computed(() =>
  Object.entries(selected.value?.links || {})
    .filter(([, url]) => contentUrl(url, source.value))
    .map(([id, url]) => ({
      id,
      url,
      label: { issues: '问题反馈', source: '源代码', wiki: 'Wiki', discord: 'Discord' }[id] || id,
    })),
);
const displayError = (e) => String(e?.message || e);
function size(n) {
  return n >= 1048576 ? `${(n / 1048576).toFixed(1)} MB` : `${Math.ceil((n || 0) / 1024)} KB`;
}

function count(n) {
  return new Intl.NumberFormat('zh-CN', { notation: 'compact', maximumFractionDigits: 1 }).format(
    n || 0,
  );
}

function channel(v) {
  return { release: '正式版', beta: '测试版', alpha: '预览版' }[v] || v;
}

function icon(project) {
  return !brokenIcons.has(project?.id) && project?.iconUrl;
}

function clearPlan() {
  planGeneration++;
  plan.value = null;
  summary.value = '';
}

function clearSelection() {
  detailGeneration++;
  clearPlan();
  selected.value = null;
  versions.value = [];
  choice.value = '';
  detailLoading.value = false;
  historyPage.value = 0;
}

async function loadCatalog() {
  if (catalogPromise) return catalogPromise;
  const provider = source.value;
  catalogPromise = api
    .contentCatalog(provider)
    .then((value) => {
      if (provider !== source.value) return;
      catalog.value = value;
      catalogError.value = '';
    })
    .catch(() => {
      catalogError.value = '筛选列表暂时无法加载，可重试或匹配当前实例。';
      catalogPromise = null;
    });
  return catalogPromise;
}

function applyInstance() {
  filters.gameVersion = props.version?.minecraft || '';
  filters.loader = ['mod', 'modpack'].includes(kind.value)
    ? instanceLoader(props.version?.loader)
    : '';
  matching.value = true;
}

function applyAutomaticInstanceFilter() {
  if (kind.value === 'mod') {
    applyInstance();
    return;
  }

  filters.gameVersion = '';
  filters.loader = '';
  matching.value = false;
}

async function matchInstance() {
  if (blocked.value) return;
  applyInstance();
  await search();
}

async function changeFilter() {
  matching.value = false;
  await search();
}

async function resetFilters() {
  if (blocked.value) return;
  Object.assign(filters, { gameVersion: '', loader: '', category: '' });
  matching.value = false;
  await search();
}

async function search(next = 0) {
  if (blocked.value || kind.value === 'game') return;
  const generation = ++searchGeneration;
  searching.value = true;
  error.value = '';
  clearSelection();
  offset.value = next;
  try {
    const result = await api.searchContent(
      query.value.trim(),
      kind.value,
      next,
      sort.value,
      { ...filters },
      source.value,
    );
    if (generation !== searchGeneration) return;
    hits.value = result.hits;
    total.value = Number(result.total) || 0;
    searched.value = true;
  } catch (e) {
    if (generation === searchGeneration) {
      hits.value = [];
      total.value = 0;
      searched.value = true;
      error.value = displayError(e);
    }
  } finally {
    if (generation === searchGeneration) searching.value = false;
  }
}

function goToResultPage(page) {
  if (!Number.isInteger(page) || page < 1 || page > resultPageCount.value) return;
  if (page !== resultPage.value) search((page - 1) * 20);
}

async function setKind(value) {
  if (blocked.value || kind.value === value) return;
  kind.value = value;
  filters.category = '';
  filters.loader = '';
  if (value === 'game') {
    clearSelection();
    return;
  }
  applyAutomaticInstanceFilter();
  loadCatalog();
  keyStatus();
  await search();
}

async function selectProject(project) {
  if (blocked.value) return;
  listScroll = document.querySelector('.main-content')?.scrollTop || 0;
  clearSelection();
  selected.value = project;
  tab.value = 'description';
  error.value = '';
  Object.assign(detailFilters, { gameVersion: filters.gameVersion, loader: filters.loader });
  await nextTick();
  document.querySelector('.main-content')?.scrollTo(0, 0);
  await loadVersions();
}

async function backToResults() {
  if (blocked.value) return;
  clearSelection();
  if (!searched.value) {
    await search();
    listScroll = 0;
  }
  await nextTick();
  document.querySelector('.main-content')?.scrollTo(0, listScroll);
}

async function loadVersions() {
  if (!selected.value || blocked.value) return;
  clearPlan();
  const generation = ++detailGeneration;
  const project = selected.value;
  detailLoading.value = true;
  error.value = '';
  versions.value = [];
  choice.value = '';
  historyPage.value = 0;
  try {
    const result = await api.contentVersions(
      project.id,
      kind.value,
      { ...detailFilters },
      source.value,
    );
    if (generation !== detailGeneration) return;
    selected.value = { ...project, ...result.project };
    versions.value = result.versions;
    choice.value =
      result.versions.find((v) => v.channel === 'release')?.id || result.versions[0]?.id || '';
  } catch (e) {
    if (generation === detailGeneration) error.value = displayError(e);
  } finally {
    if (generation === detailGeneration) detailLoading.value = false;
  }
}

function selectHistory(item) {
  if (blocked.value) return;
  choice.value = item.id;
  tab.value = 'description';
}

async function prepare() {
  if (!selected.value || !choice.value || !compatible.value || props.locked) return;
  error.value = '';
  preparing.value = true;
  plan.value = null;
  const generation = ++planGeneration;
  try {
    const result = await api.prepareContent(
      selected.value.id,
      choice.value,
      kind.value,
      source.value,
    );
    if (generation === planGeneration) plan.value = result;
  } catch (e) {
    if (generation === planGeneration) error.value = displayError(e);
  } finally {
    preparing.value = false;
  }
}

async function install() {
  if (!plan.value || installingNow.value || props.locked) return;
  installing.value = true;
  error.value = '';
  summary.value = '';
  try {
    const result = await api.installContent(plan.value.planId, source.value);
    const existingSummary = result.alreadyInstalled ? `，${result.alreadyInstalled} 个已存在` : '';
    summary.value = `安装完成 · 新增 ${result.installed} 个文件${existingSummary}`;
    const resourceNotice = {
      title: selected.value?.title || localPack.value?.name || '资源安装完成',
      message: '安装完成，已加入游戏库。',
      iconUrl: selected.value?.iconUrl || localPack.value?.iconUrl || '',
    };
    plan.value = null;
    emit('changed');
    emit('notification', resourceNotice);
  } catch (e) {
    const message = displayError(e);
    summary.value = message === '下载已取消。' ? '下载已取消，未安装新文件。' : '';
    if (!summary.value) error.value = message;
    plan.value = null;
  } finally {
    installing.value = false;
  }
}

async function cancel() {
  try {
    await api.cancelContent(source.value);
    summary.value = '正在取消下载…';
  } catch (e) {
    error.value = displayError(e);
  }
}

async function openProject() {
  try {
    if (source.value === 'curseforge') {
      const projectPaths = {
        mod: 'mc-mods',
        modpack: 'modpacks',
        resourcepack: 'texture-packs',
        shader: 'shaders',
      };
      const projectPath = projectPaths[kind.value];
      const projectId = selected.value.slug || selected.value.id;
      await api.openContentLink(`https://www.curseforge.com/minecraft/${projectPath}/${projectId}`);
    } else await api.openContentPage(selected.value.id, kind.value);
  } catch (e) {
    error.value = displayError(e);
  }
}

async function openLink(url) {
  const safe = contentUrl(url, source.value);
  if (!safe) return;
  try {
    await api.openContentLink(safe);
  } catch (e) {
    error.value = displayError(e);
  }
}

function descriptionClick(event) {
  const target = event.target instanceof Element ? event.target : null;
  const image = target?.closest('img');
  if (image && event.currentTarget.contains(image) && !image.closest('a[href]')) {
    const src = contentUrl(image.getAttribute('src'), source.value);
    if (src) {
      event.preventDefault();
      previewTrigger = image;
      imagePreview.value = { src, alt: image.getAttribute('alt') || '项目截图' };
      nextTick(() => previewCloseButton.value?.focus());
    }
    return;
  }

  const link = target?.closest('a[href]');
  if (link) {
    event.preventDefault();
    if (link.getAttribute('href')) openLink(link.getAttribute('href'));
  }
}

function closeImagePreview() {
  imagePreview.value = null;
  nextTick(() => {
    if (previewTrigger?.isConnected) previewTrigger.focus?.();
    previewTrigger = null;
  });
}

async function changeSource(value) {
  if (blocked.value || preparing.value) return;
  searchGeneration++;
  source.value = value;
  if (value === 'curseforge') keyStatus();
  catalogPromise = null;
  catalog.value = { gameVersions: [], loaders: [], categories: [] };
  filters.category = '';
  clearSelection();
  hits.value = [];
  total.value = 0;
  searched.value = false;
  loadCatalog();
  await search();
}

async function keyStatus() {
  try {
    keyConfigured.value = (await api.download('key_status', {}, 'curseforge')).configured;
  } catch {}
}

function openPack() {
  if (!chosen.value || props.locked) return;
  localPack.value = null;
  packSetup.value = true;
}
async function importLocalPack(path) {
  if (importingPack.value) return;
  const extension = String(path).split(/[./\\]/).pop()?.toLowerCase();
  if (!['zip', 'mrpack'].includes(extension)) {
    emit('notification', '请导入 ZIP 或 MRPACK 格式的整合包。');
    return;
  }
  importingPack.value = true;
  error.value = '';
  try {
    const inspected = await api.download('game_inspect_pack', { archivePath: path }, 'local');
    localPack.value = {
      provider: 'local',
      archivePath: path,
      title: inspected.title,
      iconData: inspected.iconData || '',
      packProvider: inspected.provider,
      minecraft: inspected.minecraft,
      loader: inspected.loader,
      loaderVersion: inspected.loaderVersion,
    };
    kind.value = 'modpack';
    selected.value = null;
    choice.value = '';
    versions.value = [];
    plan.value = null;
    packSetup.value = true;
    emit('navigate', 'downloads');
  } catch (e) {
    emit('notification', displayError(e));
  } finally {
    importingPack.value = false;
    dragOverlay.value = false;
  }
}
async function chooseLocalPack() {
  try {
    const path = await api.chooseModpack();
    if (path) await importLocalPack(path);
  } catch (e) {
    emit('notification', displayError(e));
  }
}
function backFromPack() {
  packSetup.value = false;
  localPack.value = null;
}
onMounted(async () => {
  try {
    stopDropListener = await getCurrentWindow().onDragDropEvent(({ payload }) => {
      const paths = payload.paths || [];
      const packPath = paths.find((item) => /\.(zip|mrpack)$/i.test(item));
      if (payload.type === 'over') {
        // Native drag events can also be produced by dragging content inside the WebView.
        // Only show the import affordance for supported external archive files.
        if (packPath) {
          if (!dragOverlay.value) emit('navigate', 'downloads');
          dragOverlay.value = true;
        } else {
          dragOverlay.value = false;
        }
      } else if (payload.type === 'drop') {
        dragOverlay.value = false;
        if (packPath) importLocalPack(packPath);
        else if (paths.length) {
          emit('notification', '未识别到 ZIP 或 MRPACK 整合包，请检查文件格式。');
        }
      } else {
        dragOverlay.value = false;
      }
    });
  } catch {
    // File picker remains available if native drag/drop events are unavailable.
  }
});
onUnmounted(() => stopDropListener?.());
watch(choice, clearPlan);
watch(
  () =>
    JSON.stringify([props.settings?.gameRoot, props.settings?.versionId, props.settings?.isolated]),
  () => {
    clearPlan();
    if (matching.value && kind.value === 'mod') {
      searchGeneration++;
      searching.value = false;
      searched.value = false;
      hits.value = [];
      total.value = 0;
      applyInstance();
      if (props.active && !selected.value) search();
    }
  },
);
watch(
  () => props.active,
  (value) => {
    if (value && kind.value !== 'game') {
      loadCatalog();
      keyStatus();
      if (!searched.value && !searching.value) {
        if (matching.value && kind.value === 'mod') applyInstance();
        search();
      }
    }
  },
  { immediate: true },
);
</script>
<template>
  <section
    id="view-downloads"
    class="view"
    :class="{ active }"
  >
    <div
      v-if="!packSetup"
      class="download-top-nav"
    >
      <div
        class="download-types"
        aria-label="下载内容类型"
      >
        <button
          v-for="type in types"
          :key="type.id"
          :data-kind="type.id"
          :class="{ selected: kind === type.id }"
          :disabled="blocked || preparing"
          @click="setKind(type.id)"
        >
          {{ type.label }}
        </button>
        <button
          v-if="kind === 'modpack'"
          class="download-import-pack"
          :disabled="blocked || importingPack"
          @click="chooseLocalPack"
        >
          <svg><use href="#i-download" /></svg>
          {{ importingPack ? '正在解析…' : '导入本地整合包' }}
        </button>
      </div>
      <div
        v-if="kind !== 'game'"
        class="download-platforms"
        aria-label="内容平台"
      >
        <button
          :class="{ selected: source === 'modrinth' }"
          :disabled="blocked"
          @click="changeSource('modrinth')"
        >
          <img src="/platforms/modrinth.svg" alt="" aria-hidden="true" />
          Modrinth
        </button>
        <button
          :class="['cf', { selected: source === 'curseforge' }]"
          :disabled="blocked"
          @click="changeSource('curseforge')"
        >
          <img src="/platforms/curseforge.svg" alt="" aria-hidden="true" />
          CurseForge
        </button>
        <span
          v-if="source === 'curseforge'"
          class="cf-status"
          :class="{ ready: keyConfigured }"
        >
          <i></i>
          {{ keyConfigured ? '平台已就绪' : '发行版未配置 CurseForge' }}
        </span>
      </div>
    </div>
    <GameDownloads
      v-if="kind === 'game' && hasOpened"
      :settings="settings"
      :state="state"
      :locked="locked"
      @changed="emit('changed')"
      @notification="(text) => emit('notification', text)"
    />
    <GameDownloads
      v-else-if="packSetup"
      :pack="setupPack"
      :settings="settings"
      :state="state"
      :locked="locked"
      @back="backFromPack"
      @changed="emit('changed')"
      @notification="(text) => emit('notification', text)"
    />
    <template v-else>
      <template v-if="!selected">
        <div class="page-heading">
          <div>
            <div class="eyebrow">DISCOVER YOUR NEXT ADVENTURE</div>
            <h1>
              下载中心
              <span class="heading-dot">.</span>
            </h1>
            <p>发现下一个让你爱上 Minecraft 的模组。</p>
          </div>
          <span class="modrinth-source">
            <img
              :src="source === 'curseforge' ? '/platforms/curseforge.svg' : '/platforms/modrinth.svg'"
              alt=""
              aria-hidden="true"
            />
            {{ sourceName }}
          </span>
        </div>
        <div class="download-instance">
          <svg><use href="#i-box" /></svg>
          <div>
            <strong>{{ hasInstance ? settings.versionId : '先发现，再加入你的世界' }}</strong>
            <small>
              {{
                hasInstance
                  ? `Minecraft ${version.minecraft} · ${version.loader}`
                  : '选择一个已安装实例后，就能把内容安装进游戏。'
              }}
            </small>
          </div>
          <button
            id="match-current-instance"
            class="outline-button"
            :disabled="!hasInstance || blocked"
            @click="matchInstance"
          >
            <svg><use href="#i-check" /></svg>
            一键匹配当前实例
          </button>
        </div>

        <form
          class="download-search"
          @submit.prevent="search()"
        >
          <label class="search-field">
            <svg><use href="#i-search" /></svg>
            <input
              id="download-search"
              v-model="query"
              maxlength="120"
              placeholder="搜索项目名称或关键词…"
              :aria-label="`搜索 ${sourceName}`"
              :disabled="blocked"
            />
          </label>
          <SearchSelect
            id="download-sort"
            label="排序方式"
            v-model="sort"
            :options="sortOptions"
            :disabled="blocked"
            @change="search()"
          />
          <button
            id="search-modrinth"
            class="primary-small"
            :disabled="searching || blocked"
          >
              <span
                v-if="searching"
                class="download-search-spinner"
                aria-hidden="true"
              />
            {{ searching ? '搜索中…' : '搜索' }}
          </button>
        </form>
        <div class="download-browse-grid">
          <aside class="download-filters">
            <div class="filter-heading">
              <strong>
                筛选条件
                <span v-if="filterCount">{{ filterCount }}</span>
              </strong>
              <button
                class="link-button"
                :disabled="blocked"
                @click="resetFilters"
              >
                重置
              </button>
            </div>
            <label
              class="field-label"
              for="filter-game-version"
            >
              游戏版本
            </label>
            <SearchSelect
              id="filter-game-version"
              v-model="filters.gameVersion"
              label="游戏版本筛选"
              placeholder="搜索 Minecraft 版本…"
              :options="gameVersionSelectOptions"
              :disabled="blocked"
              @change="changeFilter"
            />
            <label class="filter-checkbox">
              <input
                v-model="showSnapshots"
                type="checkbox"
                :disabled="blocked"
              />
              显示快照与预览版本
            </label>
            <label
              class="field-label"
              for="filter-loader"
            >
              {{ kind === 'shader' ? '光影加载器' : '模组加载器' }}
            </label>
            <SearchSelect
              id="filter-loader"
              v-model="filters.loader"
              label="加载器筛选"
              placeholder="搜索加载器…"
              :options="loaderSelectOptions"
              :disabled="blocked || kind === 'resourcepack'"
              @change="changeFilter"
            />
            <label
              class="field-label"
              for="filter-category"
            >
              内容分类
            </label>
            <SearchSelect
              id="filter-category"
              v-model="filters.category"
              label="内容分类筛选"
              placeholder="搜索分类…"
              :options="categorySelectOptions"
              :disabled="blocked"
              @change="changeFilter"
            />
            <p
              v-if="matching && hasInstance"
              class="filter-caption"
            >
              <svg><use href="#i-check" /></svg>
              已匹配当前实例
            </p>
            <p
              v-else
              class="filter-caption"
            >
              切换筛选不会修改你的游戏实例。
            </p>
            <div
              v-if="catalogError"
              class="download-hint"
            >
              {{ catalogError }}
              <button
                class="link-button"
                @click="loadCatalog"
              >
                重新加载
              </button>
            </div>
            <div class="filter-note">
              <svg><use href="#i-download" /></svg>
              <strong>从发现到游玩</strong>
              <p>查看完整介绍与历史版本，选择合适的文件。必要前置会自动补全。</p>
            </div>
          </aside>
          <div class="download-results">
            <div class="download-results-heading">
              <span>
                {{ searching ? `正在连接 ${sourceName}…` : `${count(total)} 个项目` }}
                <small v-if="filters.gameVersion">· {{ filters.gameVersion }}</small>
                <small v-if="filters.loader">· {{ loaderLabels[filters.loader] }}</small>
              </span>
              <span v-if="total">第 {{ resultPage }} / {{ resultPageCount }} 页</span>
            </div>
            <div
              v-if="error"
              id="download-error"
              class="download-error"
              role="alert"
            >
              {{ error }}
            </div>
            <div
              v-if="searching"
              class="download-loading"
              role="status"
              aria-live="polite"
            >
              <div class="download-loading-label">
                <span class="download-loading-spinner" aria-hidden="true" />
                <span>正在连接 {{ sourceName }}，为你查找匹配内容…</span>
              </div>
              <div
                v-for="row in 3"
                :key="row"
                class="download-skeleton-card"
                aria-hidden="true"
              >
                <span class="download-skeleton-icon" />
                <span class="download-skeleton-copy">
                  <i class="download-skeleton-title" />
                  <i class="download-skeleton-description" />
                  <i class="download-skeleton-meta" />
                </span>
              </div>
            </div>
            <div
              v-else-if="!hits.length"
              class="download-empty"
            >
              {{ error ? '暂时无法加载，请重新搜索。' : '没有匹配的项目，试试调整筛选条件。' }}
            </div>
            <button
              v-for="(project, index) in hits"
              v-else
              :key="project.id"
              class="download-project"
              :style="{ '--row-delay': `${Math.min(index, 12) * 16}ms` }"
              :disabled="blocked"
              :data-project="project.id"
              @click="selectProject(project)"
            >
              <span class="download-project-mark">
                <img
                  v-if="icon(project)"
                  :src="project.iconUrl"
                  :alt="`${project.title} 图标`"
                  loading="lazy"
                  referrerpolicy="no-referrer"
                  @error="brokenIcons.add(project.id)"
                />
                <svg
                  v-else
                  aria-label="暂无项目图标"
                >
                  <use href="#i-box" />
                </svg>
              </span>
              <span class="download-project-copy">
                <strong>{{ project.title }}</strong>
                <span>{{ project.description }}</span>
                <small>
                  {{ project.author }}
                  <i>·</i>
                  {{ count(project.downloads) }} 次下载
                </small>
                <span class="download-tags">
                  <em
                    v-for="tag in (project.categories || []).slice(0, 5)"
                    :key="tag"
                  >
                    {{ loaderLabels[tag] || categoryLabels[tag] || cfCategoryLabels[tag] || tag }}
                  </em>
                </span>
              </span>
              <span class="download-project-arrow">→</span>
            </button>
            <div
              v-if="total > 20"
              class="download-pagination download-pagination-pages"
              aria-label="搜索结果分页"
            >
              <button
                id="download-prev"
                class="outline-button"
                :disabled="resultPage === 1 || searching || blocked"
                aria-label="上一页"
                @click="goToResultPage(resultPage - 1)"
              >
                ← 上一页
              </button>
              <nav class="download-page-numbers" aria-label="选择页码">
                <template v-for="page in resultPageItems" :key="page">
                  <span
                    v-if="typeof page === 'string'"
                    class="download-page-ellipsis"
                    aria-hidden="true"
                  >…</span>
                  <button
                    v-else
                    class="download-page-button"
                    type="button"
                    :class="{ current: page === resultPage }"
                    :disabled="searching || blocked"
                    :aria-label="`第 ${page} 页`"
                    :aria-current="page === resultPage ? 'page' : undefined"
                    @click="goToResultPage(page)"
                  >
                    {{ page }}
                  </button>
                </template>
              </nav>
              <button
                id="download-next"
                class="outline-button"
                :disabled="resultPage >= resultPageCount || searching || blocked"
                aria-label="下一页"
                @click="goToResultPage(resultPage + 1)"
              >
                下一页 →
              </button>
            </div>
          </div>
        </div>
      </template>
      <template v-else>
        <button
          id="download-back"
          class="download-back link-button"
          :disabled="blocked"
          @click="backToResults"
        >
          ← 返回搜索结果
        </button>
        <header class="project-heading">
          <span class="download-project-mark large">
            <img
              v-if="icon(selected)"
              :src="selected.iconUrl"
              :alt="`${selected.title} 图标`"
              referrerpolicy="no-referrer"
              @error="brokenIcons.add(selected.id)"
            />
            <svg
              v-else
              aria-label="暂无项目图标"
            >
              <use href="#i-box" />
            </svg>
          </span>
          <div>
            <div class="eyebrow">
              {{ sourceName.toUpperCase() }} / {{ types.find((t) => t.id === kind)?.label }}
            </div>
            <h1>{{ selected.title }}</h1>
            <p>{{ selected.description }}</p>
            <div class="project-statistics">
              <span v-if="selected.author">由 {{ selected.author }} 创作</span>
              <span>{{ count(selected.downloads) }} 次下载</span>
              <span v-if="selected.followers">{{ count(selected.followers) }} 人关注</span>
              <span v-if="selected.license?.id">
                {{ selected.license.name || selected.license.id }}
              </span>
            </div>
          </div>
          <button
            id="open-modrinth-project"
            class="quiet-button"
            @click="openProject"
          >
            {{ sourceName }} ↗
          </button>
        </header>
        <div
          v-if="error"
          id="download-error"
          class="download-error"
          role="alert"
        >
          {{ error }}
        </div>
        <div
          v-if="summary"
          id="download-summary"
          class="download-success"
          role="status"
        >
          {{ summary }}
        </div>
        <div class="project-detail-grid">
          <div class="project-content">
            <div class="project-tabs">
              <button
                :class="{ selected: tab === 'description' }"
                @click="tab = 'description'"
              >
                介绍
              </button>
              <button
                id="project-versions-tab"
                :class="{ selected: tab === 'versions' }"
                @click="tab = 'versions'"
              >
                历史版本
                <span>{{ versions.length }}</span>
              </button>
            </div>
            <div
              v-if="detailLoading && (!selected.body || tab === 'versions')"
              class="project-detail-loading"
              role="status"
              aria-live="polite"
            >
              <div class="download-loading-label">
                <span class="download-loading-spinner" aria-hidden="true" />
                <span>{{ selected.body ? '正在读取历史版本…' : '正在读取项目介绍…' }}</span>
              </div>
              <div
                v-if="!selected.body"
                class="project-description-skeleton"
                aria-hidden="true"
              >
                <i class="description-skeleton-title" />
                <i class="description-skeleton-line" />
                <i class="description-skeleton-line short" />
                <div class="description-skeleton-images">
                  <i />
                  <i />
                </div>
              </div>
            </div>
            <article
              v-else-if="tab === 'description'"
              id="project-description"
              class="project-markdown"
              @click="descriptionClick"
              v-html="description"
            ></article>
            <div
              v-else
              class="project-history"
            >
              <div class="history-caption">
                {{ versions.length }} 个版本符合右侧筛选条件
                <span>点击版本即可选择下载</span>
              </div>
              <div
                v-if="!versions.length"
                class="download-empty"
              >
                没有符合筛选条件的历史版本。
              </div>
              <button
                v-for="(item, index) in pageVersions"
                :key="item.id"
                class="history-item"
                :style="{ '--row-delay': `${Math.min(index, 12) * 16}ms` }"
                :class="{ selected: choice === item.id }"
                :disabled="blocked"
                :data-version="item.id"
                @click="selectHistory(item)"
              >
                <span>
                  <strong>{{ item.name || item.number }}</strong>
                  <small>
                    {{ item.gameVersions.join(' / ') }} ·
                    {{ item.loaders.map((l) => loaderLabels[l] || l).join(' / ') }}
                  </small>
                </span>
                <span>
                  <em :class="item.channel">{{ channel(item.channel) }}</em>
                  <small>{{ item.date?.slice(0, 10) }} · {{ size(item.size) }}</small>
                </span>
              </button>
              <div
                v-if="versions.length > 20"
                class="download-pagination"
              >
                <button
                  class="outline-button"
                  :disabled="historyPage === 0"
                  @click="historyPage--"
                >
                  上一页
                </button>
                <small>{{ historyPage + 1 }} / {{ Math.ceil(versions.length / 20) }}</small>
                <button
                  class="outline-button"
                  :disabled="(historyPage + 1) * 20 >= versions.length"
                  @click="historyPage++"
                >
                  下一页
                </button>
              </div>
            </div>
          </div>
          <aside class="download-details">
            <div class="detail-download-title">
              <svg><use href="#i-download" /></svg>
              <h2>下载到你的世界</h2>
            </div>
            <div class="detail-instance">
              <small>{{ kind === 'modpack' ? '安装方式' : '安装实例' }}</small>
              <strong>
                {{
                  kind === 'modpack'
                    ? '创建独立游戏实例'
                    : hasInstance
                      ? settings.versionId
                      : '尚未选择实例'
                }}
              </strong>
              <span v-if="hasInstance">{{ version.minecraft }} · {{ version.loader }}</span>
            </div>
            <label
              class="field-label"
              for="detail-game-version"
            >
              选择游戏版本
            </label>
            <SearchSelect
              id="detail-game-version"
              v-model="detailFilters.gameVersion"
              label="选择游戏版本"
              placeholder="搜索支持的 Minecraft 版本…"
              :options="detailGameSelectOptions"
              :disabled="blocked || detailLoading"
              @change="loadVersions"
            />
            <label
              class="field-label"
              for="detail-loader"
            >
              选择加载器
            </label>
            <SearchSelect
              id="detail-loader"
              v-model="detailFilters.loader"
              label="选择加载器"
              placeholder="搜索加载器…"
              :options="detailLoaderSelectOptions"
              :disabled="blocked || detailLoading || kind === 'resourcepack'"
              @change="loadVersions"
            />
            <label
              class="field-label"
              for="download-version"
            >
              模组版本 / 历史版本
            </label>
            <SearchSelect
              id="download-version"
              v-model="choice"
              label="模组版本和历史版本"
              placeholder="搜索版本号或发布日期…"
              :options="versionSelectOptions"
              :disabled="blocked || detailLoading"
            />
            <div
              v-if="chosen"
              class="download-version-meta"
            >
              <span>{{ chosen.loaders.map((l) => loaderLabels[l] || l).join(' / ') }}</span>
              <span>{{ size(chosen.size) }}</span>
              <span>{{ chosen.date?.slice(0, 10) }}</span>
            </div>
            <p
              v-if="!hasInstance && kind !== 'modpack'"
              class="download-hint"
            >
              可自由浏览历史版本。在游戏库选择已安装的实例后即可安装。
            </p>
            <p
              v-else-if="chosen && !compatible && kind !== 'modpack'"
              id="content-incompatible"
              class="download-hint warning"
            >
              所选文件与当前实例不兼容。请调整版本或在游戏库切换对应实例。
            </p>
            <p
              v-if="kind === 'shader'"
              class="download-hint"
            >
              在游戏中启用光影需要 Iris 或 OptiFine。
            </p>
            <button
              v-if="kind === 'modpack' && !installingNow"
              class="primary-small download-wide"
              :disabled="!chosen || locked || detailLoading"
              @click="openPack"
            >
              安装此整合包版本
            </button>
            <button
              v-else-if="!plan && !installingNow"
              id="prepare-modrinth-install"
              class="primary-small download-wide"
              :disabled="!compatible || locked || preparing || detailLoading"
              @click="prepare"
            >
              <svg><use href="#i-download" /></svg>
              {{ preparing ? '检查前置与已有文件…' : '下载此版本' }}
            </button>
            <div
              v-if="plan"
              id="download-install-plan"
              class="download-plan"
            >
              <h3>确认安装清单</h3>
              <p>
                {{ plan.files.length }} 个文件 · {{ size(plan.totalBytes) }}
                <span v-if="plan.dependencyCount">· {{ plan.dependencyCount }} 个必要前置</span>
              </p>
              <ul>
                <li
                  v-for="file in plan.files"
                  :key="file.folder + '/' + file.filename"
                  :style="{ paddingLeft: `${Math.min(file.dependencyDepth || 0, 8) * 12}px` }"
                >
                  <strong>
                    {{ file.title }}
                    <small v-if="file.dependency">
                      前置 · 第 {{ file.dependencyDepth }} 层
                    </small>
                  </strong>
                  <span>{{ file.filename }}</span>
                </li>
              </ul>
              <label class="field-label">安装到</label>
              <code>{{ plan.context.gameDir }}</code>
              <button
                v-if="!installingNow"
                id="install-modrinth-content"
                class="primary-small download-wide"
                :disabled="locked"
                @click="install"
              >
                确认下载并安装
              </button>
            </div>
            <div
              v-if="installingNow"
              class="download-progress"
              role="status"
            >
              <div>
                <strong>正在下载并校验</strong>
                <span>{{ percent }}%</span>
              </div>
              <progress
                :value="percent"
                max="100"
                aria-label="下载进度"
              ></progress>
              <p>{{ state?.message }}</p>
              <button
                id="cancel-content-download"
                class="outline-button download-wide"
                :disabled="state?.phase !== 'downloading'"
                @click="cancel"
              >
                取消下载
              </button>
            </div>
            <p
              v-if="locked && !installingNow"
              class="download-hint"
            >
              游戏正在运行或准备中，结束后即可安装。
            </p>
            <div
              v-if="projectLinks.length"
              class="project-links"
            >
              <span>了解更多</span>
              <button
                v-for="link in projectLinks"
                :key="link.id"
                class="link-button"
                @click="openLink(link.url)"
              >
                {{ link.label }} ↗
              </button>
            </div>
            <p class="download-integrity">
              <svg><use href="#i-check" /></svg>
              {{
                source === 'curseforge'
                  ? 'CurseForge 官方源 · SHA-1 校验'
                  : '官方文件源 · SHA-512 校验'
              }}
            </p>
          </aside>
        </div>
      </template>
    </template>
    <div
      v-if="imagePreview"
      class="image-lightbox"
      role="dialog"
      aria-modal="true"
      aria-label="图片预览"
      tabindex="-1"
      @click.self="closeImagePreview"
      @keydown.esc.stop.prevent="closeImagePreview"
    >
      <button
        ref="previewCloseButton"
        class="image-lightbox-close"
        type="button"
        aria-label="关闭图片预览"
        @click="closeImagePreview"
      >
        ×
      </button>
      <figure class="image-lightbox-content">
        <img
          :src="imagePreview.src"
          :alt="imagePreview.alt"
        />
        <figcaption v-if="imagePreview.alt !== '项目截图'">{{ imagePreview.alt }}</figcaption>
      </figure>
    </div>
    <Teleport to="body">
      <div v-if="dragOverlay" class="modpack-drop-overlay" role="status">
        <div class="modpack-drop-card">
          <span class="modpack-drop-icon">↓</span>
          <strong>松开以导入整合包</strong>
          <small>支持 ZIP 和 MRPACK 文件</small>
        </div>
      </div>
    </Teleport>
  </section>
</template>
