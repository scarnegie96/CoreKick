import { useEffect } from "react";
import { AppProvider, useApp, type Page } from "./lib/app";
import { Toasts } from "./components/ui";
import * as Icons from "./components/icons";
import Home from "./pages/Home";
import RideSetup from "./pages/RideSetup";
import LiveRide from "./pages/LiveRide";
import Routes, { RouteDetailPage, RouteBuilder } from "./pages/Routes";
import Workouts, { WorkoutEditor } from "./pages/Workouts";
import Events from "./pages/Events";
import Tests from "./pages/Tests";
import Activities, { ActivityDetail } from "./pages/Activities";
import Stats from "./pages/Stats";
import Settings from "./pages/Settings";
import Devices from "./pages/Devices";
import { DeviceChip } from "./components/DeviceChip";

const NAV: { page: Page; label: string; icon: (p: { size?: number }) => JSX.Element; match?: Page[] }[] = [
  { page: "home", label: "Home", icon: Icons.IconHome },
  { page: "ride", label: "Ride", icon: Icons.IconPlay },
  { page: "routes", label: "Routes", icon: Icons.IconRoute, match: ["route", "route-new"] },
  { page: "workouts", label: "Workouts", icon: Icons.IconWorkout, match: ["workout-edit"] },
  { page: "events", label: "Events", icon: Icons.IconCalendar },
  { page: "tests", label: "Tests", icon: Icons.IconTest },
  { page: "activities", label: "Activities", icon: Icons.IconList, match: ["activity"] },
  { page: "stats", label: "Stats", icon: Icons.IconChart },
];

function Shell() {
  const { nav, go, profile } = useApp();

  useEffect(() => {
    document.documentElement.dataset.page = nav.page;
  }, [nav.page]);

  if (nav.page === "live") {
    return (
      <>
        <LiveRide />
        <Toasts />
      </>
    );
  }

  let body: JSX.Element;
  switch (nav.page) {
    case "home": body = <Home />; break;
    case "ride": body = <RideSetup />; break;
    case "routes": body = <Routes />; break;
    case "route": body = <RouteDetailPage id={nav.params.id} />; break;
    case "route-new": body = <RouteBuilder initialTab={nav.params.tab} />; break;
    case "workouts": body = <Workouts />; break;
    case "workout-edit": body = <WorkoutEditor initial={nav.params.workout} />; break;
    case "events": body = <Events />; break;
    case "tests": body = <Tests />; break;
    case "activities": body = <Activities />; break;
    case "activity": body = <ActivityDetail id={nav.params.id} fresh={nav.params.fresh} />; break;
    case "stats": body = <Stats />; break;
    case "settings": body = <Settings />; break;
    case "devices": body = <Devices />; break;
    default: body = <Home />;
  }

  return (
    <div className="shell">
      <aside className="side">
        <div className="brand" data-tauri-drag-region>
          <img src="/icon.png" alt="" />
          <span>CoreKick</span>
        </div>
        <nav>
          {NAV.map((n) => {
            const active = nav.page === n.page || n.match?.includes(nav.page);
            const Icon = n.icon;
            return (
              <button key={n.page} className={`nav-i ${active ? "on" : ""}`} onClick={() => go(n.page)}>
                <Icon size={18} />
                <span>{n.label}</span>
              </button>
            );
          })}
        </nav>
        <div className="side-foot">
          <DeviceChip />
          <button className={`nav-i ${nav.page === "devices" ? "on" : ""}`} onClick={() => go("devices")}>
            <Icons.IconBluetooth size={18} />
            <span>Devices</span>
          </button>
          <button className={`nav-i ${nav.page === "settings" ? "on" : ""}`} onClick={() => go("settings")}>
            <Icons.IconUser size={18} />
            <span>{profile?.name || "Profile"}</span>
          </button>
        </div>
      </aside>
      <main className="main">
        <div className="drag-strip" data-tauri-drag-region />
        <div className="main-in">{body}</div>
      </main>
      <Toasts />
    </div>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
