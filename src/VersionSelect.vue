<script setup>
import { computed, ref, watch, onBeforeUnmount, nextTick } from 'vue';
import { loaderMark } from './loaderIcons';
const props = defineProps({
  modelValue: String,
  options: { type: Array, default: () => [] },
  disabled: Boolean,
  label: { type: String, default: '选择游戏版本' },
  id: String,
});
const emit = defineEmits(['update:modelValue', 'change']);
const open = ref(false);
const query = ref('');
const active = ref(0);
const trigger = ref(null);
const panel = ref(null);
const searchInput = ref(null);
const position = ref({});
const filtered = computed(() =>
  props.options.filter((o) =>
    `${o.label} ${o.minecraft || ''} ${o.loader || ''}`
      .toLowerCase()
      .includes(query.value.trim().toLowerCase()),
  ),
);
const selected = computed(() => props.options.find((o) => o.value === props.modelValue));
/** 每行预解析一次加载器图标，避免模板里对同一行重复调用。 */
const rows = computed(() => filtered.value.map((item) => ({ ...item, mark: loaderMark(item) })));
const loader = (item) =>
  item?.loader ||
  ['NeoForge', 'Fabric', 'Quilt', 'Forge', 'OptiFine'].find((l) =>
    item?.label?.toLowerCase().includes(l.toLowerCase()),
  ) ||
  '原版';
function place() {
  if (!trigger.value) return;
  const r = trigger.value.getBoundingClientRect();
  const width = Math.min(410, Math.max(r.width, 300), window.innerWidth - 24);
  const below = window.innerHeight - r.bottom - 16;
  const above = r.top - 16;
  const up = below < 260 && above > below;
  position.value = {
    position: 'fixed',
    left: `${Math.max(12, Math.min(r.right - width, window.innerWidth - width - 12))}px`,
    width: `${width}px`,
    maxHeight: `${Math.max(120, Math.min(420, up ? above : below))}px`,
    ...(up ? { bottom: `${window.innerHeight - r.top + 7}px` } : { top: `${r.bottom + 7}px` }),
  };
}

async function toggle() {
  if (props.disabled) return;
  if (open.value) {
    close();
    return;
  }
  query.value = '';
  active.value = Math.max(
    0,
    filtered.value.findIndex((o) => o.value === props.modelValue),
  );
  place();
  open.value = true;
  await nextTick();
  searchInput.value?.focus();
  panel.value?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
}

function close(focus = false) {
  open.value = false;
  if (focus) trigger.value?.focus();
}

function choose(item) {
  if (item.disabled) return;
  emit('update:modelValue', item.value);
  emit('change', item.value);
  close(true);
}

function key(event) {
  if (event.key === 'Escape') {
    event.preventDefault();
    close(true);
  } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    active.value = Math.max(
      0,
      Math.min(filtered.value.length - 1, active.value + (event.key === 'ArrowDown' ? 1 : -1)),
    );
    nextTick(() =>
      panel.value
        ?.querySelector(`[data-index="${active.value}"]`)
        ?.scrollIntoView({ block: 'nearest' }),
    );
  } else if (event.key === 'Enter') {
    event.preventDefault();
    if (filtered.value[active.value]) choose(filtered.value[active.value]);
  } else if (event.key === 'Tab') close();
}

function outside(e) {
  if (!panel.value?.contains(e.target) && !trigger.value?.contains(e.target)) close();
}

