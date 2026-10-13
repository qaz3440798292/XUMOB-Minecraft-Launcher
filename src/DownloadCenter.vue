<script setup>
import { ref, computed, watch } from 'vue';
import { api } from './useLauncher';
import './downloads.css';
const props=defineProps({active:Boolean,settings:Object,version:Object,state:Object,locked:Boolean});
const emit=defineEmits(['changed','notification']);
const query=ref(''),kind=ref('mod'),sort=ref('relevance'),hits=ref([]),offset=ref(0),total=ref(0),searching=ref(false),searched=ref(false),selected=ref(null),versions=ref([]),choice=ref(''),detailLoading=ref(false),preparing=ref(false),installing=ref(false),error=ref(''),plan=ref(null),summary=ref('');
let searchGeneration=0,detailGeneration=0,planGeneration=0;
const types=[{id:'mod',label:'模组'},{id:'resourcepack',label:'资源包'},{id:'shader',label:'光影'}];
const hasInstance=computed(()=>Boolean(props.version&&props.settings.gameRoot));
const installingNow=computed(()=>installing.value||props.state?.phase==='downloading');
const noLoader=computed(()=>kind.value==='mod'&&hasInstance.value&&!/fabric|quilt|forge/i.test(props.version.loader));
const percent=computed(()=>Math.min(100,Math.round((props.state?.progress?.bytes||0)/Math.max(1,props.state?.progress?.totalBytes||0)*100)));
const chosen=computed(()=>versions.value.find(v=>v.id===choice.value));
const displayError=e=>String(e?.message||e);
function size(n){return n>=1048576?`${(n/1048576).toFixed(1)} MB`:`${Math.ceil((n||0)/1024)} KB`;}
function count(n){return new Intl.NumberFormat('zh-CN',{notation:'compact',maximumFractionDigits:1}).format(n||0);}
function clearSelection(){detailGeneration++;planGeneration++;selected.value=null;versions.value=[];choice.value='';plan.value=null;detailLoading.value=false;preparing.value=false;summary.value='';}
async function search(next=0){const generation=++searchGeneration;searching.value=true;error.value='';clearSelection();offset.value=next;
  try{const result=await api.searchContent(query.value.trim(),kind.value,next,sort.value);if(generation!==searchGeneration)return;hits.value=result.hits;total.value=Number(result.total)||0;searched.value=true;}
  catch(e){if(generation===searchGeneration){hits.value=[];total.value=0;searched.value=true;error.value=displayError(e);}}
  finally{if(generation===searchGeneration)searching.value=false;}
}
async function setKind(value){if(installingNow.value)return;kind.value=value;await search();}
async function selectProject(project){if(installingNow.value)return;clearSelection();selected.value=project;error.value='';detailLoading.value=true;const generation=++detailGeneration;
  try{const result=await api.contentVersions(project.id,kind.value);if(generation!==detailGeneration)return;selected.value={...project,...result.project};versions.value=result.versions;choice.value=result.versions.find(v=>v.channel==='release')?.id||result.versions[0]?.id||'';}
  catch(e){if(generation===detailGeneration)error.value=displayError(e);}
  finally{if(generation===detailGeneration)detailLoading.value=false;}
}
async function prepare(){if(!selected.value||!choice.value)return;error.value='';preparing.value=true;plan.value=null;const generation=++planGeneration;
  try{const result=await api.prepareContent(selected.value.id,choice.value,kind.value);if(generation===planGeneration)plan.value=result;}
  catch(e){if(generation===planGeneration)error.value=displayError(e);}
  finally{if(generation===planGeneration)preparing.value=false;}
}
async function install(){if(!plan.value||installingNow.value)return;installing.value=true;error.value='';summary.value='';
  try{const result=await api.installContent(plan.value.planId);summary.value=`安装完成 · 新增 ${result.installed} 个文件${result.alreadyInstalled?`，${result.alreadyInstalled} 个已存在`:''}`;plan.value=null;emit('changed');emit('notification',summary.value);}
  catch(e){summary.value='';const message=displayError(e);if(message==='下载已取消。'){summary.value='下载已取消，未安装新文件。';}else{error.value=message;}plan.value=null;}
  finally{installing.value=false;}
}
async function cancel(){try{await api.cancelContent();summary.value='正在取消下载…';}catch(e){error.value=displayError(e);}}
async function openProject(){try{await api.openContentPage(selected.value.id,kind.value);}catch(e){error.value=displayError(e);}}
watch(choice,()=>{planGeneration++;plan.value=null;summary.value='';});
watch(()=>JSON.stringify([props.settings?.gameRoot,props.settings?.versionId,props.settings?.isolated]),()=>{searchGeneration++;searching.value=false;searched.value=false;hits.value=[];clearSelection();if(props.active)search();});
watch(()=>props.active,value=>{if(value&&!searched.value&&!searching.value)search();},{immediate:true});
</script>
<template>
  <section id="view-downloads" class="view" :class="{active}">
    <div class="page-heading"><div><div class="eyebrow">DISCOVER YOUR NEXT ADVENTURE</div><h1>下载中心<span class="heading-dot">.</span></h1><p>从 Modrinth 发现模组、资源包与光影。</p></div><span class="modrinth-source"><span>M</span>Modrinth</span></div>
    <div class="download-instance"><svg><use href="#i-box"/></svg><div><strong>{{ hasInstance?settings.versionId:'尚未选择安装实例' }}</strong><small>{{ hasInstance?`Minecraft ${version.minecraft} · ${version.loader}`:'可以先浏览内容，在游戏库选择实例后即可安装。' }}</small></div><span v-if="hasInstance" class="download-match">自动匹配兼容版本</span></div>
    <div class="download-types" aria-label="下载内容类型"><button v-for="type in types" :key="type.id" :data-kind="type.id" :class="{selected:kind===type.id}" :disabled="installingNow||preparing" @click="setKind(type.id)">{{ type.label }}</button></div>
    <form class="download-search" @submit.prevent="search()"><label class="search-field"><svg><use href="#i-search"/></svg><input id="download-search" v-model="query" maxlength="120" placeholder="搜索项目名称或关键词…" aria-label="搜索 Modrinth" :disabled="installingNow||preparing"></label><select id="download-sort" v-model="sort" :disabled="installingNow||preparing" @change="search()" aria-label="排序"><option value="relevance">相关程度</option><option value="downloads">下载最多</option><option value="updated">最近更新</option><option value="newest">最新发布</option></select><button id="search-modrinth" class="primary-small" :disabled="searching||installingNow||preparing">{{ searching?'搜索中…':'搜索' }}</button></form>
    <p v-if="noLoader" class="download-hint">当前为原版或未支持的加载器实例。安装模组前，请在游戏库选择 Fabric、Forge、NeoForge 或 Quilt 实例。</p>
    <p v-if="kind==='shader'" class="download-hint">光影文件保存到 shaderpacks；在游戏内启用时需要 Iris 或 OptiFine。</p>
    <div v-if="error" id="download-error" class="download-error" role="alert">{{ error }}</div>
    <div v-if="summary" id="download-summary" class="download-success" role="status">{{ summary }}</div>
    <div class="download-layout">
      <div class="download-results"><div class="download-results-heading"><span>{{ searching?'正在连接 Modrinth…':`${count(total)} 个项目` }}</span><span v-if="total">第 {{ Math.floor(offset/20)+1 }} 页</span></div>
        <div v-if="searching" class="download-empty">正在寻找适合你的内容…</div>
        <div v-else-if="!hits.length" class="download-empty">{{ error?'暂时无法加载，请稍后重新搜索。':'没有找到匹配的项目，试试其他关键词。' }}</div>
        <button v-for="project in hits" v-else :key="project.id" class="download-project" :class="{selected:selected?.id===project.id}" :disabled="installingNow||preparing" :data-project="project.id" @click="selectProject(project)"><span class="download-project-mark">{{ project.title?.slice(0,1).toUpperCase() }}</span><span class="download-project-copy"><strong>{{ project.title }}</strong><span>{{ project.description }}</span><small>{{ project.author }}<i>·</i>{{ count(project.downloads) }} 次下载</small><span class="download-tags"><em v-for="tag in project.categories.slice(0,4)" :key="tag">{{ tag }}</em></span></span><span class="download-project-arrow">↗</span></button>
        <div v-if="total>20" class="download-pagination"><button id="download-prev" class="outline-button" :disabled="offset===0||searching||installingNow||preparing" @click="search(Math.max(0,offset-20))">上一页</button><button id="download-next" class="outline-button" :disabled="offset+20>=total||offset>=10000||searching||installingNow||preparing" @click="search(offset+20)">下一页</button></div>
      </div>
      <aside class="download-details">
        <template v-if="selected"><div class="eyebrow">MAKE IT PART OF YOUR WORLD</div><h2>{{ selected.title }}</h2><p>{{ selected.description }}</p><button id="open-modrinth-project" class="link-button" @click="openProject">在 Modrinth 查看项目 ↗</button><div class="download-detail-rule"></div>
          <p v-if="detailLoading" class="download-hint">正在读取兼容版本…</p>
          <template v-else><label class="field-label" for="download-version">{{ hasInstance?'兼容当前实例的版本':'项目版本' }}</label><select id="download-version" v-model="choice" :disabled="preparing||installingNow"><option v-if="!versions.length" value="">没有兼容的版本</option><option v-for="item in versions" :key="item.id" :value="item.id">{{ item.number||item.name }}{{ item.channel==='release'?'':` · ${item.channel}` }}</option></select><div v-if="chosen" class="download-version-meta"><span>{{ chosen.loaders.join(' / ') }}</span><span>{{ size(chosen.size) }}</span><span>{{ chosen.date?.slice(0,10) }}</span></div>
            <button v-if="!plan&&!installingNow" id="prepare-modrinth-install" class="primary-small download-wide" :disabled="!hasInstance||!choice||locked||preparing||noLoader" @click="prepare">{{ preparing?'正在检查前置与已有文件…':'查看安装清单' }}</button>
          </template>
          <div v-if="plan" id="download-install-plan" class="download-plan"><h3>本次安装</h3><p>{{ plan.files.length }} 个文件 · {{ size(plan.totalBytes) }}<span v-if="plan.dependencyCount"> · 包含 {{ plan.dependencyCount }} 个必要前置</span></p><ul><li v-for="file in plan.files" :key="file.folder+'/'+file.filename"><strong>{{ file.title }}<small v-if="file.dependency">前置</small></strong><span>{{ file.filename }}</span></li></ul><label class="field-label">安装到</label><code>{{ plan.context.gameDir }}</code><button v-if="!installingNow" id="install-modrinth-content" class="primary-small download-wide" :disabled="locked" @click="install">确认下载并安装</button></div>
          <div v-if="installingNow" class="download-progress" role="status"><div><strong>正在下载并校验</strong><span>{{ percent }}%</span></div><progress :value="percent" max="100" aria-label="下载进度"></progress><p>{{ state?.message }}</p><button id="cancel-content-download" class="outline-button download-wide" :disabled="state?.phase!=='downloading'" @click="cancel">取消下载</button></div>
          <p v-if="locked&&!installingNow" class="download-hint">游戏正在运行或准备中，结束后即可安装。</p>
        </template>
        <div v-else class="download-detail-empty"><svg><use href="#i-download"/></svg><h3>发现新的玩法</h3><p>选择一个项目，查看兼容版本及安装清单。</p><small>必要前置自动补全<br>官方文件源 · 文件完整性校验</small></div>
      </aside>
    </div>
  </section>
</template>
