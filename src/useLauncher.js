import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
const call = (command, args) => invoke(command, args);
export const api = {
  searchContent: (query, kind, offset=0, sort='relevance') => call('modrinth_search', { query, kind, offset, sort }),
  contentVersions: (projectId, kind) => call('modrinth_versions', { projectId, kind }),
  prepareContent: (projectId, versionId, kind) => call('modrinth_prepare', { projectId, versionId, kind }),
  installContent: planId => call('modrinth_install', { planId }), cancelContent: () => call('modrinth_cancel'),
  openContentPage: (projectId, kind) => call('modrinth_open', { projectId, kind }),
  configureAccount: clientId => call('account_configure', { clientId }), selectAccount: mode => call('account_select', { mode }), logoutAccount: () => call('account_logout'),
  startLogin: () => call('auth_start'), pollLogin: sessionId => call('auth_poll', { sessionId }), cancelLogin: () => call('auth_cancel'), openLoginBrowser: sessionId => call('auth_open_browser', { sessionId }),
  snapshot: () => call('snapshot'), save: patch => call('save_settings', { patch }), addDirectory: selected => call('add_directory', { selected }),
  java: force => call('find_java', { force: Boolean(force) }), check: () => call('check_environment'), repair: () => call('repair_files'),
  launch: () => call('launch_game'), stop: () => call('stop_game'), mods: () => call('list_mods'), chooseFolder: () => call('choose_folder'), chooseJava: () => call('choose_java'),
  openFolder: kind => call('open_folder', { kind }), windowAction: action => call('window_action', { action })
};
// Keep the previous narrow launcher bridge, including automated compatibility tests.
window.launcher = api;
export function onState(callback) { return listen('launcher:state', event => callback(event.payload)); }
export function onLogs(callback) { return listen('launcher:logs', event => callback(event.payload)); }
