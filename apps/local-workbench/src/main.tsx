import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import "./workbench.css";

const root = createRoot(document.getElementById("root")!);
root.render(<div role="status">正在打开…</div>);

async function openApplication() {
  // Load only this window's entry before its first mount. A root Suspense
  // retry can otherwise leave a ready entry behind its fallback when the
  // browser clock is paused or timers are suspended.
  const Application =
    window.location.hash === "#reader-window"
      ? (await import("./reader/ReaderWindow.tsx")).ReaderWindow
      : (await import("./App.tsx")).default;
  root.render(
    <StrictMode>
      <Application />
    </StrictMode>,
  );
}

void openApplication().catch(() => {
  root.render(<div role="alert">界面暂时无法打开，请重新打开程序。</div>);
});
