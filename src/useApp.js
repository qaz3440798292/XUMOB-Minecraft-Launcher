import {
  computed,
  reactive,
  ref,
  shallowRef,
  watch,
  nextTick,
  onMounted,
  onBeforeUnmount,
} from 'vue';
import { api, onState, onLogs, onCloseRequested } from './useLauncher';
import { useModUpdates } from './composables/useModUpdates';
import { resolveForgeRequirement } from './forgeRequirement';
export function useApp() {
  const labels = {
    home: '开始游戏',
    versions: '游戏库',
    mods: '模组列表',
    downloads: '下载中心',
    'download-details': '下载详情',
    settings: '启动设置',
    'version-settings': '版本设置',
    'server-list': '服务器',
    logs: '运行日志',
  };
  const model = shallowRef({
    globalSettings: {
      gameRoot: '',
      versionId: '',
      username: 'Explorer',
      memoryMB: 4096,
      javaPath: '',
      javaMode: 'auto',
      isolated: 'auto',
      width: 1280,
      height: 720,
      directories: [],
      extraJvmArgs: '',
      extraGameArgs: '',
    },
    settings: {
      gameRoot: '',
      versionId: '',
      username: 'Explorer',
      memoryMB: 4096,
      javaPath: '',
      javaMode: 'auto',
      isolated: 'auto',
      width: 1280,
      height: 720,
      directories: [],
      extraJvmArgs: '',
      extraGameArgs: '',
    },
    versions: [],
    suggestions: [],
    system: { totalMB: 0, maxMB: 16384 },
    state: { phase: 'idle', message: '正在读取本地配置…', pid: null },
  });
  const currentView = ref('home');
  const serverListVersion = ref('');
  const loaded = ref(false);
  const javaList = shallowRef([]);
  const mods = shallowRef([]);
  const logs = shallowRef([]);
  const modSearch = ref('');
  const versionSearch = ref('');
  const selectedVersionIds = ref([]);
  const versionBatchMode = ref(false);
  const selectedModNames = ref([]);
  const directorySearch = ref('');
  const versionLoader = ref('all');
  const confirmation = ref(null);
  const versionSettingsTarget = ref(null);
  const versionSettingsDraft = reactive({
    custom: false,
    javaChoice: '__auto__',
    memoryMB: 4096,
    isolated: 'auto',
    width: 1280,
    height: 720,
    extraJvmArgs: '',
    extraGameArgs: '',
  });
  const componentInfo = shallowRef(null);
  const componentLoading = ref(false);
  const componentApplying = ref(false);
  const componentLoader = ref('vanilla');
  const componentVersion = ref('');
  const componentOptifine = ref(false);
  const componentOptifineVersion = ref('');
  const follow = ref(true);
  const scanning = ref(false);
  const launching = ref(false);
  const repairBusy = ref(false);
  const homeMemory = ref(4096);
  const toastText = ref('');
  const toastTitle = ref('');
  const toastIconUrl = ref('');
  const toastError = ref(false);
  const toastVisible = ref(false);
  const checkResult = shallowRef(null);
  const draft = reactive({ ...model.value.settings });
  let toastTimer;
  let logFrame = null;
  let pendingLogs = [];
  let unlistenState;
  let unlistenLogs;
  let unlistenCloseRequested;
  let resolveConfirmation;
  let memoryTimer;
  let memoryRefreshing = false;
  let launchIssueMessage = '';
  let launchIssueNotified = false;
  let javaPromptHandled = false;
  let lastDownloadSample = null;
  const settings = computed(() => model.value.settings);
  const globalSettings = computed(() => model.value.globalSettings || model.value.settings);
  const state = computed(() => model.value.state);
  const downloadSpeed = ref(0);
  const version = computed(() =>
    model.value.versions.find((v) => v.id === settings.value.versionId && !v.error),
  );
  const versionSettingsItem = computed(() =>
    model.value.versions.find((v) => v.id === versionSettingsTarget.value?.id),
  );
  const componentVersions = computed(() => componentInfo.value?.components?.[componentLoader.value]?.versions || []);
  const optifineVersions = computed(() => componentInfo.value?.components?.optifine?.versions || []);
  const selectedOptifine = computed(() => optifineVersions.value.find((item) => item.id === componentOptifineVersion.value));
  const optifineForgeRequirement = computed(() =>
    resolveForgeRequirement(selectedOptifine.value?.forge, componentVersions.value),
  );
  const optifineIncompatible = computed(() => componentOptifine.value
    && componentLoader.value === 'forge'
    && selectedOptifine.value
    && Boolean(optifineForgeRequirement.value)
    && optifineForgeRequirement.value !== componentVersion.value);
  const busy = computed(() =>
    ['checking', 'repairing', 'preparing', 'stopping', 'downloading', 'downloading_java'].includes(state.value.phase),
  );
  const running = computed(() => state.value.phase === 'running');
  const modUpdateActions = useModUpdates({ settings, mods, loadMods, refresh, toast, askConfirm });
  const gb = (n) => (n / 1024).toFixed(1);
  const accountName = computed(() => {
    const account = model.value.account;
    return account?.profile?.name
      || account?.accounts?.find((item) => item.active)?.name
      || settings.value.username
      || '请登录';
  });
  const accountLabel = computed(() => {
    const mode = model.value.account?.mode;
    return mode === 'microsoft' ? '正版账户'
      : mode === 'littleskin' ? 'LittleSkin'
        : mode === 'yggdrasil' ? '外置账户' : '离线账户';
  });
  const accountModeLabel = computed(() => {
    const mode = model.value.account?.mode;
    return mode === 'microsoft' ? '微软正版模式'
      : mode === 'littleskin' ? 'LittleSkin 外置登录'
        : mode === 'yggdrasil' ? 'Yggdrasil 外置登录' : '离线模式';
  });
  const accountSkinUrl = computed(() => {
    const account = model.value.account;
    const uuid = account?.mode === 'microsoft' ? account.profile?.id?.replaceAll('-', '') : '';
    return uuid ? `https://mc-heads.net/avatar/${encodeURIComponent(uuid)}/64` : '';
  });
  const filteredVersions = computed(() => {
    const query = versionSearch.value.trim().toLocaleLowerCase();
    return model.value.versions.filter((item) => {
      const searchable =
        `${item.id} ${item.minecraft || ''} ${item.loader || ''}`.toLocaleLowerCase();
      if (query && !searchable.includes(query)) return false;
      if (versionLoader.value === 'pack') return item.modded && item.modCount > 0;
      if (versionLoader.value === 'vanilla') return !item.modded;
      if (versionLoader.value !== 'all')
        return (item.loader || '').toLocaleLowerCase().includes(versionLoader.value);
      return true;
    });
  });
  const allVisibleVersionsSelected = computed(
    () => filteredVersions.value.length > 0 && filteredVersions.value.every((item) => selectedVersionIds.value.includes(item.id)),
  );
  const allVisibleModsSelected = computed(
    () => filteredMods.value.length > 0 && filteredMods.value.every((item) => selectedModNames.value.includes(item.name)),
  );
  watch(
    () => [settings.value.gameRoot, settings.value.versionId, settings.value.isolated].join('\0'),
    () => {
      selectedVersionIds.value = [];
      versionBatchMode.value = false;
      selectedModNames.value = [];
    },
  );
  const filteredDirectories = computed(() => {
    const query = directorySearch.value.trim().toLocaleLowerCase();
    return settings.value.directories.filter((item) =>
      `${item.name} ${item.path}`.toLocaleLowerCase().includes(query),
    );
  });
  function toast(message, error = false) {
    clearTimeout(toastTimer);
    if (message && typeof message === 'object' && 'message' in message) {
      toastTitle.value = String(message.title || '');
      toastIconUrl.value = String(message.iconUrl || '');
      error = message.error ?? error;
      message = message.message;
    } else {
      toastTitle.value = '';
      toastIconUrl.value = '';
    }
    toastText.value = String(message ?? '');
    toastError.value = error;
    toastVisible.value = true;
    toastTimer = setTimeout(() => (toastVisible.value = false), error ? 6500 : 3500);
  }

  function askConfirm(options) {
    if (resolveConfirmation) resolveConfirmation(false);
    confirmation.value = options;
    return new Promise((resolve) => {
      resolveConfirmation = resolve;
    });
  }

  function answerConfirm(accepted) {
    const resolve = resolveConfirmation;
    resolveConfirmation = null;
    confirmation.value = null;
    resolve?.(accepted);
  }

  async function safely(action) {
    try {
      return await action();
    } catch (error) {
      toast(String(error?.message || error), true);
      return null;
    }
  }
  const message = computed(() =>
    loaded.value && state.value.phase === 'idle' && !version.value
      ? settings.value.gameRoot
        ? '当前目录没有可用版本，请添加已安装的游戏目录'
        : '先添加游戏目录，选择你的第一个世界'
      : state.value.message,
  );
  const directoryOptions = computed(() => {
    const items = settings.value.directories.map((d) => ({
      value: d.path,
      label: `${d.name} · ${d.path}`,
    }));
    if (settings.value.gameRoot && !items.some((d) => d.value === settings.value.gameRoot))
      items.push({ value: settings.value.gameRoot, label: settings.value.gameRoot });
    return items.length ? items : [{ value: '', label: '尚未添加游戏目录' }];
  });
  const versionOptions = computed(() =>
    model.value.versions.length
      ? model.value.versions.map((v) => ({
          value: v.id,
          label: `${v.id}${v.error ? ' · 配置异常' : ''}`,
          disabled: Boolean(v.error),
          minecraft: v.minecraft,
          loader: v.loader,
          iconData: v.iconData,
          modded: v.modded,
        }))
      : [{ value: '', label: '没有找到游戏版本', disabled: true }],
  );
  const versionFilterOptions = [
    { value: 'all', label: '全部版本' },
    { value: 'vanilla', label: '原版' },
    { value: 'fabric', label: 'Fabric' },
    { value: 'forge', label: 'Forge' },
    { value: 'neoforge', label: 'NeoForge' },
    { value: 'quilt', label: 'Quilt' },
    { value: 'optifine', label: 'OptiFine' },
    { value: 'pack', label: '含模组的整合包' },
  ];
  const javaOptions = computed(() => {
    const entries = [
      { value: '__auto__', label: '自动匹配当前版本（推荐）' },
      ...javaList.value.map((j) => ({ value: j.path, label: j.label })),
    ];
    const p = draft.javaChoice && draft.javaChoice !== '__auto__'
      ? draft.javaChoice
      : globalSettings.value.javaPath;
    if (p && !entries.some((e) => e.value.toLowerCase() === p.toLowerCase()))
      entries.unshift({ value: p, label: `已选择 · ${p.split(/[\\/]/).slice(-3, -1).join('/')}` });
    return entries;
  });
  const versionJavaOptions = computed(() => {
    const entries = [
      { value: '__auto__', label: '自动匹配当前版本（推荐）' },
      ...javaList.value.map((j) => ({ value: j.path, label: j.label })),
    ];
    const p = versionSettingsDraft.javaChoice;
    if (p && p !== '__auto__' && !entries.some((e) => e.value.toLowerCase() === p.toLowerCase()))
      entries.unshift({ value: p, label: `已选择 · ${p.split(/[\\/]/).slice(-3, -1).join('/')}` });
    return entries;
  });
  function compareJavaSuitability(a, b) {
    const rank = (java) => {
      const major = Number(java.major || 0);
      const preferred = new Map([[21, 0], [17, 1], [8, 2], [25, 3]]);
      return [java.isJdk === false ? 1 : 0, preferred.get(major) ?? 10 + major];
    };
    const left = rank(a);
    const right = rank(b);
    return left[0] - right[0] || left[1] - right[1] || a.major - b.major;
  }
  const selectedJava = computed(() => {
    if (settings.value.javaMode === 'manual')
      return javaList.value.find(
        (j) => j.path.toLowerCase() === settings.value.javaPath.toLowerCase(),
      );
    const candidates = javaList.value.filter((j) => j.bits === 64);
    const recommended = version.value?.javaRecommended || version.value?.javaRequired || 8;
    const eligible = candidates
      .filter(
        (j) =>
          j.major >= (version.value?.javaRequired || 8) &&
          (!version.value?.javaMaximum || j.major <= version.value.javaMaximum),
      )
      .sort(compareJavaSuitability);
    return (
      eligible.find((j) => j.major === recommended) ||
      eligible[0] ||
      candidates.sort(compareJavaSuitability)[0]
    );
  });
  const memoryLimitFor = (java) =>
    Math.max(1024, Math.min(model.value.system.maxMB || 1024, java?.bits === 32 ? 1536 : Infinity));
  const memoryMax = computed(() => memoryLimitFor(selectedJava.value));
  const versionSettingsMemoryMax = computed(() => {
    if (versionSettingsDraft.javaChoice !== '__auto__') {
      return memoryLimitFor(
        javaList.value.find((java) => java.path.toLowerCase() === versionSettingsDraft.javaChoice.toLowerCase()),
      );
    }
    const targetVersion = model.value.versions.find((item) => item.id === versionSettingsTarget.value?.id);
    const candidates = javaList.value.filter((java) => java.bits === 64);
    const recommended = targetVersion?.javaRecommended || targetVersion?.javaRequired || 8;
    const eligible = candidates
      .filter((java) =>
        java.major >= (targetVersion?.javaRequired || 8) &&
        (!targetVersion?.javaMaximum || java.major <= targetVersion.javaMaximum),
      )
      .sort(compareJavaSuitability);
    const java = eligible.find((item) => item.major === recommended) || eligible[0];
    return memoryLimitFor(java);
  });
  const draftJava = computed(() =>
    draft.javaChoice === '__auto__'
      ? selectedJava.value
      : javaList.value.find((j) => j.path.toLowerCase() === (draft.javaChoice || '').toLowerCase()),
  );
  const draftMemoryMax = computed(() => memoryLimitFor(draftJava.value));
  const memoryStyle = (value) => ({
    width: `${Math.min(100, Math.max(0, (Number(value) / (model.value.system.totalMB || 1)) * 100))}%`,
  });
  const memoryUsed = computed(() =>
    Math.max(0, (model.value.system.totalMB || 0) - (model.value.system.freeMB || 0)),
  );
  const memoryWarning = (value) =>
    Number(value) > (model.value.system.recommendedMB ?? model.value.system.freeMB ?? 0)
      ? '当前可用内存不足以保留系统余量，建议降低分配或关闭其他程序。'
      : '';
  const memoryNote = computed(() =>
    selectedJava.value?.bits === 32
      ? '当前选择 32 位 Java，最多分配 1.5 GB；本启动器运行游戏需要 64 位 Java。'
      : selectedJava.value
        ? '已按物理内存预留系统空间。'
        : 'Java 检测后还会检查运行环境限制。',
  );
  async function refreshMemory() {
    if (memoryRefreshing || !loaded.value) return;
    memoryRefreshing = true;
    try {
      const memory = await api.memory();
      model.value = { ...model.value, system: memory };
    } catch {
    } finally {
      memoryRefreshing = false;
    }
  }
  watch(memoryMax, (max) => {
    if (homeMemory.value > max) homeMemory.value = max;
  });
  watch(draftMemoryMax, (max) => {
    if (draft.memoryMB > max) draft.memoryMB = max;
  });
  watch(versionSettingsMemoryMax, (max) => {
    if (versionSettingsDraft.custom && versionSettingsDraft.memoryMB > max)
      versionSettingsDraft.memoryMB = max;
  });
  const javaSummary = computed(() =>
    selectedJava.value
      ? `Java ${selectedJava.value.major} · ${selectedJava.value.bits} 位`
      : settings.value.javaMode === 'auto'
        ? '自动匹配 Java'
        : settings.value.javaPath
          ? '已选择本地 Java'
          : '未选择 Java',
  );
  const javaDetail = computed(() =>
    selectedJava.value
      ? `${settings.value.javaMode === 'auto' ? '自动选择 · ' : ''}${selectedJava.value.version}`
      : '按版本要求匹配，或手动选择',
  );
  const javaPath = computed(() =>
    draft.javaChoice !== '__auto__'
      ? draft.javaChoice || '请手动选择 Java。'
      : selectedJava.value
        ? `自动匹配：${selectedJava.value.path}`
        : `当前版本建议使用 64 位 JDK Java ${version.value?.javaRecommended || '8、17 或 21'}；启动器会按版本要求自动匹配。`,
  );
  const filteredMods = computed(() =>
    mods.value.filter((mod) =>
      `${mod.title || ''} ${mod.modId || ''} ${mod.name}`
        .toLowerCase()
        .includes(modSearch.value.trim().toLowerCase()),
    ),
  );
  const launchLabel = computed(() =>
    running.value ? '停止游戏' : busy.value ? '正在准备…' : '启动游戏',
  );
  const logStatus = computed(() =>
    running.value
      ? `游戏进程 ${state.value.pid}`
      : state.value.phase === 'error'
        ? '出现启动问题'
        : busy.value
          ? '准备中'
          : '等待启动',
  );
  function syncDraft() {
    Object.assign(draft, globalSettings.value, {
      isolated: String(globalSettings.value.isolated),
      javaChoice: globalSettings.value.javaMode === 'auto' ? '__auto__' : globalSettings.value.javaPath,
    });
    homeMemory.value = Math.min(settings.value.memoryMB, memoryMax.value);
    draft.memoryMB = Math.min(draft.memoryMB, draftMemoryMax.value);
  }

  async function refresh() {
    model.value = await api.snapshot();
    if (!version.value && model.value.versions.some((v) => !v.error) && !busy.value) {
      await api.save({ versionId: model.value.versions.find((v) => !v.error).id });
      model.value = await api.snapshot();
    }
    syncDraft();
    loaded.value = true;
    return model.value;
  }

  async function scanJava(force = false) {
    scanning.value = true;
    try {
      javaList.value = await api.java(force);
      return javaList.value;
    } finally {
      scanning.value = false;
    }
  }

  async function changeVersion(id) {
    await api.save({ versionId: id });
    await refresh();
    await scanJava();
  }

  async function changeDirectory(root) {
    await api.save({ gameRoot: root });
    await refresh();
    await scanJava();
  }

  async function addDirectory(root) {
    model.value = await api.addDirectory(root);
    syncDraft();
    loaded.value = true;
    await scanJava();
  }

  async function chooseFolder() {
    const root = await api.chooseFolder();
    if (!root) return;
    await addDirectory(root);
    toast(
      model.value.versions.length
        ? `已读取 ${model.value.versions.length} 个游戏版本`
        : '这个目录没有已安装的 Minecraft 版本',
      !model.value.versions.length,
    );
  }

  async function saveSettings() {
    const automatic = draft.javaChoice === '__auto__';
    await api.save({
      gameRoot: draft.gameRoot,
      versionId: draft.versionId,
      username: draft.username.trim(),
      javaMode: automatic ? 'auto' : 'manual',
      javaPath: automatic ? '' : draft.javaChoice,
      memoryMB: Number(draft.memoryMB),
      isolated: draft.isolated === 'auto' ? 'auto' : draft.isolated === 'true',
      width: Number(draft.width),
      height: Number(draft.height),
      extraJvmArgs: draft.extraJvmArgs,
      extraGameArgs: draft.extraGameArgs,
    });
    await refresh();
    toast('设置已保存');
  }

  function onHomeMemory(event) {
    homeMemory.value = Number(event.target.value);
    draft.memoryMB = homeMemory.value;
  }

  async function saveHomeMemory() {
    if (version.value?.hasCustomSettings) {
      await api.saveVersionSettings(settings.value.versionId, true, {
        ...settings.value,
        memoryMB: homeMemory.value,
      });
      await refresh();
      return;
    }
    const next = await api.save({ memoryMB: homeMemory.value });
    model.value = {
      ...model.value,
      settings: next,
      globalSettings: { ...globalSettings.value, memoryMB: homeMemory.value },
    };
  }

  async function openVersionSettings(id) {
    if (busy.value || running.value) throw new Error('游戏运行或准备期间不能修改版本设置。');
    const result = await api.versionSettings(id);
    componentLoading.value = true;
    componentInfo.value = null;
    try {
      componentInfo.value = await api.gameVersionInfo(id);
    } catch (error) {
      componentInfo.value = { error: String(error?.message || error), components: {} };
    } finally {
      componentLoading.value = false;
    }
    if (!javaList.value.length) await scanJava();
    const effective = result.effective;
    versionSettingsTarget.value = { id, customized: result.customized };
    Object.assign(versionSettingsDraft, {
      custom: result.customized,
      javaChoice: effective.javaMode === 'auto' ? '__auto__' : effective.javaPath,
      memoryMB: effective.memoryMB,
      isolated: String(effective.isolated),
      width: effective.width,
      height: effective.height,
      extraJvmArgs: effective.extraJvmArgs || '',
      extraGameArgs: effective.extraGameArgs || '',
    });
    const currentLoader = versionSettingsItem.value?.loader || '';
    componentLoader.value = ['neoforge', 'forge', 'fabric', 'quilt'].find((kind) => currentLoader.toLowerCase().includes(kind)) || 'vanilla';
    const currentLoaderVersion = currentLoader.match(/(?:NeoForge|Forge|Fabric|Quilt)\s+([^+]+)/i)?.[1]?.trim() || '';
    const loaderVersionId = currentLoaderVersion.startsWith(`${versionSettingsItem.value?.minecraft}-`)
      ? currentLoaderVersion.slice(versionSettingsItem.value.minecraft.length + 1)
      : currentLoaderVersion;
    componentVersion.value = componentInfo.value?.components?.[componentLoader.value]?.versions?.find((item) => item.id === loaderVersionId)?.id
      || componentInfo.value?.components?.[componentLoader.value]?.versions?.[0]?.id || '';
    componentOptifine.value = currentLoader.toLowerCase().includes('optifine');
    const currentOptifine = currentLoader.match(/OptiFine\s+([^+]+)/i)?.[1]?.trim();
    const optifineVersionId = currentOptifine?.startsWith(`${versionSettingsItem.value?.minecraft}_`)
      ? currentOptifine.slice(versionSettingsItem.value.minecraft.length + 1).replaceAll('_', '/')
      : currentOptifine;
    componentOptifineVersion.value = optifineVersions.value.find((item) => item.id === optifineVersionId || item.name === currentOptifine)?.id
      || optifineVersions.value[0]?.id || '';
    await selectView('version-settings');
  }

  async function openServerList(id) {
    if (id !== settings.value.versionId) await changeVersion(id);
    serverListVersion.value = id;
    await selectView('server-list');
  }

  function changeComponentLoader(kind) {
    componentLoader.value = kind;
    componentVersion.value = componentInfo.value?.components?.[kind]?.versions?.[0]?.id || '';
    if (!['vanilla', 'forge'].includes(kind)) componentOptifine.value = false;
  }

  async function applyVersionLoader() {
    const item = versionSettingsItem.value;
    if (!item) throw new Error('找不到当前客户端。');
    if (componentLoader.value !== 'vanilla' && !componentVersion.value) throw new Error('这个 Minecraft 版本暂时没有可用的加载器版本。');
    if (componentOptifine.value && !componentOptifineVersion.value) throw new Error('这个 Minecraft 版本暂时没有可用的 OptiFine 版本。');
    if (optifineIncompatible.value) throw new Error('所选 OptiFine 版本与 Forge 版本不兼容。');
    componentApplying.value = true;
    try {
      const plan = await api.download('game_prepare', {
        name: item.id,
        gameRoot: settings.value.gameRoot,
        minecraft: item.minecraft,
        loader: componentLoader.value,
        loaderVersion: componentLoader.value === 'vanilla' ? '' : componentVersion.value,
        optifine: componentOptifine.value ? componentOptifineVersion.value : '',
        fabricApi: '',
        modifyVersion: true,
      });
      await api.download('game_install', { planId: plan.planId });
      await refresh();
      componentInfo.value = await api.gameVersionInfo(item.id);
      toast(`已更新 ${item.id} 的加载器配置`);
    } finally {
      componentApplying.value = false;
    }
  }

  function closeVersionSettings() {
    versionSettingsTarget.value = null;
    selectView('versions');
  }

  async function saveVersionSettings() {
    const target = versionSettingsTarget.value;
    if (!target) return;
    const automatic = versionSettingsDraft.javaChoice === '__auto__';
    const result = await api.saveVersionSettings(target.id, versionSettingsDraft.custom, {
      javaMode: automatic ? 'auto' : 'manual',
      javaPath: automatic ? '' : versionSettingsDraft.javaChoice,
      memoryMB: Number(versionSettingsDraft.memoryMB),
      isolated: versionSettingsDraft.isolated === 'auto'
        ? 'auto'
        : versionSettingsDraft.isolated === 'true',
      width: Number(versionSettingsDraft.width),
      height: Number(versionSettingsDraft.height),
      extraJvmArgs: versionSettingsDraft.extraJvmArgs,
      extraGameArgs: versionSettingsDraft.extraGameArgs,
    });
    model.value = {
      ...model.value,
      globalSettings: result.globalSettings,
      versions: model.value.versions.map((item) =>
        item.id === target.id ? { ...item, hasCustomSettings: result.customized } : item,
      ),
    };
    if (target.id === settings.value.versionId) await refresh();
    versionSettingsTarget.value = { ...target, customized: result.customized };
    const effective = result.effective;
    Object.assign(versionSettingsDraft, {
      custom: result.customized,
      javaChoice: effective.javaMode === 'auto' ? '__auto__' : effective.javaPath,
      memoryMB: effective.memoryMB,
      isolated: String(effective.isolated),
      width: effective.width,
      height: effective.height,
      extraJvmArgs: effective.extraJvmArgs || '',
      extraGameArgs: effective.extraGameArgs || '',
    });
    toast(result.customized ? `已保存“${target.id}”的独立设置` : `“${target.id}”将跟随全局设置`);
  }

  async function resetVersionSettings() {
    versionSettingsDraft.custom = false;
    await saveVersionSettings();
  }

  async function chooseJava() {
    const javaPath = await api.chooseJava();
    if (javaPath) {
      await api.save({ javaPath });
      await refresh();
      await scanJava();
      toast('Java 已更新');
    }
  }

  async function loadMods() {
    mods.value = await api.mods();
    try {
      const result = await api.modMetadata();
      const metadata = new Map((result?.mods || []).map((item) => [item.name, item]));
      mods.value = mods.value.map((mod) => ({ ...mod, ...(metadata.get(mod.name) || {}) }));
    } catch {
      // Local mod management remains available when either project platform is offline.
    }
  }

  async function contentChanged() {
    modUpdateActions.clearModUpdates();
    await refresh();
    await loadMods();
  }

  async function selectView(view) {
    currentView.value = view;
    await nextTick();
    document.querySelector('.main-content').scrollTop = 0;
    if (view === 'mods') safely(loadMods);
    if (view === 'logs' && follow.value) scrollLogs();
  }

  function editAccount() {
    window.dispatchEvent(new Event('launcher:account-open'));
  }

  function scrollLogs() {
    const out = document.getElementById('log-output');
    if (out) out.scrollTop = out.scrollHeight;
  }

  function queueLogs(entries) {
    if (Array.isArray(entries) && (launching.value || running.value)) {
      for (const entry of entries) {
        const line = String(entry?.text || '');
        const forgeJavaIssue = parseForgeJava8Issue(line);
        if (forgeJavaIssue) {
          launchIssueMessage = forgeJavaIssue.message;
          if (!launchIssueNotified) {
            launchIssueNotified = true;
            toast(forgeJavaIssue.message, true);
          }
          if (forgeJavaIssue.canInstall && !javaPromptHandled) {
            javaPromptHandled = true;
            void safely(() => offerJavaInstall(forgeJavaIssue));
          }
          break;
        }
        const mismatch = parseJavaMismatch(line);
        if (mismatch) {
          launchIssueMessage = mismatch.message;
          if (!javaPromptHandled) {
            javaPromptHandled = true;
            launchIssueNotified = true;
            void safely(() => offerJavaInstall(mismatch));
          }
          break;
        }
        const message = summarizeLaunchIssue(line);
        if (message) {
          launchIssueMessage = message;
          if (!launchIssueNotified) {
            launchIssueNotified = true;
            toast(message, true);
          }
          break;
        }
      }
    }
    pendingLogs.push(...entries);
    if (pendingLogs.length > 2500) pendingLogs = pendingLogs.slice(-2500);
    if (logFrame !== null) return;
    logFrame = requestAnimationFrame(async () => {
      logFrame = null;
      logs.value = [...logs.value, ...pendingLogs.splice(0)].slice(-1500);
      await nextTick();
      if (follow.value) scrollLogs();
    });
  }

  function parseJavaMismatch(line) {
    const modRequirement = line.match(
      /Mod\s+['"]?([^'"\s]+)['"]?[\s\S]{0,240}?requires[\s\S]{0,100}?version\s*(\d+)[\s\S]{0,120}?(?:but|已安装|currently)[\s\S]{0,100}?version\s*(\d+)/i,
    );
    if (modRequirement) {
      const [, mod, needed, current] = modRequirement;
      return {
        mod,
        major: Number(needed),
        current: Number(current),
        message: `模组 ${mod} 要求 Java ${needed}，当前使用 Java ${current}。`,
      };
    }
    const classVersion = line.match(/Unsupported class file major version\s+(\d+)/i);
    if (classVersion) {
      const major = Number(classVersion[1]) - 44;
      if (major > 0)
        return {
          major,
          current: selectedJava.value?.major,
          message: `游戏文件需要 Java ${major}，当前 Java 版本不兼容。`,
        };
    }
    return null;
  }

  function java8Update(java) {
    const match = String(java?.version || '').match(/(?:1\.8\.0[_-]|\b8[._])(\d{3,})/i);
    return match ? Number(match[1]) : null;
  }

  function forgeVersion(versionInfo) {
    const match = String(versionInfo?.loader || '').match(/\bForge\s+(\d+)\.(\d+)\.(\d+)\b/i);
    return match ? match.slice(1).map(Number) : null;
  }

  function isAffectedForge(versionInfo) {
    const forge = forgeVersion(versionInfo);
    if (!forge) return false;
    if (!/^1\.16(?:\.|$)/.test(String(versionInfo?.minecraft || ''))) return false;
    const before = (a, b) => {
      for (let index = 0; index < 3; index += 1) {
        if (a[index] !== b[index]) return a[index] < b[index];
      }
      return false;
    };
    return !before(forge, [34, 1, 27]) && before(forge, [36, 2, 26]);
  }

  function legacyForgeJava8Compatibility(versionInfo, java) {
    const update = java8Update(java);
    if (
      java?.major !== 8 ||
      update === null ||
      update < 321 ||
      !isAffectedForge(versionInfo)
    )
      return null;
    const forge = String(versionInfo.loader.match(/\bForge\s+([\d.]+)/i)?.[1] || '旧版 Forge');
    const message = `检测到 Forge ${forge} 与 Java 8u${update} 不兼容，可能导致 ManifestEntryVerifier 方法错误。`;
    return {
      major: 8,
      release: '8.312.07.1',
      message,
      promptTitle: 'Forge 与 Java 版本不兼容',
      promptMessage: `${message}\n建议优先将 Forge 更新到 36.2.26 或更高版本。若要保留当前 Forge，可下载 Amazon Corretto Java 8u312（旧版运行时，之后应尽快更新 Forge），并仅应用到当前游戏版本。`,
      confirmLabel: '下载兼容版 Java 8u312',
      successMessage: '已安装 Amazon Corretto Java 8u312，并仅设置到当前游戏版本。',
    };
  }

  function parseForgeJava8Issue(line) {
    if (!/NoSuchMethodError[\s\S]*ManifestEntryVerifier|ManifestEntryVerifier\.\<init\>/i.test(line))
      return null;
    const compatibility = legacyForgeJava8Compatibility(version.value, selectedJava.value);
    if (compatibility) return { ...compatibility, canInstall: true };
    const forge = forgeVersion(version.value);
    const forgeLabel = forge ? `Forge ${forge.join('.')}` : '当前 Forge';
    return {
      canInstall: false,
      message: `启动失败：检测到 ManifestEntryVerifier 方法错误，可能是 ${forgeLabel} 与 Java 更新版本不兼容。建议将 Forge 更新到 36.2.26 或更高版本；如果仍失败，请检查模组兼容性。`,
    };
  }

  async function offerJavaInstall(mismatch) {
    const versionId = settings.value.versionId;
    const accepted = await askConfirm({
      title: mismatch.promptTitle || `需要 Java ${mismatch.major}（64 位 JDK）`,
      message:
        mismatch.promptMessage ||
        `${mismatch.message}\n建议使用符合版本要求的 64 位 JDK。是否从 Amazon Corretto 下载 Java ${mismatch.major} 并仅应用到当前游戏版本？`,
      confirmLabel: mismatch.confirmLabel || `下载并设置 Java ${mismatch.major}`,
    });
    if (!accepted) {
      toast(mismatch.message, true);
      return false;
    }
    try {
      if (running.value) {
        await api.stop();
        for (let attempt = 0; attempt < 50; attempt += 1) {
          const snapshot = await api.snapshot();
          if (!['running', 'stopping'].includes(snapshot.state?.phase)) break;
          await new Promise((resolve) => setTimeout(resolve, 100));
        }
      }
      const javaPath = await api.downloadJava(mismatch.major, mismatch.release);
      await scanJava(true);
      await api.saveVersionSettings(versionId, true, {
        javaMode: 'manual',
        javaPath,
      });
      await refresh();
      await scanJava(true);
      toast(mismatch.successMessage || `已安装 Amazon Corretto Java ${mismatch.major}，并设置为当前游戏版本。`);
      return true;
    } catch (error) {
      toast(`Java ${mismatch.major} 安装失败：${String(error?.message || error).slice(0, 140)}`, true);
      return false;
    }
  }

  function summarizeLaunchIssue(line) {
    const javaRequirement = line.match(
      /Mod\s+['"]?([^'"\s]+)['"]?[\s\S]{0,240}?requires[\s\S]{0,100}?version\s*(\d+)[\s\S]{0,120}?(?:but|已安装|currently)[\s\S]{0,100}?version\s*(\d+)/i,
    );
    if (javaRequirement) {
      return `启动失败：模组 ${javaRequirement[1]} 要求 Java ${javaRequirement[2]}，当前使用 Java ${javaRequirement[3]}。请为该版本切换 Java。`;
    }
    const incompatibleJava = line.match(/Unsupported class file major version\s+(\d+)/i);
    if (incompatibleJava) {
      const requiredJava = Number(incompatibleJava[1]) - 44;
      return requiredJava > 0
        ? `启动失败：游戏文件与当前 Java 版本不兼容（需要 Java ${requiredJava}）。请调整该版本的 Java 设置。`
        : '启动失败：游戏文件与当前 Java 版本不兼容，请调整该版本的 Java 设置。';
    }
    if (/Incompatible mods found|不兼容的模组/i.test(line)) {
      const javaConflict = line.match(/(?:requires|需要)[^\n]{0,90}?Java\s*(\d+)[^\n]{0,120}?(?:but|current|当前|已安装)[^\n]{0,70}?Java\s*(\d+)/i);
      if (javaConflict)
        return `启动失败：模组要求 Java ${javaConflict[1]}，当前 Java ${javaConflict[2]} 不符合要求。请在版本设置中切换到模组要求的 Java。`;
      const modName = line.match(/(?:Mod|模组)\s+['“]?([^'”\s:]+)['”]?[^\n]{0,180}?(?:requires|needs|需要|要求|depends on|依赖)/i);
      return modName
        ? `启动失败：模组“${modName[1]}”的依赖或版本不兼容。请检查它支持的 Minecraft 版本和加载器，并更新或移除冲突模组。`
        : '启动失败：检测到模组或加载器版本冲突。请检查报错中指出的模组、Minecraft 版本和加载器是否匹配。';
    }
    if (/NoSuchMethodError|NoClassDefFoundError|ClassNotFoundException/i.test(line))
      return '启动失败：模组或加载器调用了当前环境中不存在的类或方法，通常是版本不匹配。请根据运行日志更新或移除相关模组。';
    if (/ModLoadingException|ModLoadingError|Mod resolution failed|missing or unsupported mandatory dependenc|缺少.*依赖|依赖.*缺失/i.test(line))
      return '启动失败：模组依赖缺失或版本不兼容。请按日志提示安装依赖，或更新、移除对应模组。';
    if (/Mixin apply failed|InjectionError|Mixin.*failed/i.test(line))
      return '启动失败：模组注入游戏代码时发生冲突。请更新相关模组，或暂时移除最近添加的模组。';
    if (/OutOfMemoryError|Could not reserve enough space for object heap/i.test(line)) {
      return '启动失败：Java 内存不足，请在启动设置中增加游戏内存。';
    }
    if (/UnsupportedClassVersionError/i.test(line)) {
      return '启动失败：模组或游戏文件需要不同的 Java 版本，请调整该版本的 Java 设置。';
    }
    return '';
  }

  async function checkEnvironment() {
    checkResult.value = await api.check();
    await nextTick();
    const dialog = document.getElementById('check-dialog');
    if (!dialog.open) dialog.showModal();
  }

  function closeCheck() {
    document.getElementById('check-dialog').close();
  }

  async function repairFiles() {
    repairBusy.value = true;
    try {
      await api.repair();
      toast('基础依赖已补全');
      await checkEnvironment();
    } finally {
      repairBusy.value = false;
    }
  }

  async function launchGame(serverAddress = null) {
    if (running.value) {
      const accepted = await askConfirm({
        title: '停止游戏',
        message: '请先在游戏内保存进度。直接结束游戏可能会丢失尚未保存的内容。',
        confirmLabel: '停止游戏',
        danger: true,
      });
      if (!accepted) return;
      await api.stop();
      await refresh();
      toast('正在结束游戏进程');
      return;
    }
    if (launching.value) return;
    launching.value = true;
    javaPromptHandled = false;
    try {
      await scanJava(true);
      let java = selectedJava.value;
      const forgeJavaIssue = legacyForgeJava8Compatibility(version.value, java);
      if (forgeJavaIssue) {
        javaPromptHandled = true;
        const installed = await offerJavaInstall(forgeJavaIssue);
        if (!installed) return;
        await scanJava(true);
        await refresh();
        java = selectedJava.value;
      }
      const required = version.value?.javaRequired || 8;
      const maximum = version.value?.javaMaximum;
      const unsupported =
        !java || java.bits !== 64 || java.major < required || (maximum && java.major > maximum);
      if (unsupported) {
        const recommended = version.value?.javaRecommended || required;
        const javaMajor = java && maximum && java.major > maximum ? maximum : recommended;
        const reason = java && java.bits !== 64
          ? `当前 Java 是 ${java.bits} 位，Minecraft 需要 64 位；建议安装 64 位 Java ${javaMajor}。`
          : java && maximum && java.major > maximum
          ? `此版本最高兼容 Java ${maximum}，当前使用 Java ${java.major}。`
          : java && java.major < required
            ? `此版本至少需要 Java ${required}，当前使用 Java ${java.major}；建议选择兼容性更好的 Java ${javaMajor}。`
            : `没有找到满足 Java ${required}+ 要求的 64 位 JDK；建议安装 Java ${javaMajor}。`;
        const installed = await offerJavaInstall({
          major: javaMajor,
          current: java?.major,
          message: reason,
        });
        if (!installed) return;
        javaPromptHandled = false;
        await scanJava(true);
        await refresh();
        java = selectedJava.value;
        if (!java || java.bits !== 64 || java.major < required || (maximum && java.major > maximum)) return;
      }
      await api.save({ versionId: settings.value.versionId, memoryMB: homeMemory.value });
      launchIssueMessage = '';
      launchIssueNotified = false;
      const result = await api.launch(serverAddress);
      await refresh();
      if (!launchIssueMessage) {
        toast(serverAddress
          ? `游戏已启动，正在连接 ${serverAddress}`
          : `游戏进程已启动 · PID ${result.pid}`);
      }
    } catch (error) {
      const detail = String(error?.message || error || '未知错误').replace(/\s+/g, ' ').trim();
      const reason = summarizeLaunchIssue(detail);
      launchIssueMessage = reason || `启动失败：${detail.slice(0, 140)}`;
      launchIssueNotified = true;
      toast(launchIssueMessage, true);
    } finally {
      launching.value = false;
    }
  }

  async function handleCloseRequested() {
    const accepted = await askConfirm({
      title: '游戏仍在运行',
      message: '请先在游戏内保存进度。结束游戏并退出启动器可能会丢失尚未保存的内容。',
      confirmLabel: '停止游戏并退出',
      danger: true,
    });
    if (!accepted) return;
    try {
      await api.stop();
      for (let attempt = 0; attempt < 80; attempt += 1) {
        const current = await api.snapshot();
        if (!['running', 'stopping'].includes(current.state?.phase)) {
          await api.windowAction('close');
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      toast('游戏进程仍在退出，请稍后再关闭启动器。', true);
    } catch (error) {
      toast(String(error?.message || error), true);
    }
  }

  async function manageMod(mod, action) {
    if (action === 'delete') {
      const accepted = await askConfirm({
        title: '删除模组',
        message: `确定从当前版本中永久删除“${mod.title || mod.name}”吗？`,
        confirmLabel: '删除模组',
        danger: true,
      });
      if (!accepted) return;
    }
    mods.value = await api.modAction(mod.name, action);
    modUpdateActions.clearModUpdates();
    await refresh();
    toast(action === 'delete' ? '模组已删除' : mod.enabled ? '模组已停用' : '模组已启用');
    selectedModNames.value = selectedModNames.value.filter((name) => name !== mod.name);
  }

  function toggleVersionSelection(id) {
    selectedVersionIds.value = selectedVersionIds.value.includes(id)
      ? selectedVersionIds.value.filter((value) => value !== id)
      : [...selectedVersionIds.value, id];
  }
  function toggleVersionBatchMode() {
    versionBatchMode.value = !versionBatchMode.value;
    if (!versionBatchMode.value) selectedVersionIds.value = [];
  }
  function toggleVisibleVersions() {
    const visible = filteredVersions.value.map((item) => item.id);
    selectedVersionIds.value = allVisibleVersionsSelected.value
      ? selectedVersionIds.value.filter((id) => !visible.includes(id))
      : [...new Set([...selectedVersionIds.value, ...visible])];
  }
  async function deleteSelectedVersions() {
    const ids = selectedVersionIds.value.filter((id) => model.value.versions.some((item) => item.id === id));
    if (!ids.length) return;
    const accepted = await askConfirm({
      title: ids.length === 1 ? '删除游戏客户端' : '批量删除游戏客户端',
      message: `将从当前游戏目录中永久删除 ${ids.length} 个客户端及其版本文件。`,
      confirmLabel: `删除 ${ids.length} 个客户端`,
      danger: true,
    });
    if (!accepted) return;
    const result = await api.deleteVersions(ids);
    selectedVersionIds.value = [];
    model.value = result;
    syncDraft();
    await loadMods();
    toast(`已删除 ${ids.length} 个游戏客户端`);
  }
  function toggleModSelection(name) {
    selectedModNames.value = selectedModNames.value.includes(name)
      ? selectedModNames.value.filter((value) => value !== name)
      : [...selectedModNames.value, name];
  }
  function toggleVisibleMods() {
    const visible = filteredMods.value.map((item) => item.name);
    selectedModNames.value = allVisibleModsSelected.value
      ? selectedModNames.value.filter((name) => !visible.includes(name))
      : [...new Set([...selectedModNames.value, ...visible])];
  }
  async function deleteSelectedMods() {
    const names = selectedModNames.value.filter((name) => mods.value.some((item) => item.name === name));
    if (!names.length) return;
    const accepted = await askConfirm({
      title: '批量删除模组',
      message: `确定从当前版本中永久删除选中的 ${names.length} 个模组文件吗？`,
      confirmLabel: `删除 ${names.length} 个模组`,
      danger: true,
    });
    if (!accepted) return;
    mods.value = await api.modActionMany(names, 'delete');
    selectedModNames.value = [];
    modUpdateActions.clearModUpdates();
    await refresh();
    toast(`已删除 ${names.length} 个模组`);
  }

  function clearLogs() {
    pendingLogs = [];
    logs.value = [];
    toast('已清空当前日志显示，日志文件仍然保留');
  }

  function drag(event) {
    if (event.button === 0 && !event.target.closest('button,input,select,.no-drag'))
      safely(() => api.windowAction('drag'));
  }
  onMounted(() =>
    safely(async () => {
      unlistenState = await onState((value) => {
        const progress = value.progress;
        const sampleTime = performance.now();
        const transferredBytes = progress?.downloadedBytes ?? (progress?.unit === 'bytes' ? progress.bytes : null);
        if (!['downloading', 'downloading_java', 'repairing'].includes(value.phase) || !Number.isFinite(transferredBytes)) {
          downloadSpeed.value = 0;
          lastDownloadSample = null;
        } else {
          if (
            lastDownloadSample &&
            lastDownloadSample.bytes <= transferredBytes &&
            sampleTime > lastDownloadSample.time
          ) {
            const elapsed = (sampleTime - lastDownloadSample.time) / 1000;
            downloadSpeed.value = Math.max(0, (transferredBytes - lastDownloadSample.bytes) / elapsed);
          } else {
            downloadSpeed.value = 0;
          }
          lastDownloadSample = { bytes: transferredBytes, time: sampleTime };
        }
        model.value = { ...model.value, state: value };
        if (value.phase === 'error' && !launchIssueNotified && !launching.value) {
          launchIssueNotified = true;
          toast(launchIssueMessage || '游戏启动失败，请查看运行日志了解详情。', true);
        }
        if (value.phase === 'idle') {
          launchIssueMessage = '';
          launchIssueNotified = false;
        }
      });
      unlistenLogs = await onLogs(queueLogs);
      unlistenCloseRequested = await onCloseRequested(() => safely(handleCloseRequested));
      await refresh();
      queueLogs(model.value.logs || []);
      await scanJava();
      memoryTimer = setInterval(() => {
        if (!document.hidden) refreshMemory();
      }, 5000);
    }),
  );
  onBeforeUnmount(() => {
    unlistenState?.();
    unlistenLogs?.();
    unlistenCloseRequested?.();
    clearTimeout(toastTimer);
    clearInterval(memoryTimer);
    if (logFrame !== null) cancelAnimationFrame(logFrame);
  });
  return {
    ...modUpdateActions,
    memoryMax,
    draftMemoryMax,
    memoryStyle,
    memoryUsed,
    memoryWarning,
    memoryNote,
    accountName,
    accountLabel,
    accountModeLabel,
    accountSkinUrl,
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
    mods,
    modSearch,
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
    checkResult,
    launchLabel,
    logStatus,
    gb,
    safely,
    toast,
    refresh,
    scanJava,
    changeVersion,
    changeDirectory,
    addDirectory,
    chooseFolder,
    saveSettings,
    onHomeMemory,
    saveHomeMemory,
    globalSettings,
    versionSettingsTarget,
    versionSettingsDraft,
    versionSettingsItem,
    versionJavaOptions,
    versionSettingsMemoryMax,
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
    openVersionSettings,
    openServerList,
    serverListVersion,
    closeVersionSettings,
    saveVersionSettings,
    resetVersionSettings,
    chooseJava,
    selectView,
    editAccount,
    checkEnvironment,
    closeCheck,
    repairFiles,
    launchGame,
    manageMod,
    answerConfirm,
    clearLogs,
    contentChanged,
    drag,
  };
}
