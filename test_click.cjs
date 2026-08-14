const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.on('pageerror', e => console.log('PAGEERROR:', e.message));
  page.on('console', m => { if (m.type() === 'error') console.log('CONSOLE_ERR:', m.text()); });

  await page.goto('http://127.0.0.1:3000/idea', { waitUntil: 'load' });
  await page.waitForTimeout(3000);

  console.log('=== Step 1: Find a DONE item with messages ===');
  // Find the item with max messages - that's the 2000 boiler one with 491 messages
  const doneItem = await page.evaluate(() => {
    const items = document.querySelectorAll('#history-list .idea-history-item');
    // Click first item
    if (items.length > 0) items[0].click();
    // Return info about first item
    return {
      clicked: items[0]?.textContent?.substring(0, 100),
      totalItems: items.length
    };
  });
  console.log('Clicked:', JSON.stringify(doneItem));

  await page.waitForTimeout(3000);

  console.log('\n=== Step 2: Check if idea loaded ===');
  const currentIdea = await page.evaluate(() => window.currentIdeaId);
  console.log('currentIdeaId:', currentIdea);

  // Check if score area has content
  const scoreHtml = await page.evaluate(() => document.getElementById('score-area').innerHTML.substring(0, 200));
  console.log('score-area:', scoreHtml);

  const analysisHtml = await page.evaluate(() => document.getElementById('analysis-area').innerHTML.substring(0, 200));
  console.log('analysis-area:', analysisHtml);

  // Check chat messages
  const chatHtml = await page.evaluate(() => {
    const el = document.getElementById('chat-messages');
    return el ? el.innerHTML.substring(0, 300) : 'NO CHAT ELEMENT';
  });
  console.log('chat-messages:', chatHtml);

  console.log('\n=== Step 3: Try clicking done item (2000 boiler) ===');
  // Find the item with '2000' in its title
  await page.evaluate(() => {
    const items = document.querySelectorAll('#history-list .idea-history-item');
    for (let i = 0; i < items.length; i++) {
      if (items[i].textContent.includes('2000')) {
        items[i].click();
        console.log('CLICKED ITEM INDEX:', i);
        break;
      }
    }
  });
  await page.waitForTimeout(4000);

  const idea2 = await page.evaluate(() => window.currentIdeaId);
  console.log('After click currentIdeaId:', idea2);

  const chat2 = await page.evaluate(() => {
    const el = document.getElementById('chat-messages');
    if (!el) return 'NO CHAT ELEMENT';
    const msgs = el.querySelectorAll('.chat-message, .message, .msg');
    return {
      rawLength: el.innerHTML.length,
      msgCount: msgs.length,
      first500: el.innerHTML.substring(0, 500)
    };
  });
  console.log('chat after done click:', JSON.stringify(chat2));

  await browser.close();
})();