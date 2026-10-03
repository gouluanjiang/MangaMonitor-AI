import type { Page } from "@playwright/test";

export async function openUnifiedSearch(
  page: Page,
  mode: "作者" | "作品关键词" | "编号或链接" | "标签" = "作者",
) {
  await page.getByTestId("nav-discovery").click();
  await page
    .getByRole("group", { name: "搜索方式", exact: true })
    .getByRole("button", { name: mode, exact: true })
    .click();
}

export async function chooseSearchMode(
  page: Page,
  mode: "作品关键词" | "编号或链接" | "标签",
) {
  await page
    .getByRole("group", { name: "搜索方式", exact: true })
    .getByRole("button", { name: mode, exact: true })
    .click();
}

export async function continueSearch(page: Page) {
  await page
    .getByTestId("source-workbench")
    .getByRole("button", { name: "读取下一页", exact: true })
    .click();
}
