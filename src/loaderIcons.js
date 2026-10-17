/**
 * 客户端卡片加载器图标映射。
 *
 * 只依赖后端结构化产出的字段，不对版本显示名 / 整合包名字做字符串匹配：
 * - `item.modded`：布尔值，后端 `core.rs` 里由「是否识别到加载器标签」决定；
 * - `item.loader`：后端按固定顺序拼出的标签串，例如
 *   `"Fabric 0.16.14"`、`"Forge 43.5.1 + OptiFine HD U H9"`、`"原版"`。
 *
 * 一个客户端装了多个组件时（例如 Forge + OptiFine），按 `PRIMARY_ORDER`
 * 取「主要加载器」；原版用本地草方块图标；识别不到的类型返回 `null`，
 * 由调用方回退到 sprite 里的线框方块图标。
 */

/** 已收录的加载器 Logo（本地资源，来源与许可见 public/loaders/ATTRIBUTION.md）。 */
const MARKS = {
  quilt: { src: '/loaders/quilt.svg', label: 'Quilt' },
  fabric: { src: '/loaders/fabric.png', label: 'Fabric' },
  neoforge: { src: '/loaders/neoforge.png', label: 'NeoForge' },
  forge: { src: '/loaders/forge.svg', label: 'Forge' },
  optifine: { src: '/loaders/optifine.png', label: 'OptiFine' },
};

/**
 * 后端 `loader` 标签串里的标签前缀 → 类型 key（标签名固定，见 core.rs）。
 * LiteLoader 暂未收录本地 Logo，显式留空以便回退到方块图标。
 */
const TOKEN_KEYS = new Map([
  ['neoforge', 'neoforge'],
  ['forge', 'forge'],
  ['quilt', 'quilt'],
  ['fabric', 'fabric'],
  ['optifine', 'optifine'],
  ['liteloader', null],
]);

/**
 * 多组件客户端展示哪个 Logo：模组加载器优先于叠加组件（OptiFine / LiteLoader）。
 * 后端已保证 Quilt 优先于 Fabric、NeoForge 优先于 Forge。
 */
const PRIMARY_ORDER = ['neoforge', 'forge', 'quilt', 'fabric', 'optifine'];

/** 原版客户端：本地草方块图标（自绘像素风矢量，见 public/loaders/ATTRIBUTION.md）。 */
const VANILLA_MARK = {
  key: 'vanilla',
  src: '/loaders/vanilla.svg',
  label: '原版',
  alt: '原版 Minecraft 草方块',
};

/** 同一条 `loader` 串会在列表里反复出现，缓存解析结果。 */
const cache = new Map();

function parseKeys(loaderText) {
  const keys = new Set();
  for (const part of String(loaderText ?? '').split('+')) {
    const head = part.trim().split(/\s+/)[0];
    if (!head) continue;
    const key = TOKEN_KEYS.get(head.toLowerCase());
    if (key) keys.add(key);
  }
  return keys;
}

/**
 * 解析客户端版本条目，返回该卡片该展示的加载器图标。
 *
 * @param {{ error?: string, modded?: boolean, loader?: string }} item
 *   后端 `snapshot().versions` 里的条目，或由它派生出的选项对象。
 * @returns {{ key: string, src: string, label: string, alt: string } | null}
 *   原版返回草方块图标；类型无法识别时返回 `null`（回退线框方块图标）。
 */
export function loaderMark(item) {
  if (!item || item.error) return null;
  // `modded === false` 是后端明确判定的原版；缺这个字段（占位选项等）不算原版，
  // 继续往下解析 `loader`，解析不出就回退。
  if (item.modded === false) return VANILLA_MARK;

  const text = String(item.loader ?? '');
  if (cache.has(text)) return cache.get(text);

  const keys = parseKeys(text);
  const primary = PRIMARY_ORDER.find((key) => keys.has(key));
  const mark =
    primary && MARKS[primary]
      ? { key: primary, ...MARKS[primary], alt: `${MARKS[primary].label} 加载器图标` }
      : null;
  cache.set(text, mark);
  return mark;
}
