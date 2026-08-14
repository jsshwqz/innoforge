const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on('pageerror', e => console.log('PAGEERROR:', e.message));

  await page.goto('http://127.0.0.1:3000/idea', { waitUntil: 'load' });
  await page.waitForTimeout(3000);

  // Check what the idea API returns for the 2000 boiler idea
  const boilerId = 'ec9a6035-fa39-473c-90c1-1fd0483a416d';
  const apiResp = await page.evaluate(async (id) => {
    const r = await fetch('/api/idea/' + id);
    return await r.json();
  }, boilerId);
  const idea = apiResp.idea;
  console.log('=== API /api/idea/:id keys ===');
  console.log('keys:', Object.keys(idea));
  console.log('has analysis?', !!idea.analysis, 'len:', idea.analysis ? idea.analysis.length : 0);
  console.log('analysis snippet:', idea.analysis ? idea.analysis.substring(0, 200) : 'EMPTY');
  console.log('discussion_summary:', idea.discussion_summary || 'EMPTY');
  console.log('status:', idea.status);

  // Check if there are discussion/chat messages API endpoints
  console.log('\n=== Checking for messages API ===');
  const endpoints = [
    '/api/idea/' + boilerId + '/messages',
    '/api/idea/' + boilerId + '/chat',
    '/api/idea/' + boilerId + '/discussion',
    '/api/idea/' + boilerId + '/conversations',
  ];
  for (const ep of endpoints) {
    const r = await page.evaluate(async (e) => {
      const resp = await fetch(e);
      if (!resp.ok) return { status: resp.status };
      const t = await resp.text();
      return { status: resp.status, body: t.substring(0, 300) };
    }, ep);
    console.log(ep + ':', JSON.stringify(r));
  }

  await browser.close();
})();