function scroll(e) {
  if (!panel.value?.contains(e.target)) close();
}
watch(query, () => (active.value = 0));
watch(
  () => props.disabled,
  (value) => {
    if (value) close();
  },
);
watch(open, (value) => {
  for (const [name, fn] of [
    ['pointerdown', outside],
    ['scroll', scroll],
    ['resize', place],
  ]) {
    if (value) window.addEventListener(name, fn, true);
    else window.removeEventListener(name, fn, true);
  }
});
onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', outside, true);
  window.removeEventListener('scroll', scroll, true);
  window.removeEventListener('resize', place, true);
});
</script>
<template>
  <button
    ref="trigger"
    :id="id"
    class="version-picker"
    type="button"
    role="combobox"
    :aria-label="label"
    :aria-expanded="open"
    aria-haspopup="listbox"
    :aria-controls="`${id}-list`"
    :disabled="disabled"
    @click="toggle"
    @keydown.down.prevent="!open && toggle()"
  >
    <span>
      <strong>{{ selected?.label || '选择游戏版本' }}</strong>
      <small v-if="selected?.minecraft">
        Minecraft {{ selected.minecraft }}
        <i>·</i>
        {{ loader(selected) }}
      </small>
    </span>
    <svg viewBox="0 0 24 24"><path d="m7 10 5 5 5-5" /></svg>
  </button>
  <Teleport to="body">
    <div
      v-if="open"
      ref="panel"
      class="version-picker-panel"
      :style="position"
      @keydown="key"
    >
      <div class="version-picker-search">
        <svg viewBox="0 0 24 24">
          <circle
            cx="10"
            cy="10"
            r="6"
          />
          <path d="m15 15 6 6" />
        </svg>
        <input
          ref="searchInput"
          v-model="query"
          placeholder="搜索名称、游戏版本或加载器"
          aria-label="搜索已安装版本"
        />
      </div>
      <div class="version-picker-count">
        你的游戏版本
        <span>{{ rows.length }}</span>
      </div>
      <div
        :id="`${id}-list`"
        class="version-picker-list"
        role="listbox"
        :aria-label="label"
      >
        <button
          v-for="(item, index) in rows"
          :key="item.value"
          type="button"
          role="option"
          :data-index="index"
          :aria-selected="modelValue === item.value"
          :class="{ highlighted: active === index }"
          :disabled="item.disabled"
          @pointermove="active = index"
          @click="choose(item)"
        >
          <span class="version-picker-icon">
            <img
              v-if="item.iconData"
              class="pack-icon"
              :src="item.iconData"
              alt=""
              loading="lazy"
              decoding="async"
            />
            <img
              v-else-if="item.mark?.src"
              class="loader-mark"
              :class="`loader-${item.mark.key}`"
              :src="item.mark.src"
              :alt="item.mark.alt"
              loading="lazy"
              decoding="async"
            />
            <svg
              v-else
              viewBox="0 0 24 24"
            >
              <path d="m12 3 9 5-9 5-9-5Zm-9 5v10l9 5 9-5V8M12 13v10" />
            </svg>
          </span>
          <span class="version-picker-copy">
            <strong>{{ item.label }}</strong>
            <small>
              {{ item.minecraft ? `Minecraft ${item.minecraft}` : '本地实例' }}
              <em>{{ loader(item) }}</em>
            </small>
          </span>
          <svg
            v-if="modelValue === item.value"
            class="version-picker-check"
            viewBox="0 0 24 24"
          >
            <path d="m5 12 4 4L19 6" />
          </svg>
        </button>
        <p v-if="!rows.length">没有匹配的版本</p>
      </div>
    </div>
  </Teleport>
