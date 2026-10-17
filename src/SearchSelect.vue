<script setup>
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';

const props = defineProps({
  id: { type: String, required: true },
  label: { type: String, default: '选择项目' },
  modelValue: { type: String, default: '' },
  options: { type: Array, default: () => [] },
  disabled: Boolean,
  searchable: { type: Boolean, default: undefined },
  placeholder: { type: String, default: '搜索名称…' },
});
const emit = defineEmits(['update:modelValue', 'change']);
const open = ref(false);
const query = ref('');
const active = ref(0);
const trigger = ref(null);
const panel = ref(null);
const search = ref(null);
const position = ref({});
const selected = computed(() => props.options.find((option) => option.value === props.modelValue));
const showSearch = computed(() => props.searchable ?? props.options.length > 8);
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase();
  return term
    ? props.options.filter((option) =>
        `${option.label} ${option.meta || ''}`.toLocaleLowerCase().includes(term),
      )
    : props.options;
});

function place() {
  if (!trigger.value) return;
  const rect = trigger.value.getBoundingClientRect();
  const width = Math.min(420, Math.max(rect.width, 300), window.innerWidth - 24);
  const below = window.innerHeight - rect.bottom - 16;
  const above = rect.top - 16;
  const up = below < 250 && above > below;
  position.value = {
    position: 'fixed',
    left: `${Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12))}px`,
    width: `${width}px`,
    maxHeight: `${Math.max(150, Math.min(420, up ? above : below))}px`,
    ...(up
      ? { bottom: `${window.innerHeight - rect.top + 7}px` }
      : { top: `${rect.bottom + 7}px` }),
  };
}

async function toggle() {
  if (props.disabled) return;
  if (open.value) return close();
  query.value = '';
  active.value = Math.max(
    0,
    filtered.value.findIndex((option) => option.value === props.modelValue),
  );
  place();
  open.value = true;
  await nextTick();
  if (showSearch.value) search.value?.focus();
}

function close(focus = false) {
  open.value = false;
  if (focus) trigger.value?.focus();
}

function choose(option) {
  if (option.disabled) return;
  emit('update:modelValue', option.value);
  emit('change', option.value);
  close(true);
}

function keydown(event) {
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

function outside(event) {
  if (!panel.value?.contains(event.target) && !trigger.value?.contains(event.target)) close();
}
function scroll(event) {
  if (!panel.value?.contains(event.target)) close();
}
watch(query, () => (active.value = 0));
watch(
  () => props.disabled,
  (disabled) => disabled && close(),
);
watch(open, (value) => {
  if (value) {
    window.addEventListener('pointerdown', outside, true);
    window.addEventListener('scroll', scroll, true);
    window.addEventListener('resize', place);
  } else {
    window.removeEventListener('pointerdown', outside, true);
    window.removeEventListener('scroll', scroll, true);
    window.removeEventListener('resize', place);
  }
});
onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', outside, true);
  window.removeEventListener('scroll', scroll, true);
  window.removeEventListener('resize', place);
});
</script>

<template>
  <button
    :id="id"
    ref="trigger"
    type="button"
    class="version-picker search-select"
    role="combobox"
    :aria-label="label"
    :aria-expanded="open"
    aria-haspopup="listbox"
    :disabled="disabled"
    @click="toggle"
    @keydown="keydown"
  >
    <span>
      <strong>{{ selected?.label || '请选择' }}</strong>
      <small v-if="selected?.meta">{{ selected.meta }}</small>
    </span>
    <svg viewBox="0 0 24 24"><path d="m7 10 5 5 5-5" /></svg>
  </button>
  <Teleport to="body">
    <div
      v-if="open"
      ref="panel"
      class="version-picker-panel"
      :style="position"
      @keydown="keydown"
    >
      <div
        v-if="showSearch"
        class="version-picker-search"
      >
        <svg viewBox="0 0 24 24">
          <circle
            cx="10"
            cy="10"
            r="6"
          />
          <path d="m15 15 6 6" />
        </svg>
        <input
          ref="search"
          v-model="query"
          :placeholder="placeholder"
          :aria-label="placeholder"
        />
      </div>
      <div class="version-picker-count">
        {{ label }}
        <span>{{ filtered.length }}</span>
      </div>
      <div
        class="version-picker-list"
        role="listbox"
        :aria-label="label"
      >
        <button
          v-for="(option, index) in filtered"
          :key="option.value"
          type="button"
          role="option"
          :data-index="index"
          :aria-selected="modelValue === option.value"
          :class="{ highlighted: active === index }"
          :disabled="option.disabled"
          @pointermove="active = index"
          @click="choose(option)"
        >
          <span class="version-picker-copy">
            <strong>{{ option.label }}</strong>
            <small v-if="option.meta">{{ option.meta }}</small>
          </span>
          <svg
            v-if="modelValue === option.value"
            class="version-picker-check"
            viewBox="0 0 24 24"
          >
            <path d="m5 12 4 4L19 6" />
          </svg>
        </button>
        <p v-if="!filtered.length">没有匹配的项目</p>
      </div>
    </div>
  </Teleport>
</template>
