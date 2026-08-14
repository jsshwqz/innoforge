const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on('pageerror', e => console.log('PAGEERROR:', e.message));
  page.on('console', m => console.log('CONSOLE[' + m.type() + ']:', m.text()));

  await page.goto('http://127.0.0.1:3000/idea', { waitUntil: 'load' });
  await page.waitForTimeout(3000);

  console.log('=== API: GET /api/idea/list ===');
  const listResp = await page.evaluate(async () => {
    const r = await fetch('/api/idea/list');
    const j = await r.json();
    return { status: r.status, count: j.ideas ? j.ideas.length : 0, first: j.ideas ? j.ideas[0] : null };
  });
  console.log(JSON.stringify(listResp));

  const ideaId = '10171993-4285-4f46-aecb-18862bb92095';
  console.log('\n=== API: GET /api/idea/' + ideaId + ' ===');
  const ideaResp = await page.evaluate(async (id) => {
    const r = await fetch('/api/idea/' + id);
    const t = await r.text();
    return { status: r.status, body: t.substring(0, 500) };
  }, ideaId);
  console.log(JSON.stringify(ideaResp));

  console.log('\n=== Click first item via JS call ===');
  // Manually invoke loadIdea to see errors
  await page.evaluate(async (id) => {
    try {
      const r = await fetch('/api/idea/' + id);
      const data = await r.json();
      console.log('API response:', JSON.stringify(data).substring(0, 300));
      if (data.status !== 'ok') throw new Error('not ok: ' + data.status);
      const idea = data.idea || data;
      window.currentIdeaId = id;
      window._testIdea = idea;
      console.log('idea loaded, keys:', Object.keys(idea || {}).join(','));
    } catch(e) {
      console.log('ERROR:', e.message);
    }
  }, ideaId);
  await page.waitForTimeout(2000);

  const keys = await page.evaluate(() => Object.keys(window._testIdea || {}).join(', '));
  console.log('idea keys:', keys);
  const title = await page.evaluate(() => window._testIdea ? window._testIdea.title : 'NONE');
  console.log('idea title:', title);

  await browser.close();
})();