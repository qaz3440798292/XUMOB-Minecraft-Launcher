import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';

const markdown = new MarkdownIt({ html: true, linkify: true, breaks: true, typographer: false });
export function contentUrl(value, provider = 'modrinth') {
  try {
    const raw = String(value || '').trim();
    const isModrinthLinkout = /^\/?linkout(?:[?#]|$)/i.test(raw);
    const base =
      provider === 'curseforge' && !isModrinthLinkout
        ? 'https://www.curseforge.com/'
        : 'https://modrinth.com/';
    let url = new URL(raw, base);

    // Mod descriptions can contain Modrinth's own external-link wrapper,
    // including when the description is being shown from CurseForge.
    for (let attempt = 0; attempt < 2; attempt++) {
      if (
        !['modrinth.com', 'www.modrinth.com'].includes(url.hostname.toLowerCase()) ||
        url.pathname !== '/linkout'
      ) {
        break;
      }
      let target = url.searchParams.get('remoteUrl') || '';
      if (!/^https:\/\//i.test(target)) target = decodeURIComponent(target);
      url = new URL(target);
    }

    return url.protocol === 'https:' && !url.username && !url.password ? url.href : '';
  } catch {
    return '';
  }
}
export function renderDescription(body, provider = 'modrinth') {
  const clean = DOMPurify.sanitize(markdown.render(String(body || '')), {
    ALLOWED_TAGS: [
      'p',
      'br',
      'hr',
      'h1',
      'h2',
      'h3',
      'h4',
      'h5',
      'h6',
      'strong',
      'em',
      'b',
      'i',
      's',
      'del',
      'blockquote',
      'ul',
      'ol',
      'li',
      'pre',
      'code',
      'a',
      'img',
      'table',
      'thead',
      'tbody',
      'tr',
      'th',
      'td',
      'details',
      'summary',
      'div',
      'span',
      'sup',
      'sub',
      'kbd',
    ],
    ALLOWED_ATTR: [
      'href',
      'src',
      'alt',
      'title',
      'width',
      'height',
      'colspan',
      'rowspan',
      'align',
      'open',
    ],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
  });
  const doc = new DOMParser().parseFromString(clean, 'text/html');
  for (const node of doc.querySelectorAll('a[href],img[src]')) {
    const attr = node.tagName === 'IMG' ? 'src' : 'href';
    const url = contentUrl(node.getAttribute(attr), provider);
    if (url) node.setAttribute(attr, url);
    else node.removeAttribute(attr);
    if (node.tagName === 'IMG') {
      node.setAttribute('loading', 'lazy');
      node.setAttribute('referrerpolicy', 'no-referrer');
    } else node.setAttribute('rel', 'noopener noreferrer');
  }
  return doc.body.innerHTML;
}
export function instanceLoader(label) {
  return (
    ['neoforge', 'fabric', 'quilt', 'forge'].find((loader) =>
      String(label || '')
        .toLowerCase()
        .includes(loader),
    ) || ''
  );
}
export function isCompatible(file, kind, version) {
  if (!file || !version || !file.gameVersions?.includes(version.minecraft)) return false;
  if (['server_only', 'dedicated_server_only'].includes(file.environment)) return false;
  const loaders = file.loaders || [];
  if (kind === 'mod')
    return (
      Boolean(instanceLoader(version.loader)) && loaders.includes(instanceLoader(version.loader))
    );
  if (kind === 'resourcepack') return loaders.includes('minecraft');
  return kind === 'shader' && loaders.some((loader) => ['iris', 'optifine'].includes(loader));
}
