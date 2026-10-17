<script setup>
import { computed } from 'vue';

const props = defineProps({ type: { type: String, default: 'release' } });
const material = computed(() => {
  if (props.type === 'snapshot') return 'command';
  if (props.type === 'april_fools') return 'gold';
  if (props.type === 'old_beta' || props.type === 'old_alpha' || props.type === 'early') return 'wool';
  return 'grass';
});
const label = computed(() => ({
  grass: '草方块',
  command: '命令方块',
  gold: '金块',
  wool: '羊毛方块',
}[material.value]));
</script>

<template>
  <svg class="game-block-svg" viewBox="0 0 48 48" role="img" :aria-label="label">
    <polygon
      points="24,3 44,14 24,25 4,14"
      :fill="material === 'grass' ? '#73ad43' : material === 'command' ? '#c57c3c' : material === 'gold' ? '#f3c431' : '#e9e6dc'"
    />
    <polygon
      points="4,14 24,25 24,46 4,35"
      :fill="material === 'grass' ? '#76513a' : material === 'command' ? '#a75e31' : material === 'gold' ? '#d69a19' : '#c9c6bd'"
    />
    <polygon
      points="44,14 24,25 24,46 44,35"
      :fill="material === 'grass' ? '#5e8738' : material === 'command' ? '#884726' : material === 'gold' ? '#b87912' : '#aaa79f'"
    />
    <template v-if="material === 'grass'">
      <polygon points="4,14 24,25 24,29 4,18" fill="#8dcf50" />
      <polygon points="44,14 24,25 24,29 44,18" fill="#76b943" />
      <polygon points="24,3 44,14 24,25 4,14" fill="#8bcf4d" />
      <path d="M10 14l3-2 2 1-3 2zm9-5 3-2 2 1-3 2zm10 7 3-2 2 1-3 2z" fill="#68a83a" />
    </template>
    <template v-else-if="material === 'command'">
      <path d="M9 15l15 8 15-8M9 20l15 8 15-8M9 25l15 8 15-8" fill="none" stroke="#e6a45e" stroke-width="1.5" />
      <path d="M10 17l11 6v12l-11-6zM38 17l-11 6v12l11-6z" fill="#603e32" stroke="#d99355" stroke-width="1" />
      <path d="M14 21l3 2v3l-3-2zm20 0-3 2v3l3-2z" fill="#f0c787" />
    </template>
    <template v-else-if="material === 'gold'">
      <path d="M24 3v22M4 14l20 11 20-11M14 8l20 11M34 8L14 19M4 24l20 11 20-11M4 30l20 11 20-11" fill="none" stroke="#ffdf63" stroke-width="1.2" opacity=".8" />
      <path d="M10 16l3-2 3 2-3 2zm18 0 3-2 3 2-3 2zM17 28l3-2 3 2-3 2zm12 7 3-2 3 2-3 2z" fill="#ffe883" />
    </template>
    <template v-else>
      <path d="M7 16l3-2 3 2-3 2zm10 6 3-2 3 2-3 2zm-9 7 3-2 3 2-3 2zm19-13 3-2 3 2-3 2zm-2 12 3-2 3 2-3 2zm8 5 3-2 3 2-3 2z" fill="#f8f6ef" />
      <path d="M24 4l4 2-4 2-4-2zM4 26l4-2v4l-4 2zM40 29l4-2v4l-4 2z" fill="#b9b6ae" />
    </template>
    <path d="M4 14l20 11 20-11M24 25v21" fill="none" stroke="#263126" stroke-opacity=".36" stroke-width="1" />
  </svg>
</template>
