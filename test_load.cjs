const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on('pageerror', e => console.log('PAGEERROR:', e.message));
  page.on('console', m => console.log('CONSOLE[' + m.type() + ']:', m.text()));

  await page.goto('http://127.0.0.1:3000/idea', { waitUntil: 'load' });
  await page.waitForTimeout(3000);

  // Test loadIdea directly with the done boiler idea
  const boilerId = 'ec9a6035-fa39-473c-90c1-1fd0483a416d';
  console.log('=== Calling loadIdea directly ===');
  await page.evaluate(async (id) => {
    try {
      console.log('Calling loadIdea for', id);
      await loadIdea(id);
      console.log('loadIdea returned, currentIdeaId =', window.currentIdeaId);
    } catch(e) {
      console.log('loadIdea threw:', e.message, e.stack);
    }
  }, boilerId);
  await page.waitForTimeout(2000);

  const ideaId = await page.evaluate(() => window.currentIdeaId);
  console.log('currentIdeaId:', ideaId);

  const analysis = await page.evaluate(() => {
    const el = document.getElementById('analysis-area');
    return el ? el.innerHTML.substring(0, 300) : 'NONE';
  });
  console.log('analysis-area:', analysis);

  const score = await page.evaluate(() => document.getElementById('score-area').innerHTML.substring(0, 200));
  console.log('score-area:', score);

  // Click via DOM
  console.log('\n=== Click item via mouse ===');
  await page.evaluate(() => {
    const items = document.querySelectorAll('.idea-history-item');
    for (let i = 0; i < items.length; i++) {
      if (items[i].textContent.includes('2000')) {
        items[i].dispatchEvent(new MouseEvent('click', {bubbles: true}));
        console.log('dispatched click on item', i);
        break;
      }
    }
  });
  await page.waitForTimeout(3000);

  const ideaId2 = await page.evaluate(() => window.currentIdeaId);
  console.log('currentIdeaId after click:', ideaId2);
  const analysis2 = await page.evaluate(() => document.getElementById('analysis-area').innerHTML.substring(0, 200));
  console.log('analysis-area after click:', analysis2);

  // Check if displayResults was called
  const score2 = await page.evaluate(() => document.getElementById('score-area').innerHTML.substring(0, 200));
  console.log('score-area after click:', score2);

  await browser.close();
})();