<script setup>
import { computed } from 'vue';
import { useGameDownloads } from './composables/useGameDownloads';
import SearchSelect from './SearchSelect.vue';
import GameBlockIcon from './GameBlockIcon.vue';
import { resolveForgeRequirement } from './forgeRequirement';
import './game-downloads.css';

const props = defineProps({ settings: Object, state: Object, locked: Boolean, pack: Object });
const emit = defineEmits(['changed', 'notification', 'back']);
const {
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
} = useGameDownloads(props, emit);
const loaderVersionOptions = computed(() =>
  choices(loader.value).map((item) => ({
    value: item.id,
    label: item.name,
    meta: item.stable ? '正式版本' : '预览版本',
  })),
);
const fabricApiOptions = computed(() =>
  choices('fabricApi').map((item) => ({ value: item.id, label: item.name })),
);
const optifineOptions = computed(() =>
  choices('optifine').map((item) => ({
    value: item.id,
    label: item.name,
    meta: `${item.stable ? '正式版' : '预览版'}${item.forge ? ` · Forge ${resolveForgeRequirement(item.forge, choices('forge'))}` : ''}`,
  })),
);
</script>

<template>
  <div class="game-downloads">
    <template v-if="!selectedGame && !pack">
      <div class="game-discovery-banner">
        <span class="game-block-icon">◈</span>
        <div>
          <div class="eyebrow">BUILD YOUR OWN WORLD</div>
          <h2>从一个全新的世界开始</h2>
          <p>选择游戏版本，自由搭配加载器与常用组件。</p>
        </div>
        <span class="game-latest">
          最新正式版
          <strong>{{ catalog.latest.release || '—' }}</strong>
        </span>
      </div>
      <div class="game-toolbar">
        <label class="search-field">
          <svg><use href="#i-search" /></svg>
          <input
            v-model="query"
            placeholder="搜索游戏版本，如 1.21.1"
            aria-label="搜索游戏版本"
          />
        </label>
        <div class="game-channel-tabs">
          <button
            v-for="(text, id) in { release: '正式版', snapshot: '快照', april_fools: '愚人节版', early: '早期版', all: '全部版本' }"
            :key="id"
            :class="{ selected: channel === id }"
            @click="channel = id"
          >
            {{ text }}
          </button>
        </div>
        <button
          class="quiet-button"
          :disabled="loading"
          @click="loadCatalog"
        >
          刷新
        </button>
      </div>
      <div
        v-if="error"
        class="download-error"
        role="alert"
      >
        {{ error }}
      </div>
      <div
        v-if="loading"
        class="game-version-loading"
        role="status"
        aria-live="polite"
      >
        <div class="game-version-loading-label">
          <span class="game-version-loading-spinner" aria-hidden="true" />
          <span>正在读取 Minecraft 官方版本列表…</span>
        </div>
        <div class="game-version-skeleton-grid" aria-hidden="true">
          <div v-for="row in 9" :key="row" class="game-version-skeleton-card">
            <span class="game-version-skeleton-icon" />
            <span class="game-version-skeleton-copy">
              <i class="game-version-skeleton-title" />
              <i class="game-version-skeleton-meta" />
            </span>
            <i class="game-version-skeleton-arrow" />
          </div>
        </div>
      </div>
      <div
        v-else
        class="game-version-grid"
      >
        <button
          v-for="game in games"
          :key="game.id"
          class="game-version-card"
          :disabled="locked"
          @click="selectGame(game)"
        >
          <span
            class="game-version-icon"
            :class="game.type"
          >
            <GameBlockIcon :type="game.type" />
          </span>
          <span>
            <strong>{{ game.id }}</strong>
            <small>
              {{
                {
                  release: '正式版',
                  snapshot: '快照',
                  april_fools: '愚人节版',
                  old_beta: '早期版',
                  old_alpha: '早期版',
                }[game.type] || game.type
              }}
              · {{ game.date?.slice(0, 10) }}
            </small>
          </span>
          <em v-if="game.id === catalog.latest.release">最新</em>
          <span class="game-version-arrow">→</span>
        </button>
      </div>
      <p
        v-if="games.length === 150"
        class="download-hint"
      >
        显示前 150 个结果，可输入版本号快速定位。
      </p>
    </template>
    <template v-else>
      <button
        class="download-back link-button"
        :disabled="blocked || preparing"
        @click="goBack"
      >
        ← {{ pack ? (pack.provider === 'local' ? '返回下载中心' : '返回整合包介绍') : '返回游戏版本列表' }}
      </button>
      <div class="game-install-heading">
        <span class="game-version-icon" :class="pack ? 'release' : selectedGame?.type">
          <img
            v-if="pack?.iconData"
            :src="pack.iconData"
            :alt="`${pack.title} 整合包图标`"
            loading="lazy"
            decoding="async"
          />
          <GameBlockIcon v-else :type="selectedGame?.type || 'release'" />
        </span>
        <div>
          <div class="eyebrow">MAKE IT YOURS</div>
          <h1>{{ pack ? pack.title : `安装 Minecraft ${selectedGame.id}` }}</h1>
          <p>
            {{
              pack
                ? '整合包使用作者指定的游戏和加载器版本，安装为独立实例。'
                : '选择喜欢的加载器，也可以直接开始原版冒险。'
            }}
          </p>
        </div>
      </div>
      <div
        v-if="error"
        class="download-error"
        role="alert"
      >
        {{ error }}
      </div>
      <div
        v-if="success"
        class="download-success"
        role="status"
      >
        {{ success }}
      </div>
      <div class="game-install-grid">
        <div class="game-install-options">
          <section class="install-option-section">
            <div class="install-step">
              <b>01</b>
              <div>
                <h2>命名你的世界</h2>
                <p>每个实例拥有独立的模组、配置和存档。</p>
              </div>
            </div>
            <label
              class="field-label"
              for="instance-install-name"
            >
              实例名称
            </label>
            <input
              id="instance-install-name"
              v-model="instanceName"
              maxlength="100"
              :disabled="blocked || preparing"
              placeholder="我的 Minecraft"
            />
            <label
              class="field-label"
              for="game-install-root"
            >
              游戏安装目录
            </label>
            <div class="install-folder-row">
              <input
                id="game-install-root"
                v-model="gameRoot"
                :disabled="blocked || preparing"
                placeholder="选择 .minecraft 目录"
              />
              <button
                class="outline-button"
                :disabled="blocked || preparing"
                @click="chooseGameFolder"
              >
                <svg><use href="#i-folder" /></svg>
                选择
              </button>
            </div>
          </section>
          <template v-if="!pack">
            <section class="install-option-section">
              <div class="install-step">
                <b>02</b>
                <div>
                  <h2>选择模组加载器</h2>
                  <p>同一实例选择一个加载器，显示此游戏版本可用的组件。</p>
                </div>
              </div>
              <div
                v-if="componentLoading"
                class="download-hint"
              >
                正在检查组件可用版本…
              </div>
              <div class="loader-choice-grid">
                <button
                  v-for="item in loaders"
                  :key="item.id"
                  :class="['loader-choice', { selected: loader === item.id }]"
                  :disabled="
                    blocked ||
                    preparing ||
                    (item.id !== 'vanilla' && (componentLoading || !choices(item.id).length))
                  "
                  @click="changeLoader(item.id)"
                >
                  <span :class="['loader-letter', item.id]">{{ item.mark }}</span>
                  <span>
                    <strong>{{ item.name }}</strong>
                    <small>
                      {{
                        item.id !== 'vanilla' && !choices(item.id).length
                          ? components[item.id]?.unsupported
                            ? '此游戏版本暂不可用'
                            : components[item.id]?.error
                              ? '无法获取版本列表，请稍后重试'
                            : '此游戏版本暂不可用'
                          : item.text
                      }}
                    </small>
                  </span>
                  <span class="loader-radio">{{ loader === item.id ? '●' : '○' }}</span>
                </button>
              </div>
              <template v-if="loader !== 'vanilla'">
                <label
                  class="field-label"
                  for="install-loader-version"
                >
                  {{ loaders.find((l) => l.id === loader)?.name }} 版本
                </label>
                <SearchSelect
                  id="install-loader-version"
                  v-model="loaderVersion"
                  :label="`${loaders.find((item) => item.id === loader)?.name || '加载器'}版本`"
                  placeholder="搜索加载器版本…"
                  :options="loaderVersionOptions"
                  :disabled="blocked || preparing"
                />
              </template>
              <p
                v-for="item in loaders.filter((l) => components[l.id]?.error && !components[l.id]?.unsupported)"
                :key="item.id"
                class="download-hint warning"
              >
                {{ `${item.name}版本列表暂时无法获取，请检查网络后重试。` }}
              </p>
            </section>
            <section class="install-option-section">
              <div class="install-step">
                <b>03</b>
                <div>
                  <h2>添加常用组件</h2>
                  <p>组件会随游戏一起安装，无需再次手动复制。</p>
                </div>
              </div>
              <div class="addon-choice">
                <label>
                  <input
                    v-model="fabricApi"
                    type="checkbox"
                    :disabled="
                      blocked || preparing || loader !== 'fabric' || !choices('fabricApi').length
                    "
                  />
                  <span>
                    <strong>Fabric API</strong>
                    <small>
                      {{
                        loader === 'fabric'
                          ? '许多 Fabric 模组需要的基础支持库'
                          : '选择 Fabric 后可用'
                      }}
                    </small>
                  </span>
                </label>
                <SearchSelect
                  v-if="fabricApi"
                  v-model="fabricApiVersion"
                  id="fabric-api-version"
                  label="Fabric API 版本"
                  placeholder="搜索 Fabric API 版本…"
                  :options="fabricApiOptions"
                  :disabled="blocked || preparing"
                />
                <p
                  v-if="components.fabricApi?.error"
                  class="download-hint warning"
                >
                  {{ components.fabricApi.error }}
                </p>
              </div>
              <div class="addon-choice">
                <label>
                  <input
                    v-model="optifineEnabled"
                    type="checkbox"
                    :disabled="
                      blocked ||
                      preparing ||
                      !['vanilla', 'forge'].includes(loader) ||
                      !choices('optifine').length
                    "
                    @change="updateInstanceName"
                  />
                  <span>
                    <strong>OptiFine</strong>
                    <small>
                      {{
                        ['vanilla', 'forge'].includes(loader)
                          ? '画面优化与光影支持'
                          : '不能与 Fabric 或 NeoForge 组合'
                      }}
                    </small>
                  </span>
                </label>
                <SearchSelect
                  v-if="optifineEnabled"
                  v-model="optifineVersion"
                  id="optifine-version"
                  label="OptiFine 版本"
                  placeholder="搜索 OptiFine 版本…"
                  :options="optifineOptions"
                  :disabled="blocked || preparing"
                />
                <p
                  v-if="optifineIncompatible"
                  class="download-hint warning"
                >
                  此 OptiFine 需要 Forge
                  {{ optifineForgeRequirement || '不支持' }}，请调整加载器版本。
                </p>
                <p
                  v-if="optifineEnabled"
                  class="download-hint"
                >
                  文件来自 BMCLAPI 镜像；OptiFine 未提供公开文件校验值。
                </p>
                <p
                  v-if="components.optifine?.error"
                  class="download-hint warning"
                >
                  {{ components.optifine.error }}
                </p>
              </div>
            </section>
          </template>
          <section
            v-else
            class="install-option-section"
          >
            <div class="install-step">
              <b>02</b>
              <div>
                <h2>整合包内容</h2>
                <p>保留作者指定的文件版本与客户端配置。</p>
              </div>
            </div>
            <label class="filter-checkbox">
              <input
                v-model="optional"
                type="checkbox"
                :disabled="blocked || preparing"
              />
              同时安装作者标为“可选”的客户端文件
            </label>
            <p class="download-hint">
              必要内容会自动下载；服务端专用文件会跳过。确认时读取游戏版本、加载器和下载清单。
            </p>
          </section>
        </div>
        <aside class="game-install-summary">
          <div class="eyebrow">READY TO PLAY</div>
          <h2>安装预览</h2>
          <dl>
            <dt>实例名称</dt>
            <dd>{{ instanceName || '请填写名称' }}</dd>
            <dt>{{ pack ? '项目' : 'Minecraft' }}</dt>
            <dd>{{ pack ? pack.title : selectedGame.id }}</dd>
            <template v-if="pack?.provider === 'local'">
              <dt>Minecraft</dt>
              <dd>{{ pack.minecraft }} · {{ pack.loader === 'vanilla' ? '原版' : `${pack.loader} ${pack.loaderVersion}` }}</dd>
            </template>
            <dt v-if="!pack">加载器</dt>
            <dd v-if="!pack">
              {{ loaders.find((l) => l.id === loader)?.name }} {{ loaderVersion }}
            </dd>
            <template v-if="optifineEnabled">
              <dt>图形组件</dt>
              <dd>OptiFine {{ optifineVersion }}</dd>
            </template>
            <template v-if="fabricApi">
              <dt>基础支持</dt>
              <dd>Fabric API</dd>
            </template>
            <dt>保存位置</dt>
            <dd class="install-path">{{ gameRoot || '请选择游戏目录' }}</dd>
          </dl>
          <div
            v-if="plan"
            class="game-plan-confirm"
          >
            <strong>已检查，可以安装</strong>
            <p>
              Minecraft {{ plan.minecraft }}
              <br />
              {{ plan.loader }} {{ plan.loaderVersion }}
              <br />
              {{ plan.java }}
            </p>
            <code>{{ plan.directory }}</code>
          </div>
          <div
            v-if="blocked"
            class="download-progress"
            role="status"
          >
            <div>
              <strong>正在安装</strong>
              <span>{{ percent }}%</span>
            </div>
            <progress
              :value="percent"
              max="100"
              aria-label="游戏安装进度"
            ></progress>
            <p>{{ state?.message }}</p>
            <small>{{ state?.progress?.filename }}</small>
            <button
              class="outline-button download-wide"
              @click="cancelInstallation"
            >
              取消安装
            </button>
          </div>
          <button
            v-else-if="plan"
            class="primary-small download-wide"
            :disabled="locked"
            @click="installGame"
          >
            <svg><use href="#i-download" /></svg>
            开始下载并安装
          </button>
          <button
            v-else
            class="primary-small download-wide"
            :disabled="
              locked ||
              preparing ||
              !instanceName.trim() ||
              !gameRoot.trim() ||
              optifineIncompatible ||
              (!pack && componentLoading)
            "
            @click="prepareInstallation"
          >
            {{ preparing ? '正在读取安装信息…' : '确认安装配置' }}
          </button>
          <p class="download-hint">
            安装完成后自动加入游戏库。实例名称重复时会提示更换，不覆盖已有游戏。
          </p>
        </aside>
      </div>
    </template>
  </div>
</template>
