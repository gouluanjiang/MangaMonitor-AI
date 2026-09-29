import { expect, test } from "@playwright/test";
// Classification booklists are cancelled; existing local documents stay inert.
test("library and detail have no classification booklist actions", async ({
  page,
}) => {
  await page.addInitScript(() =>
    localStorage.setItem(
      "mangamonitor.workbench.booklists.v1",
      "legacy-document-kept",
    ),
  );
  await page.goto("/");
  await expect(
    page.getByRole("button", { name: "本地书单", exact: true }),
  ).toHaveCount(0);
  await expect(page.getByTestId("booklists-error")).toHaveCount(0);
  await expect(page.getByTestId("detail-booklist")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      localStorage.getItem("mangamonitor.workbench.booklists.v1"),
    ),
  ).toBe("legacy-document-kept");
});
