<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { api } from './useLauncher';
import './account.css';
const props=defineProps({account:Object,username:String,locked:Boolean});
const emit=defineEmits(['changed','notification']);
const dialog=ref(null),tab=ref('offline'),offlineName=ref(''),clientId=ref(''),challenge=ref(null),working=ref(false),error=ref(''),now=ref(Date.now());
let pollTimer,clockTimer,generation=0;
const remaining=computed(()=>Math.max(0,(challenge.value?.expiresAt||0)-Math.floor(now.value/1000)));
const expiresLabel=computed(()=>`${Math.floor(remaining.value/60)}:${String(remaining.value%60).padStart(2,'0')}`);
async function changed(){emit('changed');}
function open(){if(dialog.value.open)return;tab.value=props.account?.mode||'offline';offlineName.value=props.username||'Explorer';clientId.value=props.account?.clientId||'';error.value='';dialog.value.showModal();}
async function cancel(){generation++;clearTimeout(pollTimer);clearInterval(clockTimer);challenge.value=null;working.value=false;await api.cancelLogin();}
async function close(){dialog.value.close();try{await cancel();}catch{}await changed();}
async function attempt(action){error.value='';try{await action();}catch(e){error.value=String(e?.message||e);}}
async function useOffline(){await api.save({username:offlineName.value.trim()});await api.selectAccount('offline');await changed();await close();emit('notification','已切换为离线账户');}
async function useMicrosoft(){await api.selectAccount('microsoft');await changed();await close();emit('notification','已切换为正版账户');}
async function logout(){await cancel();await api.logoutAccount();await changed();emit('notification','已移除这个启动器保存的正版登录');}
async function saveClient(){await api.configureAccount(clientId.value.trim());await changed();emit('notification','登录应用配置已保存');}
async function poll(id,session){if(id!==generation||!dialog.value.open)return;try{
  const result=await api.pollLogin(session);
  if(id!==generation)return;
  if(result.status==='complete'){clearInterval(clockTimer);challenge.value=null;working.value=false;await changed();emit('notification','微软正版登录成功');return;}
  pollTimer=setTimeout(()=>poll(id,session),Math.max(1,result.retryAfter||5)*1000);
}catch(e){if(id!==generation)return;error.value=String(e?.message||e);await cancel();await changed();}}
async function login(){error.value='';working.value=true;const id=++generation;
  try{const result=await api.startLogin();if(id!==generation)return;challenge.value=result;now.value=Date.now();clockTimer=setInterval(()=>now.value=Date.now(),1000);pollTimer=setTimeout(()=>poll(id,result.sessionId),result.interval*1000);}
  catch(e){if(id===generation){working.value=false;error.value=String(e?.message||e);await changed();}}
}
async function copyCode(){const input=document.getElementById('microsoft-user-code');input.select();try{await navigator.clipboard.writeText(challenge.value.userCode);}catch{if(!document.execCommand('copy'))throw new Error('请选中代码后手动复制。');}emit('notification','登录代码已复制');}
onMounted(()=>window.addEventListener('launcher:account-open',open));
onBeforeUnmount(()=>{window.removeEventListener('launcher:account-open',open);clearTimeout(pollTimer);clearInterval(clockTimer);api.cancelLogin().catch(()=>{});});
</script>
<template>
  <dialog id="account-dialog" ref="dialog" @cancel.prevent="close">
    <div class="dialog-heading"><div><div class="eyebrow">YOUR IDENTITY</div><h2>游戏账户</h2></div><button id="close-account" class="dialog-close" @click="close" aria-label="关闭账户设置">×</button></div>
    <div class="account-tabs"><button id="account-tab-offline" :class="{selected:tab==='offline'}" :disabled="working" @click="tab='offline';error=''">离线账户</button><button id="account-tab-microsoft" :class="{selected:tab==='microsoft'}" :disabled="working" @click="tab='microsoft';error=''">微软正版</button></div>
    <div v-if="tab==='offline'" class="account-content"><label class="field-label" for="account-offline-name">离线用户名</label><input id="account-offline-name" v-model="offlineName" maxlength="16" :disabled="locked"><p class="field-help">用于本地游玩和允许离线身份的服务器。3–16 位字母、数字或下划线。</p><div class="dialog-actions"><button class="outline-button" @click="close">关闭</button><button id="use-offline-account" class="primary-small" :disabled="locked" @click="attempt(useOffline)">使用离线账户</button></div></div>
    <div v-else class="account-content">
      <div v-if="account?.profile" class="account-profile"><span class="pixel-avatar"><i></i></span><div><strong id="microsoft-profile-name">{{ account.profile.name }}</strong><small>{{ account.signedIn?'已保存正版登录':'请重新登录' }}</small></div><button id="logout-microsoft" class="link-button" :disabled="locked||working" @click="attempt(logout)">退出登录</button></div>
      <template v-if="challenge"><p class="account-intro">在微软页面登录并输入下方代码。完成授权后，这里会自动继续。</p><label class="field-label" for="microsoft-user-code">登录代码</label><div class="input-action"><input id="microsoft-user-code" :value="challenge.userCode" readonly @click="$event.target.select()"><button id="copy-login-code" @click="attempt(copyCode)">复制代码</button></div><p class="field-help account-login-url">{{ challenge.verificationUri }}</p><div class="account-wait" role="status"><span class="status-dot"></span>等待你在微软页面完成授权 <span>{{ expiresLabel }}</span></div><div class="dialog-actions"><button id="cancel-microsoft-login" class="outline-button" @click="attempt(cancel)">取消登录</button><button id="open-microsoft-login" class="primary-small" @click="attempt(()=>api.openLoginBrowser(challenge.sessionId))">打开微软登录页 ↗</button></div></template>
      <template v-else><p class="account-intro">登录拥有 Minecraft Java 版的微软账户，使用正版角色进入游戏和正版服务器。密码只在微软页面输入。</p><details class="account-client-config" :open="!account?.configured"><summary>登录应用配置</summary><label class="field-label" for="microsoft-client-id">微软应用 Client ID</label><div class="input-action"><input id="microsoft-client-id" v-model="clientId" :disabled="working||locked" placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" spellcheck="false"><button id="save-microsoft-client" :disabled="working||locked" @click="attempt(saveClient)">保存</button></div><p class="field-help">使用已启用公共客户端流程并获准访问 Minecraft 服务的应用。无需 Client Secret。</p></details><div class="dialog-actions"><button class="outline-button" @click="close">关闭</button><button v-if="account?.signedIn&&account.mode!=='microsoft'" id="use-microsoft-account" class="outline-button" :disabled="working||locked" @click="attempt(useMicrosoft)">使用已登录账户</button><button id="start-microsoft-login" class="primary-small" :disabled="working||locked||!account?.configured" @click="login">{{ working?'正在连接微软…':account?.signedIn?'更换微软账户':'登录微软账户' }}</button></div></template>
      <p class="field-help account-storage-note">登录凭据由 Windows 加密保存；启动时自动验证并刷新。</p>
    </div>
    <div v-if="error" id="account-error" class="account-error" role="alert">{{ error }}</div>
    <p v-if="locked" class="field-help">游戏正在运行或准备中，请先结束游戏再更换账户。</p>
  </dialog>
</template>
