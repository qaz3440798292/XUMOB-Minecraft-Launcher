import { ref, computed, watch, onMounted } from 'vue';
import { api } from '../useLauncher';
import { resolveForgeRequirement } from '../forgeRequirement';

const loaders = [
  { id: 'vanilla', name: '原版', mark: 'M', text: '经典 Minecraft 体验' },
  { id: 'fabric', name: 'Fabric', mark: 'F', text: '轻量、快速的模组加载器' },
  { id: 'forge', name: 'Forge', mark: 'F', text: '丰富的模组生态' },
  { id: 'neoforge', name: 'NeoForge', mark: 'N', text: '新一代模组加载器' },
];

/** 游戏安装页面的状态与操作。须在组件 setup 中调用。 */
export function useGameDownloads(props, emit) {
  // 游戏目录和安装表单。
  const catalog = ref({ versions: [], latest: {} });
  const query = ref('');
  const channel = ref('release');
  const selectedGame = ref(null);
  const components = ref({});
  const loading = ref(false);
  const componentLoading = ref(false);
  const preparing = ref(false);
  const installing = ref(false);
  const error = ref('');
  const success = ref('');
  const plan = ref(null);
  const instanceName = ref('');
  const gameRoot = ref(props.settings?.gameRoot || '');
  const loader = ref('vanilla');
  const loaderVersion = ref('');
  const optifineEnabled = ref(false);
  const optifineVersion = ref('');
  const fabricApi = ref(false);
  const fabricApiVersion = ref('');
  const optional = ref(false);
  // 忽略切换版本前发出的旧组件请求。
  let generation = 0;
  const blocked = computed(() => installing.value || props.state?.phase === 'downloading');
  const games = computed(() =>
    catalog.value.versions
      .filter(
        (v) => {
          const isEarly = v.type === 'old_beta' || v.type === 'old_alpha';
          const matchesChannel =
            channel.value === 'all' ||
            (channel.value === 'early' ? isEarly : v.type === channel.value);
          return matchesChannel && v.id.toLowerCase().includes(query.value.toLowerCase());
        },
      )
      .slice(0, 150),
  );

  const percent = computed(() =>
    Math.min(
      100,
      Math.round(
        ((props.state?.progress?.bytes || 0) /
          Math.max(1, props.state?.progress?.totalBytes || 0)) *
          100,
      ),
    ),
  );
  const choices = (id) => components.value[id]?.versions || [];
  const selectedOptifine = computed(() =>
    choices('optifine').find((v) => v.id === optifineVersion.value),
  );
  const optifineForgeRequirement = computed(() =>
    resolveForgeRequirement(selectedOptifine.value?.forge, choices('forge')),
  );
  const optifineIncompatible = computed(
    () =>
      optifineEnabled.value &&
      loader.value === 'forge' &&
      Boolean(optifineForgeRequirement.value) &&
      optifineForgeRequirement.value !== loaderVersion.value,
  );
  function errorMessage(e) {
    return String(e?.message || e);
  }

  function updateInstanceName() {
    if (!selectedGame.value) return;

    const loaderName = loaders.find((item) => item.id === loader.value)?.name || loader.value;
    const loaderSuffix = loader.value === 'vanilla' ? '' : `-${loaderName}`;
    const optifineSuffix = optifineEnabled.value ? '-OptiFine' : '';
    instanceName.value = `${selectedGame.value.id}${loaderSuffix}${optifineSuffix}`;
  }
  // 读取版本与组件；切换加载器时清除不兼容选项。
  async function loadCatalog() {
    loading.value = true;
    error.value = '';
    try {
      catalog.value = await api.download('game_catalog');
    } catch (e) {
      error.value = errorMessage(e);
    } finally {
      loading.value = false;
    }
  }

  async function selectGame(game) {
    if (blocked.value) return;
    selectedGame.value = game;
    loader.value = 'vanilla';
    loaderVersion.value = '';
    optifineEnabled.value = false;
    fabricApi.value = false;
    plan.value = null;
    updateInstanceName();
    error.value = '';
    success.value = '';
    componentLoading.value = true;
    components.value = {};
    const requestGeneration = ++generation;
    try {
      const result = await api.download('game_components', { minecraft: game.id });
      if (requestGeneration !== generation) return;
      components.value = result;
      optifineVersion.value = choices('optifine')[0]?.id || '';
      fabricApiVersion.value = choices('fabricApi')[0]?.id || '';
    } catch (e) {
      if (requestGeneration === generation) error.value = errorMessage(e);
    } finally {
      if (requestGeneration === generation) componentLoading.value = false;
    }
  }

  function changeLoader(id) {
    if (blocked.value) return;
    loader.value = id;
    loaderVersion.value = choices(id)[0]?.id || '';
    if (!['vanilla', 'forge'].includes(id)) optifineEnabled.value = false;
    if (id !== 'fabric') fabricApi.value = false;
    updateInstanceName();
    plan.value = null;
  }

  async function chooseGameFolder() {
    try {
      const path = await api.chooseFolder();
      if (path) {
        gameRoot.value = path;
        plan.value = null;
      }
    } catch (e) {
      error.value = errorMessage(e);
    }
  }
  // 先预览安装计划，用户确认后才写入实例。
  async function prepareInstallation() {
    preparing.value = true;
    error.value = '';
    plan.value = null;
    try {
      plan.value = await api.download(
        'game_prepare',
        {
          name: instanceName.value.trim(),
          gameRoot: gameRoot.value,
          minecraft: selectedGame.value?.id || '',
          loader: loader.value,
          loaderVersion: loaderVersion.value,
          optifine: optifineEnabled.value ? optifineVersion.value : '',
          fabricApi: fabricApi.value ? fabricApiVersion.value : '',
          optional: optional.value,
          ...(props.pack || {}),
        },
        props.pack?.provider || 'modrinth',
      );
    } catch (e) {
      error.value = errorMessage(e);
    } finally {
      preparing.value = false;
    }
  }

  async function installGame() {
    installing.value = true;
    error.value = '';
    try {
      const result = await api.download('game_install', { planId: plan.value.planId });
      success.value = `${result.versionId} 安装完成，已加入游戏库。`;
      const packNotice = props.pack
        ? {
            title: props.pack.title || result.versionId,
            message: '安装完成，已加入游戏库。',
            iconUrl: props.pack.iconData || '',
          }
        : success.value;
      emit('changed');
      emit('notification', packNotice);
      plan.value = null;
    } catch (e) {
      error.value = errorMessage(e);
      plan.value = null;
    } finally {
      installing.value = false;
    }
  }

  async function cancelInstallation() {
    try {
      await api.download('game_cancel');
    } catch (e) {
      error.value = errorMessage(e);
    }
  }

  function goBack() {
    if (blocked.value || preparing.value) return;
    if (props.pack) emit('back');
    else {
      generation++;
      selectedGame.value = null;
      plan.value = null;
      error.value = '';
      success.value = '';
    }
  }
  // 跟随主界面的目录设置，并在表单修改后使旧计划失效。
  watch(
    () => props.settings?.gameRoot,
    (value, previous) => {
      if (value && (!gameRoot.value || gameRoot.value === previous)) gameRoot.value = value;
    },
    { immediate: true },
  );
  watch(
    () => {
      const pack = props.pack;
      return pack
        ? [pack.provider, pack.archivePath, pack.projectId, pack.versionId, pack.title].join('\0')
        : '';
    },
    (identity) => {
      if (!identity || !props.pack) return;
      instanceName.value = String(props.pack.title || '我的整合包')
        .replace(/[<>:"/\\|?*\x00-\x1f]/g, '-')
        .replace(/[. ]+$/, '')
        .slice(0, 80);
      optional.value = false;
      plan.value = null;
      success.value = '';
      error.value = '';
    },
    { immediate: true },
  );
  watch(
    [
      instanceName,
      gameRoot,
      loaderVersion,
      optifineVersion,
      fabricApiVersion,
      optifineEnabled,
      fabricApi,
      optional,
    ],
    () => {
      if (!preparing.value && !blocked.value) plan.value = null;
      error.value = '';
    },
  );
  watch(
    [optifineEnabled, loader, optifineForgeRequirement],
    ([enabled, selectedLoader, requiredForge]) => {
      if (
        enabled &&
        selectedLoader === 'forge' &&
        requiredForge &&
        choices('forge').some((version) => version.id === requiredForge)
      ) {
        loaderVersion.value = requiredForge;
      }
    },
  );
  onMounted(() => {
    if (!props.pack) loadCatalog();
  });

  return {
    catalog,
    query,
    channel,
    selectedGame,
    components,
    loading,
    componentLoading,
    preparing,
    error,
    success,
    plan,
    instanceName,
    gameRoot,
    loader,
    loaderVersion,
    optifineEnabled,
    optifineVersion,
    fabricApi,
    fabricApiVersion,
    optional,
    blocked,
    games,
    loaders,
    percent,
    choices,
    selectedOptifine,
    optifineForgeRequirement,
    optifineIncompatible,
    updateInstanceName,
    loadCatalog,
    selectGame,
    changeLoader,
    chooseGameFolder,
    prepareInstallation,
    installGame,
    cancelInstallation,
    goBack,
  };
}
