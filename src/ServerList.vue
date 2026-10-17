<script setup>
import { computed, reactive, ref, watch } from 'vue';
import { api } from './useLauncher';
import './server-list.css';

const props = defineProps({
  active: { type: Boolean, default: false },
  versionId: { type: String, default: '' },
  locked: { type: Boolean, default: false },
});
const emit = defineEmits(['back', 'notification', 'join']);
const servers = ref([]);
const loading = ref(false);
const refreshing = ref(false);
const issue = ref('');
const addForm = reactive({ name: '', address: '' });
const adding = ref(false);
let loadSequence = 0;

const readyCount = computed(() => servers.value.filter((server) => server.status?.ok).length);

async function load({ refresh = true } = {}) {
  if (!props.versionId) return;
  const sequence = ++loadSequence;
  issue.value = '';
  loading.value = true;
  try {
    const result = await api.servers(props.versionId);
    if (sequence !== loadSequence) return;
    servers.value = result;
    loading.value = false;
    if (refresh && servers.value.length) await ping(sequence);
  } catch (error) {
    if (sequence === loadSequence) issue.value = String(error?.message || error);
  } finally {
    if (sequence === loadSequence) loading.value = false;
  }
}

async function ping(sequence = loadSequence) {
  if (!servers.value.length) return;
  refreshing.value = true;
  servers.value = servers.value.map((server) => ({ ...server, status: { pending: true } }));
  try {
    const result = await api.pingServers(props.versionId);
    if (sequence === loadSequence) servers.value = result;
  } catch (error) {
    if (sequence === loadSequence) issue.value = String(error?.message || error);
  } finally {
    if (sequence === loadSequence) refreshing.value = false;
  }
}

async function addServer() {
  if (!addForm.name.trim() || !addForm.address.trim()) {
    issue.value = '请填写服务器名称和地址。';
    return;
  }
  adding.value = true;
  issue.value = '';
  try {
    servers.value = await api.addServer(props.versionId, addForm.name, addForm.address);
    addForm.name = '';
    addForm.address = '';
    emit('notification', '服务器已添加');
    await ping();
  } catch (error) {
    issue.value = String(error?.message || error);
  } finally {
    adding.value = false;
  }
}

async function removeServer(server) {
  try {
    servers.value = await api.removeServer(props.versionId, server.address);
    emit('notification', '服务器已删除');
  } catch (error) {
    issue.value = String(error?.message || error);
  }
}

async function moveServer(server, offset) {
  try {
    const statuses = new Map(servers.value.map((item) => [item.address, item.status]));
    const reordered = await api.moveServer(props.versionId, server.address, offset);
    servers.value = reordered.map((item) => ({ ...item, status: statuses.get(item.address) }));
    await ping();
  } catch (error) {
    issue.value = String(error?.message || error);
  }
}

function latencyLabel(server) {
  const value = server.status?.latencyMs;
  return Number.isFinite(value) ? `${value} ms` : '—';
}

function motdLines(parts = []) {
  const lines = [[]];
  for (const part of parts) {
    const segments = String(part.text || '').split(/\r\n|\r|\n/);
    segments.forEach((text, index) => {
      if (text) lines[lines.length - 1].push({ ...part, text });
      if (index < segments.length - 1) lines.push([]);
    });
  }
  return lines.map((line) => {
    const trimmed = line.map((part) => ({ ...part }));
    const first = trimmed.find((part) => part.text.length);
    const last = [...trimmed].reverse().find((part) => part.text.length);
    if (first) first.text = first.text.replace(/^[ \t]+/, '');
    if (last) last.text = last.text.replace(/[ \t]+$/, '');
    return trimmed.filter((part) => part.text.length);
  });
}

watch(
  () => [props.active, props.versionId],
  ([active]) => {
    if (active) void load();
    else loadSequence++;
  },
  { immediate: true },
);
</script>

