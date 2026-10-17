import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
const call = (command, args) => invoke(command, args);
export const api = {
  download: (action, args = {}, provider = 'modrinth') =>
    call('download_request', { action, provider, args }),
  searchContent: (
    query,
    kind,
    offset = 0,
    sort = 'relevance',
    filters = {},
    provider = 'modrinth',
  ) => api.download('search', { query, kind, offset, sort, filters }, provider),
  contentVersions: (projectId, kind, filters = {}, provider = 'modrinth') =>
    api.download('versions', { projectId, kind, filters }, provider),
  contentCatalog: (provider = 'modrinth') => api.download('catalog', {}, provider),
  openContentLink: (url) => call('modrinth_open_link', { url }),
  prepareContent: (projectId, versionId, kind, provider = 'modrinth') =>
    api.download('prepare', { projectId, versionId, kind }, provider),
  installContent: (planId, provider = 'modrinth') => api.download('install', { planId }, provider),
  cancelContent: (provider = 'modrinth') => api.download('cancel', {}, provider),
  openContentPage: (projectId, kind) => call('modrinth_open', { projectId, kind }),
  selectAccount: (mode) => call('account_select', { mode }),
  logoutAccount: () => call('account_logout'),
  addOfflineAccount: (username) => call('account_add_offline', { username }),
  addYggdrasilAccount: (mode, apiRoot, username, password) =>
    call('account_add_yggdrasil', { mode, apiRoot, username, password }),
  selectSavedAccount: (id) => call('account_select_saved', { id }),
  removeAccount: (id) => call('account_remove', { id }),
  refreshAccountSkins: () => call('account_refresh_skins'),
  startLogin: () => call('auth_start'),
  loginWithWindowsAccount: () => call('auth_wam_login'),
  windowsAccountLoginSupported: () => call('windows_account_login_supported'),
  openAuthWindow: (sessionId) => call('auth_open_window', { sessionId }),
  closeAuthWindow: () => call('auth_close_window'),
  pollLogin: (sessionId) => call('auth_poll', { sessionId }),
  cancelLogin: () => call('auth_cancel'),
  snapshot: () => call('snapshot'),
  memory: () => call('system_memory'),
  save: (patch) => call('save_settings', { patch }),
  addDirectory: (selected) => call('add_directory', { selected }),
  java: (force) => call('find_java', { force: Boolean(force) }),
  downloadJava: (major, release) => call('download_java', { major, release }),
  check: () => call('check_environment'),
  repair: () => call('repair_files'),
  launch: (serverAddress = null) => call('launch_game', { serverAddress }),
  stop: () => call('stop_game'),
  mods: () => call('list_mods'),
  modAction: (name, action) => call('mod_action', { name, action }),
  modActionMany: (names, action) => call('mod_action_many', { names, action }),
  deleteVersions: (ids) => call('delete_versions', { ids }),
  versionSettings: (versionId) => call('version_settings', { versionId }),
  saveVersionSettings: (versionId, custom, patch) =>
    call('save_version_settings', { versionId, custom, patch }),
  servers: (versionId) => call('list_servers', { versionId }),
  addServer: (versionId, name, address) =>
    call('add_server', { versionId, name, address }),
  removeServer: (versionId, address) =>
    call('remove_server', { versionId, address }),
  moveServer: (versionId, address, offset) =>
    call('move_server', { versionId, address, offset }),
  pingServers: (versionId) => call('ping_servers', { versionId }),
  gameVersionInfo: (versionId) => api.download('game_version_info', { versionId }),
  checkModUpdates: () => api.download('mods_check_updates'),
  modMetadata: () => api.download('mods_metadata'),
  updateMod: (updateId, provider) => api.download('mod_update', { updateId }, provider),
  chooseFolder: () => call('choose_folder'),
  chooseJava: () => call('choose_java'),
  chooseModpack: () => call('choose_modpack'),
  openFolder: (kind) => call('open_folder', { kind }),
  windowAction: (action) => call('window_action', { action }),
};
// Keep the previous narrow launcher bridge, including automated compatibility tests.
window.launcher = api;
export function onState(callback) {
  return listen('launcher:state', (event) => callback(event.payload));
}
export function onLogs(callback) {
  return listen('launcher:logs', (event) => callback(event.payload));
}
export function onCloseRequested(callback) {
  return listen('launcher:close-requested', callback);
}
