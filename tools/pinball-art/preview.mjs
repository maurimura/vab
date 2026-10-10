// Draws a table as the mockup shows it: node preview.mjs saloon [out.png]
import path from "node:path";
import { open } from "./chrome.mjs";

const [table = "saloon", out = `${table}-preview.png`] = process.argv.slice(2);
const { browser, page } = await open("file://" + path.resolve(table, "table.html"));
await page.screenshot({ path: out });
await browser.close();
console.log("drew", out);