<template>
  <section id="view-server-list" class="view server-list-view" :class="{ active }">
    <button class="quiet-button server-list-back" @click="emit('back')">
      <svg><use href="#i-arrow" /></svg>
      返回游戏库
    </button>
    <div class="page-heading server-list-heading">
      <div>
        <div class="eyebrow">MULTIPLAYER</div>
        <h1>服务器列表<span class="heading-dot">.</span></h1>
        <p>管理 {{ versionId }} 的服务器。列表与此版本的 Minecraft 客户端同步。</p>
      </div>
      <button class="outline-button" :disabled="loading || refreshing || !servers.length" @click="ping()">
        <svg :class="{ spinning: refreshing }"><use href="#i-refresh" /></svg>
        {{ refreshing ? '检测中…' : '刷新状态' }}
      </button>
    </div>

    <form class="server-add-card" @submit.prevent="addServer">
      <div class="server-add-title">
        <span class="server-list-icon server-add-plus" aria-hidden="true">+</span>
        <div><h2>添加服务器</h2><p>输入 Java 版服务器地址，启动器会读取游戏内的状态信息。</p></div>
      </div>
      <label>
        <span>服务器名称</span>
        <input v-model="addForm.name" :disabled="locked || adding" maxlength="256" placeholder="例如：我的生存服务器" />
      </label>
      <label>
        <span>服务器地址</span>
        <input v-model="addForm.address" :disabled="locked || adding" maxlength="255" placeholder="play.example.com 或 play.example.com:25565" />
      </label>
      <button class="primary-small" type="submit" :disabled="locked || adding">
        {{ adding ? '添加中…' : '添加服务器' }}
      </button>
    </form>

    <div v-if="issue" class="server-list-message error-text" role="alert">{{ issue }}</div>
    <div class="server-list-toolbar">
      <span>{{ loading ? '正在读取服务器列表…' : `${servers.length} 个服务器` }}</span>
      <span v-if="servers.length && !refreshing">{{ readyCount }} 个可连接</span>
      <span v-else-if="refreshing">正在检测连接、MOTD 和延迟…</span>
    </div>

    <div v-if="loading && !servers.length" class="server-list-skeleton" aria-label="正在加载">
      <div v-for="item in 3" :key="item" class="server-skeleton-row"><i></i><span><b></b><small></small></span></div>
    </div>
    <div v-else-if="!servers.length" class="server-list-empty">
      <span class="server-list-icon"><svg><use href="#i-box" /></svg></span>
      <h2>还没有服务器</h2>
      <p>添加的服务器也会出现在这个版本的游戏内服务器列表中。</p>
    </div>
    <div v-else class="server-list-entries">
      <article v-for="(server, index) in servers" :key="server.address" class="server-entry">
        <div class="server-entry-logo">
          <img v-if="server.icon" :src="server.icon" alt="服务器图标" />
          <svg v-else><use href="#i-box" /></svg>
          <button class="server-entry-join" title="启动游戏并加入服务器" aria-label="启动游戏并加入服务器" :disabled="locked" @click="emit('join', server.address)">
            <svg><use href="#i-play" /></svg>
          </button>
        </div>
        <div class="server-entry-main">
          <div class="server-entry-heading">
            <div class="server-entry-title"><h2>{{ server.name }}</h2><code>{{ server.address }}</code></div>
            <div class="server-entry-stats" :class="{ online: server.status?.ok, offline: server.status && !server.status.ok && !server.status.pending }">
              <div v-if="server.status?.ok" class="server-entry-players">
                <strong>{{ server.status.playersOnline ?? '?' }}<span>/</span>{{ server.status.playersMax ?? '?' }}</strong>
                <div class="server-signal" :aria-label="`延迟 ${latencyLabel(server)}`"><i></i><i></i><i></i><i></i></div>
              </div>
              <div v-else class="server-signal" :class="{ pending: server.status?.pending }" aria-hidden="true"><i></i><i></i><i></i><i></i></div>
              <strong class="server-ping">{{ latencyLabel(server) }}</strong>
            </div>
          </div>
          <div v-if="server.status?.ok" class="server-entry-motd" aria-label="服务器简介">
            <div v-for="(line, lineIndex) in motdLines(server.status.motd)" :key="lineIndex" class="server-entry-motd-line">
              <span
                v-for="(part, partIndex) in line"
                :key="partIndex"
                :style="{ color: part.color || undefined, fontWeight: part.bold ? 700 : undefined, fontStyle: part.italic ? 'italic' : undefined, textDecoration: [part.underlined && 'underline', part.strikethrough && 'line-through'].filter(Boolean).join(' ') || undefined }"
              >{{ part.text }}</span>
            </div>
          </div>
          <div v-else-if="server.status?.pending" class="server-entry-status">正在检测服务器…</div>
          <div v-else-if="server.status?.error" class="server-entry-status server-offline">{{ server.status.error }}</div>
          <div v-else class="server-entry-status">尚未检测连接状态</div>
          <div class="server-entry-version" v-if="server.status?.version">{{ server.status.version }}</div>
        </div>
        <div class="server-entry-actions">
          <button class="icon-button" title="上移" aria-label="上移服务器" :disabled="locked || index === 0" @click="moveServer(server, -1)">↑</button>
          <button class="icon-button" title="下移" aria-label="下移服务器" :disabled="locked || index === servers.length - 1" @click="moveServer(server, 1)">↓</button>
          <button class="icon-button danger" title="删除" aria-label="删除服务器" :disabled="locked" @click="removeServer(server)">×</button>
        </div>
      </article>
    </div>
    <div class="server-list-note"><svg><use href="#i-info" /></svg><span>添加、删除和排序会写入所选客户端的服务器列表文件。MOTD 会保留 Minecraft 颜色、样式与换行。</span></div>
  </section>
</template>
