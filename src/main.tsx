import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/global.css";
import "highlight.js/styles/github-dark.css";
import App from "./App";
import ToolbarWindow from "./features/toolbar/ToolbarWindow";

const isToolbar = new URLSearchParams(window.location.search).get("window") === "toolbar";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isToolbar ? <ToolbarWindow /> : <App />}</React.StrictMode>,
);
