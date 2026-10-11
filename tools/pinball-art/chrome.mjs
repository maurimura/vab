// Headless Chrome for drawing the tables: the GPU, file URLs allowed (the pages are ES modules).
import puppeteer from "puppeteer-core";

const CHROME =
  process.env.CHROME ??
  (process.platform === "darwin"
    ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    : "/usr/bin/google-chrome");

/** A page at `url`, `scale` device pixels per CSS pixel, once it says it's ready. */
export async function open(url, { width = 1920, height = 1080, scale = 1 } = {}) {
  const browser = await puppeteer.launch({
    executablePath: CHROME,
    headless: "new",
    args: ["--allow-file-access-from-files", "--force-color-profile=srgb", "--font-render-hinting=none"],
    defaultViewport: { width, height, deviceScaleFactor: scale },
  });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  await page.goto(url);
  await page.waitForFunction(() => window.READY === true, { timeout: 60000 });
  if (errors.length) throw new Error(errors.join("\n"));
  return { browser, page };
}
