const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage();
  page.on('console', m => console.log('CONSOLE:', m.type(), m.text()));
  page.on('pageerror', e => console.log('PAGEERROR:', e.message));
  await page.goto('http://127.0.0.1:3000/idea', { waitUntil: 'load' });
  await page.waitForTimeout(2000);
  const html = await page.evaluate(() => document.getElementById('history-list').innerHTML);
  const count = await page.evaluate(() => document.querySelectorAll('#history-list .idea-history-item').length);
  console.log('history-list items:', count);
  console.log('history-list html (first 600):', html.substring(0, 600));
  await browser.close();
})();