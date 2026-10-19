// Branch-based Pages hosts only docs/. Repository-relative references outside
// that folder stay useful on GitHub without duplicating source/example files.
(() => {
  const script = document.currentScript;
  const main = document.querySelector('main[data-source]');
  if (!main || !script) return;
  const { repository, branch } = script.dataset;
  const source = new URL(main.dataset.source, 'https://source.invalid/docs/');
  for (const node of main.querySelectorAll('a[href], img[src]')) {
    const attr = node.tagName === 'IMG' ? 'src' : 'href';
    const value = node.getAttribute(attr);
    if (!value || /^(?:[a-z][a-z\d+.-]*:|\/|#)/i.test(value)) continue;
    const target = new URL(value, source);
    if (target.pathname.startsWith('/docs/')) continue;
    const raw = attr === 'src' || target.searchParams.get('raw') === 'true';
    const host = raw ? 'https://raw.githubusercontent.com' : 'https://github.com';
    const route = raw ? '' : '/blob';
    node.setAttribute(attr, `${host}/${repository}${route}/${branch}${target.pathname}${target.hash}`);
  }
})();
