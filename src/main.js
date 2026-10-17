import { createApp } from 'vue';
import App from './App.vue';
import './styles.css';
import './library.css';
import './account.css';
import { invoke } from '@tauri-apps/api/core';
document.addEventListener('contextmenu', (event) => event.preventDefault());
// Lower idle WebView cache usage without suspending real-time game logs.
let idleTimer;
let lowMemory = false;
function setMemory(low) {
  if (lowMemory === low) return;
  lowMemory = low;
  invoke('memory_mode', { low }).catch(() => {});
}

function resumeMemory() {
  setMemory(false);
  clearTimeout(idleTimer);
  idleTimer = setTimeout(() => setMemory(true), 15000);
}
for (const event of ['pointerdown', 'keydown', 'wheel', 'focus']) {
  window.addEventListener(event, resumeMemory, { passive: true });
}
window.addEventListener('blur', () => {
  clearTimeout(idleTimer);
  setMemory(true);
});
resumeMemory();
createApp(App).mount('#app');
