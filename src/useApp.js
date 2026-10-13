import { computed, reactive, ref, shallowRef, nextTick, onMounted, onBeforeUnmount } from 'vue';
import { api, onState, onLogs } from './useLauncher';
export function useApp() {
  const labels = { home: '开始游戏', versions: '游戏库', mods: '模组列表', downloads: '下载中心', settings: '启动设置', logs: '运行日志' };
  const model = shallowRef({ settings: { gameRoot:'', versionId:'', username:'Explorer', memoryMB:4096, javaPath:'', javaMode:'auto', isolated:'auto', width:1280,height:720,directories:[],extraJvmArgs:'',extraGameArgs:'' }, versions:[],suggestions:[],system:{totalMB:0,maxMB:16384},state:{phase:'idle',message:'正在读取本地配置…',pid:null} });
  const currentView=ref('home'), loaded=ref(false), javaList=shallowRef([]), mods=shallowRef([]), logs=shallowRef([]), modSearch=ref(''), follow=ref(true), scanning=ref(false), launching=ref(false), repairBusy=ref(false), homeMemory=ref(4096), toastText=ref(''), toastError=ref(false), toastVisible=ref(false), checkResult=shallowRef(null);
  const draft=reactive({...model.value.settings}); let toastTimer, logFrame=null, pendingLogs=[], unlistenState, unlistenLogs;
  const settings=computed(()=>model.value.settings), state=computed(()=>model.value.state), version=computed(()=>model.value.versions.find(v=>v.id===settings.value.versionId&&!v.error)), busy=computed(()=>['checking','repairing','preparing','stopping','downloading'].includes(state.value.phase)), running=computed(()=>state.value.phase==='running');
  const gb=n=>(n/1024).toFixed(1);
  const accountName=computed(()=>model.value.account?.mode==='microsoft'?model.value.account.profile?.name||'请登录':settings.value.username);
  const accountLabel=computed(()=>model.value.account?.mode==='microsoft'?'正版账户':'离线账户');
  const accountModeLabel=computed(()=>model.value.account?.mode==='microsoft'?'微软正版模式':'离线模式');
  function toast(message,error=false){clearTimeout(toastTimer);toastText.value=message;toastError.value=error;toastVisible.value=true;toastTimer=setTimeout(()=>toastVisible.value=false,error?6500:3500);}
  async function safely(action){try{return await action();}catch(error){toast(String(error?.message||error),true);return null;}}
  const message=computed(()=>loaded.value&&state.value.phase==='idle'&&!version.value ? settings.value.gameRoot?'当前目录没有可用版本，请添加已安装的游戏目录':'先添加游戏目录，选择你的第一个世界' : state.value.message);
  const directoryOptions=computed(()=>{const items=settings.value.directories.map(d=>({value:d.path,label:`${d.name} · ${d.path}`}));if(settings.value.gameRoot&&!items.some(d=>d.value===settings.value.gameRoot))items.push({value:settings.value.gameRoot,label:settings.value.gameRoot});return items.length?items:[{value:'',label:'尚未添加游戏目录'}];});
  const versionOptions=computed(()=>model.value.versions.length?model.value.versions.map(v=>({value:v.id,label:`${v.id}${v.error?' · 配置异常':''}`,disabled:Boolean(v.error)})):[{value:'',label:'没有找到游戏版本',disabled:true}]);
  const javaOptions=computed(()=>{const entries=[{value:'__auto__',label:'自动匹配当前版本（推荐）'},...javaList.value.map(j=>({value:j.path,label:j.label}))];const p=settings.value.javaPath;if(p&&!entries.some(e=>e.value.toLowerCase()===p.toLowerCase()))entries.unshift({value:p,label:`已选择 · ${p.split(/[\\/]/).slice(-3,-1).join('/')}`});return entries;});
  const selectedJava=computed(()=>{if(settings.value.javaMode==='manual')return javaList.value.find(j=>j.path.toLowerCase()===settings.value.javaPath.toLowerCase());const eligible=javaList.value.filter(j=>j.bits===64&&j.major>=(version.value?.javaRequired||8)&&(!version.value?.javaMaximum||j.major<=version.value.javaMaximum)).sort((a,b)=>a.major-b.major);return eligible.find(j=>j.major===version.value?.javaRecommended)||eligible[0];});
  const javaSummary=computed(()=>selectedJava.value?`Java ${selectedJava.value.major} · ${selectedJava.value.bits} 位`:settings.value.javaMode==='auto'?'自动匹配 Java':settings.value.javaPath?'已选择本地 Java':'未选择 Java');
  const javaDetail=computed(()=>selectedJava.value?`${settings.value.javaMode==='auto'?'自动选择 · ':''}${selectedJava.value.version}`:'按版本要求匹配，或手动选择');
  const javaPath=computed(()=>draft.javaChoice!== '__auto__'?draft.javaChoice||'请手动选择 Java。':selectedJava.value?`自动匹配：${selectedJava.value.path}`:`当前版本建议 Java ${version.value?.javaRecommended||'8 / 17 / 21'}，可手动选择。`);
  const filteredMods=computed(()=>mods.value.filter(mod=>mod.name.toLowerCase().includes(modSearch.value.trim().toLowerCase())));
  const launchLabel=computed(()=>running.value?'停止游戏':busy.value?'正在准备…':'启动游戏');
  const logStatus=computed(()=>running.value?`游戏进程 ${state.value.pid}`:state.value.phase==='error'?'出现启动问题':busy.value?'准备中':'等待启动');
  function syncDraft(){Object.assign(draft,settings.value,{isolated:String(settings.value.isolated),javaChoice:settings.value.javaMode==='auto'?'__auto__':settings.value.javaPath});homeMemory.value=settings.value.memoryMB;}
  async function refresh(){model.value=await api.snapshot();if(!version.value&&model.value.versions.some(v=>!v.error)&&!busy.value){await api.save({versionId:model.value.versions.find(v=>!v.error).id});model.value=await api.snapshot();}syncDraft();loaded.value=true;return model.value;}
  async function scanJava(force=false){scanning.value=true;try{javaList.value=await api.java(force);return javaList.value;}finally{scanning.value=false;}}
  async function changeVersion(id){await api.save({versionId:id});await refresh();await scanJava();}
  async function changeDirectory(root){await api.save({gameRoot:root});await refresh();await scanJava();}
  async function addDirectory(root){model.value=await api.addDirectory(root);syncDraft();loaded.value=true;await scanJava();}
  async function chooseFolder(){const root=await api.chooseFolder();if(!root)return;await addDirectory(root);toast(model.value.versions.length?`已读取 ${model.value.versions.length} 个游戏版本`:'这个目录没有已安装的 Minecraft 版本',!model.value.versions.length);}
  async function saveSettings(){const automatic=draft.javaChoice==='__auto__';await api.save({gameRoot:draft.gameRoot,versionId:draft.versionId,username:draft.username.trim(),javaMode:automatic?'auto':'manual',javaPath:automatic?'':draft.javaChoice,memoryMB:Number(draft.memoryMB),isolated:draft.isolated==='auto'?'auto':draft.isolated==='true',width:Number(draft.width),height:Number(draft.height),extraJvmArgs:draft.extraJvmArgs,extraGameArgs:draft.extraGameArgs});await refresh();toast('设置已保存');}
  function onHomeMemory(event){homeMemory.value=Number(event.target.value);draft.memoryMB=homeMemory.value;}
  async function saveHomeMemory(){const next=await api.save({memoryMB:homeMemory.value});model.value={...model.value,settings:next};}
  async function chooseJava(){const javaPath=await api.chooseJava();if(javaPath){await api.save({javaPath});await refresh();await scanJava();toast('Java 已更新');}}
  async function loadMods(){mods.value=await api.mods();}
  async function contentChanged(){await refresh();await loadMods();}
  async function selectView(view){currentView.value=view;await nextTick();document.querySelector('.main-content').scrollTop=0;if(view==='mods')safely(loadMods);if(view==='logs'&&follow.value)scrollLogs();}
  function editAccount(){window.dispatchEvent(new Event('launcher:account-open'));}
  function scrollLogs(){const out=document.getElementById('log-output');if(out)out.scrollTop=out.scrollHeight;}
  function queueLogs(entries){pendingLogs.push(...entries);if(pendingLogs.length>2500)pendingLogs=pendingLogs.slice(-2500);if(logFrame!==null)return;logFrame=requestAnimationFrame(async()=>{logFrame=null;logs.value=[...logs.value,...pendingLogs.splice(0)].slice(-1500);await nextTick();if(follow.value)scrollLogs();});}
  async function checkEnvironment(){checkResult.value=await api.check();await nextTick();const dialog=document.getElementById('check-dialog');if(!dialog.open)dialog.showModal();}
  function closeCheck(){document.getElementById('check-dialog').close();}
  async function repairFiles(){repairBusy.value=true;try{await api.repair();toast('基础依赖已补全');await checkEnvironment();}finally{repairBusy.value=false;}}
  async function launchGame(){if(running.value){await api.stop();return;}if(launching.value)return;launching.value=true;try{await api.save({versionId:settings.value.versionId,memoryMB:homeMemory.value});const result=await api.launch();await refresh();toast(`游戏进程已启动 · PID ${result.pid}`);}finally{launching.value=false;}}
  function clearLogs(){pendingLogs=[];logs.value=[];toast('已清空当前日志显示，日志文件仍然保留');}
  function drag(event){if(event.button===0&&!event.target.closest('button,input,select,.no-drag'))safely(()=>api.windowAction('drag'));}
  onMounted(()=>safely(async()=>{unlistenState=await onState(value=>model.value={...model.value,state:value});unlistenLogs=await onLogs(queueLogs);await refresh();queueLogs(model.value.logs||[]);await scanJava();}));
  onBeforeUnmount(()=>{unlistenState?.();unlistenLogs?.();clearTimeout(toastTimer);if(logFrame!==null)cancelAnimationFrame(logFrame);});
  return {accountName,accountLabel,accountModeLabel,api,labels,model,settings,state,version,currentView,loaded,busy,running,message,directoryOptions,versionOptions,javaOptions,selectedJava,javaSummary,javaDetail,javaPath,filteredMods,mods,modSearch,logs,follow,draft,homeMemory,scanning,launching,repairBusy,toastText,toastError,toastVisible,checkResult,launchLabel,logStatus,gb,safely,toast,refresh,scanJava,changeVersion,changeDirectory,addDirectory,chooseFolder,saveSettings,onHomeMemory,saveHomeMemory,chooseJava,selectView,editAccount,checkEnvironment,closeCheck,repairFiles,launchGame,clearLogs,contentChanged,drag};
}
