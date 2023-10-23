import { chromium, ChromiumBrowserContext } from "playwright";
import { PostPageInstance } from "../utility/post-page-instance";
import { promisify } from "util";
import { mkdtemp } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { wait } from "../utility/utils";
import { NGROK_DASHBOARD } from "../mocks";

const makeTmpDir = promisify(mkdtemp);

export async function setupBrowser() {
  const launchArgs: string[] = [
    `--no-sandbox`,
  ];

  const tmpDir = await makeTmpDir(join(tmpdir(), "ext-data-"));
  const context = (await chromium.launchPersistentContext(tmpDir, {
    args: launchArgs,
    headless: false,
    slowMo: 100,
  })) as ChromiumBrowserContext;
  await context.grantPermissions(["clipboard-read"]);
  return {
    context,
  };
}
type Await<T> = T extends PromiseLike<infer U> ? U : T;

export type BrowserDriver = Await<ReturnType<typeof setupBrowser>>;

export async function getPOSTPage() {
  // First initialize a chromium browser where we can load our pages
  let browser = await setupBrowser();
  // Added some random wait time to make sure the browser is loaded fully
  await wait(3000);
  // once we have loaded browser then load the post URL
  const postPage: PostPageInstance = await PostPageInstance.setupPage(browser, NGROK_DASHBOARD);
  return postPage;
}