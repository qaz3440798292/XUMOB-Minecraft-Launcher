<script setup>
import { computed, ref, watch } from 'vue';
import steveAvatar from './assets/steve-avatar.svg';

const props = defineProps({
  account: { type: Object, default: null },
  small: Boolean,
});

const failed = ref(false);
const failedTexture = ref(false);
const microsoft = computed(() => props.account?.mode === 'microsoft');
const skinUrl = computed(() => props.account?.profile?.skinUrl || props.account?.skinUrl || '');
const hasSkinTexture = computed(() => Boolean(skinUrl.value));
const avatarUrl = computed(() => {
  const id = microsoft.value ? props.account?.profile?.id?.replaceAll('-', '') : '';
  return id ? `https://mc-heads.net/avatar/${encodeURIComponent(id)}/64` : '';
});
watch([skinUrl, avatarUrl], () => {
  failed.value = false;
  failedTexture.value = false;
});
</script>

<template>
  <span
    v-if="hasSkinTexture && !failedTexture"
    class="skin-avatar-crop"
    :class="{ small }"
  >
    <img
      class="skin-avatar-front"
      :src="skinUrl"
      alt=""
      @error="failedTexture = true"
    />
    <img
      class="skin-avatar-hat"
      :src="skinUrl"
      alt=""
      @error="failedTexture = true"
    />
  </span>
  <img
    v-else-if="microsoft && avatarUrl && !failed"
    class="player-skin-avatar"
    :class="{ small }"
    :src="avatarUrl"
    alt="Minecraft 角色皮肤头像"
    @error="failed = true"
  />
  <img
    v-else
    class="player-avatar-steve"
    :class="{ small }"
    :src="steveAvatar"
    alt="Steve 默认皮肤头像"
  />
</template>
