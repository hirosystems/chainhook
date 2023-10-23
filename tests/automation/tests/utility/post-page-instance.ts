import { Page } from "playwright-core";
import { BrowserDriver } from "./browser-instance";
import {
  idSelector,
  textSelector,
} from "./utils";
import { PostPageSelectors } from "../selectors/postPage.selectors";

const selectors = {
  $request: idSelector(PostPageSelectors.Request),
  $requestTab2: idSelector(PostPageSelectors.RequestTab2),
  $requestPane2: idSelector(PostPageSelectors.RequestPane2),
  $clearRequest: textSelector(PostPageSelectors.ClearRequest),
}

export class PostPageInstance {
  page: Page;
  browser: BrowserDriver;

  constructor(page: Page, browser: BrowserDriver) {
    this.page = page;
    this.browser = browser;
  }

  static async setupPage(browser: BrowserDriver, url: string) {
    const page: any = await browser.context.newPage();
    await page.goto(url);
    page.on("pageerror", (event: { message: any }) => {
      console.log("Error in loading page:", event.message);
    });
    return new this(page, browser);
  }

  async closeBrowser() {
    await this.browser.context.close();
  }

  async clearPOSTResult() {
    await this.page.click(selectors.$clearRequest);
  }

  async getPOSTResult(): Promise<any> {
    await this.page.waitForSelector(selectors.$request);
    await this.page.click(selectors.$requestTab2);
    const content = await this.page.innerText(selectors.$requestPane2);
    let parsedContent = {};
    if (content) {
      try {
        parsedContent = JSON.parse(content.split("Accept-Encoding: gzip")[1]);
      } catch (e) {
        console.log('Error parsing the response for POST result', e);
      }
    }
    await this.clearPOSTResult();
    return parsedContent;
  }
}
