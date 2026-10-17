import { computed, ref, shallowRef, watch } from 'vue';
import { api } from '../useLauncher';

export function useModUpdates({ settings, loadMods, refresh, toast, askConfirm }) {
  const checkingModUpdates = ref(false);
  const updatingMod = ref('');
  const modUpdates = shallowRef({});
  const modUpdateWarnings = shallowRef([]);
  const hasCheckedModUpdates = ref(false);
  const updateCount = computed(
    () => Object.values(modUpdates.value).filter((item) => item.status === 'available').length,
  );
  const modUpdateBusy = computed(() => checkingModUpdates.value || Boolean(updatingMod.value));
  const instanceKey = computed(() =>
    JSON.stringify([settings.value.gameRoot, settings.value.versionId, settings.value.isolated]),
  );

  function clearModUpdates() {
    modUpdates.value = {};
    modUpdateWarnings.value = [];
    hasCheckedModUpdates.value = false;
  }
  watch(instanceKey, clearModUpdates);

  async function checkModUpdates() {
    if (modUpdateBusy.value) return;
    const key = instanceKey.value;
    checkingModUpdates.value = true;
    clearModUpdates();
    try {
      const result = await api.checkModUpdates();
      if (key !== instanceKey.value) return;
      modUpdates.value = Object.fromEntries(result.mods.map((item) => [item.name, item]));
      modUpdateWarnings.value = result.warnings || [];
      hasCheckedModUpdates.value = true;
      // Refresh the local list too, including files added outside the launcher.
      await loadMods();
      toast(
        updateCount.value
          ? `发现 ${updateCount.value} 个兼容更新`
          : '检查完成，未发现可用的兼容更新',
      );
    } finally {
      checkingModUpdates.value = false;
      await refresh();
    }
  }

  async function updateMod(mod) {
    const update = modUpdates.value[mod.name];
    if (modUpdateBusy.value || update?.status !== 'available') return;
    const accepted = await askConfirm({
      title: '更新模组',
      message: `将“${mod.title || mod.name}”从 ${update.currentVersion || '当前版本'} 更新至 ${update.latestVersion}？会一并检查必要前置，保留启用或停用状态。整合包建议遵循作者的版本搭配。`,
      confirmLabel: '下载并更新',
    });
    if (!accepted) return;
    updatingMod.value = mod.name;
    try {
      const result = await api.updateMod(update.updateId, update.provider);
      await loadMods();
      const remaining = { ...modUpdates.value };
      delete remaining[mod.name];
      remaining[result.filename || mod.name] = {
        provider: update.provider,
        status: 'current',
        currentVersion: result.version || update.latestVersion,
        message: '已更新至兼容的最新正式版',
      };
      modUpdates.value = remaining;
      toast(result.backupRetained ? '模组已更新，旧文件备份保留在模组目录中' : '模组已更新');
    } catch (error) {
      modUpdates.value = {
        ...modUpdates.value,
        [mod.name]: { ...update, status: 'error', message: String(error?.message || error) },
      };
      throw error;
    } finally {
      updatingMod.value = '';
      await refresh();
    }
  }

  async function updateMods(modList) {
    const candidates = modList
      .map((mod) => ({ mod, update: modUpdates.value[mod.name] }))
      .filter(({ update }) => update?.status === 'available');
    if (!candidates.length || modUpdateBusy.value) return;
    const accepted = await askConfirm({
      title: '批量更新模组',
      message: `将依次更新 ${candidates.length} 个当前版本与加载器兼容的模组，并检查必要前置。每个模组会保留启用或停用状态；整合包建议遵循作者的版本搭配。`,
      confirmLabel: `更新 ${candidates.length} 个模组`,
    });
    if (!accepted) return;

    updatingMod.value = '__batch__';
    let updated = 0;
    const failures = [];
    const nextUpdates = { ...modUpdates.value };
    try {
      for (const { mod, update } of candidates) {
        try {
          const result = await api.updateMod(update.updateId, update.provider);
          delete nextUpdates[mod.name];
          nextUpdates[result.filename || mod.name] = {
            provider: update.provider,
            status: 'current',
            currentVersion: result.version || update.latestVersion,
            message: '已更新至兼容的最新正式版',
          };
          updated += 1;
        } catch (error) {
          failures.push(`${mod.title || mod.name}：${String(error?.message || error)}`);
          nextUpdates[mod.name] = { ...update, status: 'error', message: failures.at(-1) };
        }
      }
      modUpdates.value = nextUpdates;
      await loadMods();
      if (failures.length) {
        modUpdateWarnings.value = [...modUpdateWarnings.value, ...failures];
        toast(`批量更新完成：成功 ${updated} 个，失败 ${failures.length} 个`, failures.length === candidates.length);
      } else {
        toast(`已成功更新 ${updated} 个模组`);
      }
    } finally {
      updatingMod.value = '';
      await refresh();
    }
  }

  return {
    checkingModUpdates,
    updatingMod,
    modUpdates,
    modUpdateWarnings,
    hasCheckedModUpdates,
    updateCount,
    modUpdateBusy,
    clearModUpdates,
    checkModUpdates,
    updateMod,
    updateMods,
  };
}
