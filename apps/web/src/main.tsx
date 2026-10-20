import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import "./styles.css";
import { ToastProvider } from "./ui/kit.tsx";
import { AdminLayout } from "./admin/Layout.tsx";
import { Overview } from "./admin/Overview.tsx";
import { Creators } from "./admin/Creators.tsx";
import { CreatorDetail } from "./admin/CreatorDetail.tsx";
import { Accounts } from "./admin/Accounts.tsx";
import { Servers } from "./admin/Servers.tsx";
import { ServerDetail } from "./admin/ServerDetail.tsx";
import { PlayTogether } from "./admin/PlayTogether.tsx";
import { Smp } from "./admin/Smp.tsx";
import { Voices } from "./admin/Voices.tsx";
import { LiveAdmin } from "./admin/LiveAdmin.tsx";
import { System } from "./admin/System.tsx";
import { LiveLayout } from "./live/Layout.tsx";
import { LiveHome } from "./live/Home.tsx";
import { Channel } from "./live/Channel.tsx";
import { Multi } from "./live/Multi.tsx";
import { LiveAuth } from "./live/Auth.tsx";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ToastProvider>
      <BrowserRouter>
        <Routes>
          <Route path="/" element={<Navigate to="/admin" replace />} />
          <Route path="/admin" element={<AdminLayout />}>
            <Route index element={<Overview />} />
            <Route path="play" element={<PlayTogether />} />
            <Route path="creators" element={<Creators />} />
            <Route path="creators/:id" element={<CreatorDetail />} />
            <Route path="accounts" element={<Accounts />} />
            <Route path="servers" element={<Servers />} />
            <Route path="servers/:id" element={<ServerDetail />} />
            <Route path="smp" element={<Smp />} />
            <Route path="minelive" element={<LiveAdmin />} />
            <Route path="voices" element={<Voices />} />
            <Route path="system" element={<System />} />
          </Route>
          <Route path="/live" element={<LiveLayout />}>
            <Route index element={<LiveHome />} />
            <Route path="multi" element={<Multi />} />
            <Route path="login" element={<LiveAuth />} />
            <Route path="creator/:slug" element={<Channel />} />
          </Route>
          <Route path="*" element={<Navigate to="/admin" replace />} />
        </Routes>
      </BrowserRouter>
    </ToastProvider>
  </StrictMode>,
);

if ("serviceWorker" in navigator && location.protocol !== "file:") {
  window.addEventListener("load", () => navigator.serviceWorker.register("/sw.js").catch(() => {}));
}
