<script setup>
import { computed } from 'vue';
import './download-details.css';

const props = defineProps({
  active: { type: Boolean, default: false },
  state: { type: Object, required: true },
  speed: { type: Number, default: 0 },
});
const emit = defineEmits(['back']);
const progress = computed(() => props.state?.progress || {});
const isByteProgress = computed(() => progress.value.unit === 'bytes');
const hasByteStats = computed(() => isByteProgress.value || Number.isFinite(progress.value.downloadedBytes));
const percent = computed(() => {
  const done = Number(isByteProgress.value ? progress.value.bytes : progress.value.fileIndex) || 0;
  const total = Number(isByteProgress.value ? progress.value.totalBytes : progress.value.fileCount) || 0;
  return total > 0 ? Math.min(100, Math.floor((done / total) * 100)) : 0;
});
function formatBytes(value) {
  const bytes = Math.max(0, Number(value) || 0);
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}
function formatSpeed(value) {
  return `${formatBytes(value)}/s`;
}
</script>

<template>
  <section
    id="view-download-details"
    class="view download-details-view"
    :class="{ active }"
  >
    <div class="download-details-heading">
      <div>
        <div class="eyebrow">DOWNLOAD ACTIVITY</div>
        <h1>下载详情<span class="heading-dot">.</span></h1>
        <p>查看当前下载任务的实时进度、速度和资源数量。</p>
      </div>
      <button class="outline-button" @click="emit('back')">返回下载中心</button>
    </div>

    <article class="download-detail-card">
      <div class="download-detail-title">
        <span class="download-detail-icon"><svg><use href="#i-download" /></svg></span>
        <div>
          <strong>{{ progress.filename || state.message || '正在准备下载任务…' }}</strong>
          <span>{{ state.phase === 'downloading_java' ? '正在下载 Java 运行环境' : state.phase === 'repairing' ? '正在补全游戏资源' : '正在下载并校验文件' }}</span>
        </div>
        <b>{{ percent }}%</b>
      </div>

      <div class="download-detail-progress" role="progressbar" :aria-valuenow="percent" aria-valuemin="0" aria-valuemax="100">
        <i :style="{ width: `${percent}%` }"></i>
      </div>

      <div class="download-detail-stats">
        <div>
          <span>下载速度</span>
          <strong>{{ hasByteStats ? formatSpeed(speed) : '正在统计' }}</strong>
        </div>
        <div>
          <span>资源数量</span>
          <strong>{{ progress.fileIndex || 0 }} / {{ progress.fileCount || '—' }}</strong>
        </div>
        <div>
          <span>{{ hasByteStats ? '已传输' : '当前任务' }}</span>
          <strong>{{ hasByteStats ? `${formatBytes(isByteProgress ? progress.bytes : progress.downloadedBytes)}${isByteProgress ? ` / ${progress.totalBytes ? formatBytes(progress.totalBytes) : '未知'}` : ''}` : (progress.filename || state.message || '准备中') }}</strong>
        </div>
      </div>
    </article>

    <article class="download-detail-note">
      <span class="download-detail-pulse"></span>
      <div>
        <strong>下载进行中</strong>
        <p>文件会在下载完成后进行校验；如果下载中断，可以重新发起任务。</p>
      </div>
    </article>
  </section>
</template>
