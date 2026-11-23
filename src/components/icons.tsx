import type { SVGProps } from "react";

type P = SVGProps<SVGSVGElement> & { size?: number };

function I({ size = 18, children, ...rest }: P & { children: React.ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round" {...rest}>
      {children}
    </svg>
  );
}

export const IconHome = (p: P) => <I {...p}><path d="M3 11.5 12 4l9 7.5" /><path d="M5.5 10v9.5h13V10" /></I>;
export const IconPlay = (p: P) => <I {...p}><path d="M7 4.5v15l12.5-7.5z" fill="currentColor" stroke="none" /></I>;
export const IconPause = (p: P) => <I {...p}><rect x="6" y="5" width="4" height="14" rx="1" fill="currentColor" stroke="none" /><rect x="14" y="5" width="4" height="14" rx="1" fill="currentColor" stroke="none" /></I>;
export const IconStop = (p: P) => <I {...p}><rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor" stroke="none" /></I>;
export const IconRoute = (p: P) => <I {...p}><path d="M2.5 19 8 10l4 5 4-8 5.5 12z" /></I>;
export const IconWorkout = (p: P) => <I {...p}><path d="M3 20V13h3V8h3v12M9 20V5h3v15M12 20v-9h3v9M15 20v-6h3v6M18 20V9h3v11" /></I>;
export const IconCalendar = (p: P) => <I {...p}><rect x="3.5" y="5" width="17" height="15.5" rx="2.5" /><path d="M3.5 10h17M8 3v4M16 3v4" /></I>;
export const IconGauge = (p: P) => <I {...p}><path d="M4.5 17a8.5 8.5 0 1 1 15 0" /><path d="m12 13 4-4.5" /><circle cx="12" cy="13" r="1.3" fill="currentColor" /></I>;
export const IconList = (p: P) => <I {...p}><path d="M8 6h13M8 12h13M8 18h13" /><circle cx="3.5" cy="6" r="1" fill="currentColor" /><circle cx="3.5" cy="12" r="1" fill="currentColor" /><circle cx="3.5" cy="18" r="1" fill="currentColor" /></I>;
export const IconChart = (p: P) => <I {...p}><path d="M3 3v18h18" /><path d="m6 15 4-5 4 3 5-7" /></I>;
export const IconUser = (p: P) => <I {...p}><circle cx="12" cy="8" r="4" /><path d="M4 21c1.5-4 4.5-6 8-6s6.5 2 8 6" /></I>;
export const IconBluetooth = (p: P) => <I {...p}><path d="m7 7 10 10-5 4V3l5 4L7 17" /></I>;
export const IconPlus = (p: P) => <I {...p}><path d="M12 5v14M5 12h14" /></I>;
export const IconMinus = (p: P) => <I {...p}><path d="M5 12h14" /></I>;
export const IconTrash = (p: P) => <I {...p}><path d="M4 7h16M9 7V4.5h6V7M6 7l1 13h10l1-13" /></I>;
export const IconStar = ({ filled, ...p }: P & { filled?: boolean }) => <I {...p}><path d="m12 3.5 2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8L3.5 9.7l5.9-.8z" fill={filled ? "currentColor" : "none"} /></I>;
export const IconBack = (p: P) => <I {...p}><path d="M15 5 8 12l7 7" /></I>;
export const IconDice = (p: P) => <I {...p}><rect x="4" y="4" width="16" height="16" rx="3.5" /><circle cx="9" cy="9" r="1.2" fill="currentColor" /><circle cx="15" cy="15" r="1.2" fill="currentColor" /><circle cx="15" cy="9" r="1.2" fill="currentColor" /><circle cx="9" cy="15" r="1.2" fill="currentColor" /></I>;
export const IconUpload = (p: P) => <I {...p}><path d="M12 16V4M7 9l5-5 5 5M4 20h16" /></I>;
export const IconDownload = (p: P) => <I {...p}><path d="M12 4v12M7 11l5 5 5-5M4 20h16" /></I>;
export const IconSkip = (p: P) => <I {...p}><path d="M5 5v14l9-7zM18 5v14" /></I>;
export const IconBolt = (p: P) => <I {...p}><path d="M13 2.5 4.5 13.5H11l-1 8 8.5-11H12z" /></I>;
export const IconHeart = (p: P) => <I {...p}><path d="M12 20s-7.5-4.6-7.5-10A4.3 4.3 0 0 1 12 7.3 4.3 4.3 0 0 1 19.5 10c0 5.4-7.5 10-7.5 10z" /></I>;
export const IconFlag = (p: P) => <I {...p}><path d="M5 21V4M5 4h11l-2 4 2 4H5" /></I>;
export const IconTimer = (p: P) => <I {...p}><circle cx="12" cy="13.5" r="7.5" /><path d="M12 9.5v4l2.5 2M9.5 2.5h5" /></I>;
export const IconFlame = (p: P) => <I {...p}><path d="M12 21c4 0 6.5-2.6 6.5-6.3 0-4.6-4.3-6.8-4.3-11.2-3 1.6-4.4 4-4.2 7-1.4-.6-2.2-1.9-2.4-3.4C5.9 9 5.5 11.4 5.5 14.7 5.5 18.4 8 21 12 21z" /></I>;
export const IconMountain = (p: P) => <I {...p}><path d="m2 20 7-12 4 6 3-4 6 10z" /></I>;
export const IconCog = (p: P) => <I {...p}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1A2 2 0 1 1 4.3 17l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1A2 2 0 1 1 7 4.3l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1A2 2 0 1 1 19.7 7l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></I>;
export const IconGhost = (p: P) => <I {...p}><path d="M5 20V11a7 7 0 0 1 14 0v9l-2.3-1.6L14.3 20 12 18.4 9.7 20l-2.4-1.6z" /><circle cx="9.5" cy="11" r="1" fill="currentColor" /><circle cx="14.5" cy="11" r="1" fill="currentColor" /></I>;
export const IconRepeat = (p: P) => <I {...p}><path d="M17 2.5 20.5 6 17 9.5" /><path d="M3.5 11V9.5A3.5 3.5 0 0 1 7 6h13.5M7 21.5 3.5 18 7 14.5" /><path d="M20.5 13v1.5A3.5 3.5 0 0 1 17 18H3.5" /></I>;
export const IconEdit = (p: P) => <I {...p}><path d="M4 20h4L19 9l-4-4L4 16z" /><path d="m13.5 6.5 4 4" /></I>;
export const IconCopy = (p: P) => <I {...p}><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8" /></I>;
export const IconCheck = (p: P) => <I {...p}><path d="m5 12.5 4.5 4.5L19 7.5" /></I>;
export const IconX = (p: P) => <I {...p}><path d="M6 6l12 12M18 6 6 18" /></I>;
export const IconTrophy = (p: P) => <I {...p}><path d="M8 4h8v5a4 4 0 0 1-8 0zM8 6H4.5a3 3 0 0 0 3.5 4M16 6h3.5a3 3 0 0 1-3.5 4M12 13v4M8.5 20.5h7M9.5 17h5" /></I>;
export const IconTest = (p: P) => <I {...p}><path d="M4 20 9 9l4 6 3-4 4 9" /><path d="M14 3h6v6" /><path d="m20 3-6 6" /></I>;
export const IconBell = (p: P) => <I {...p}><path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15z" /><path d="M10 20.5a2 2 0 0 0 4 0" /></I>;
export const IconBellOff = (p: P) => <I {...p}><path d="M6 16V11a6 6 0 0 1 9.5-4.9M18 11v5l1.5 2H8" /><path d="M10 20.5a2 2 0 0 0 4 0M3 3l18 18" /></I>;
export const IconLink = (p: P) => <I {...p}><path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1" /><path d="M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" /></I>;