</template>
<style>
.version-picker {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-height: 48px;
  padding: 10px 12px;
  background: #131e17;
  border: 1px solid #3d5133;
  border-radius: 8px;
  text-align: left;
  transition:
    border-color 0.16s ease,
    background-color 0.16s ease,
    box-shadow 0.16s ease;
}
.version-picker:hover:not(:disabled):not([aria-expanded='true']) {
  border-color: #60784e;
  background: #17251a;
}
.version-picker[aria-expanded='true'] {
  border-color: #8eae69;
  background: #1b2a1e;
  box-shadow: 0 0 0 2px rgb(142 174 105 / 10%);
}
.version-picker > span {
  flex: 1;
  min-width: 0;
}
.version-picker strong {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  font-weight: 500;
  color: #d5e5c6;
}
.version-picker > span > small {
  display: block;
  color: #7d9671;
  font-size: 9px;
  margin-top: 5px;
}
.version-picker i {
  font-style: normal;
  margin: 0 7px;
  color: #536c49;
}
.version-picker > svg {
  width: 15px;
  height: 15px;
  color: #8faa7a;
  transition: transform 0.15s;
}
.version-picker[aria-expanded='true'] > svg {
  transform: rotate(180deg);
}
.version-picker-panel {
  z-index: 20000;
  display: flex;
  flex-direction: column;
  background: #18261d;
  border: 1px solid #526a42;
  border-radius: 11px;
  box-shadow: 0 15px 45px #0008;
  overflow: hidden;
  padding: 9px;
  transform-origin: top center;
  animation: version-picker-enter 0.16s cubic-bezier(0.2, 0.7, 0.2, 1);
}
@keyframes version-picker-enter {
  from { opacity: 0; transform: translateY(-5px) scale(0.99); }
  to { opacity: 1; transform: translateY(0) scale(1); }
}
.version-picker-search {
  display: flex;
  align-items: center;
  gap: 8px;
  border: 1px solid #3c5032;
  background: #101c14;
  border-radius: 7px;
  padding: 0 9px;
  flex-shrink: 0;
  transition: border-color 0.16s ease, box-shadow 0.16s ease;
}
.version-picker-search:focus-within {
  border-color: #80a367;
  box-shadow: 0 0 0 3px #badd9322;
}
.version-picker-search > svg {
  width: 15px;
  height: 15px;
  color: #839c74;
}
.version-picker-search input {
  border: 0 !important;
  background: transparent !important;
  padding-left: 0 !important;
  font-size: 11px !important;
}
.version-picker-search input:focus {
  border: 0 !important;
  box-shadow: none !important;
}
.version-picker-count {
  display: flex;
  justify-content: space-between;
  padding: 13px 7px 9px;
  color: #7b956d;
  font-size: 9px;
  flex-shrink: 0;
}
.version-picker-count > span {
  background: #2a3e26;
  border-radius: 4px;
  padding: 0 5px;
}
.version-picker-list {
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
}
.version-picker-list > button {
  display: flex;
  gap: 10px;
  align-items: center;
  width: 100%;
  padding: 11px 9px;
  margin-bottom: 3px;
  border: 1px solid transparent;
  border-radius: 7px;
  text-align: left;
  transition: background-color 0.14s ease, border-color 0.14s ease, transform 0.14s ease;
}
.version-picker-list > button.highlighted {
  background: #263a25;
  transform: translateX(2px);
}
.version-picker-list > button[aria-selected='true'] {
  background: #30492a;
  border-color: #597548;
}
.version-picker-icon {
  display: grid;
  place-items: center;
  width: 31px;
  height: 34px;
  background: #243920;
  border: 1px solid #435b37;
  border-radius: 6px;
  flex-shrink: 0;
  color: #afcc94;
}
.version-picker-icon svg {
  width: 18px;
  height: 18px;
}
.version-picker-icon .pack-icon {
  width: 100%;
  height: 100%;
  border-radius: 5px;
  object-fit: cover;
}
.version-picker-copy {
  min-width: 0;
  flex: 1;
  overflow: hidden;
}
.version-picker-copy > strong {
  display: block;
  font-size: 11px;
  font-weight: 500;
  color: #d4e6c5;
  overflow-wrap: anywhere;
}
.version-picker-copy > small {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 5px;
  font-size: 9px;
  color: #7d9871;
}
.version-picker-copy em {
  font-size: 8px;
  font-style: normal;
  border-radius: 4px;
  padding: 2px 5px;
  background: #354d2c;
  color: #b1d197;
}
.version-picker-check {
  width: 15px;
  height: 15px;
  color: #c2e99d;
}
.version-picker-list > p {
  text-align: center;
  padding: 25px 0;
  color: #8da17e;
  font-size: 11px;
}
.settings-two .version-picker {
  min-height: 48px;
}
</style>
