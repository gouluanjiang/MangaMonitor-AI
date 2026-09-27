import { StrictMode, lazy, Suspense } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import "./workbench.css";

const App = lazy(() => import("./App.tsx"));
const ReaderWindow = lazy(() =>
  import("./reader/ReaderWindow.tsx").then((module) => ({
    default: module.ReaderWindow,
  })),
);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Suspense fallback={<div role="status">正在打开…</div>}>
      {window.location.hash === "#reader-window" ? <ReaderWindow /> : <App />}
    </Suspense>
  </StrictMode>,
);
