//! BrowserSession example — direct browser control.
//!
//! Run with:
//!   node examples/browser-session.mjs

import { BrowserSession } from '../index.js';

async function main() {
  console.log('Creating browser session...');
  const session = new BrowserSession({ headless: true });

  try {
    // Navigate to a page
    const url = await session.navigate('https://example.com');
    console.log(`Navigated to: ${url}`);

    // Get a snapshot
    const snapshotJson = await session.snapshot();
    const snapshot = JSON.parse(snapshotJson);
    console.log(`Snapshot has ${snapshot.nodes.length} nodes`);

    // Extract text
    const text = await session.extract();
    console.log(`Page text:\n${text.slice(0, 200)}...`);

    // Take a screenshot (base64)
    const screenshot = await session.screenshot();
    console.log(`Screenshot: ${screenshot.slice(0, 40)}... (${screenshot.length} chars base64)`);

    console.log('Done!');
  } finally {
    await session.close();
    console.log('Session closed.');
  }
}

main().catch(console.error);
