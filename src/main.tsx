import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import App from "./App";
import { DictionaryProvider } from "./i18n/DictionaryProvider";
import { ThemeProvider } from "./theme/ThemeProvider";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <DictionaryProvider>
        <App />
      </DictionaryProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
